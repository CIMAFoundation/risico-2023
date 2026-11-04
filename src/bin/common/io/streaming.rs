use std::fs::{File, OpenOptions};
use std::io;
use std::ops::Range;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use risico::models::output::{Output, OutputVariableName};

use crate::common::helpers::RISICOError;
use crate::common::io::static_data::geotiff::RasterGrid;

const FLOAT_BYTES: usize = std::mem::size_of::<f32>();

/// A rectangular, row-major window on a regular raster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridWindow {
    pub row: usize,
    pub col: usize,
    pub height: usize,
    pub width: usize,
}

impl GridWindow {
    pub fn end_row(self) -> usize {
        self.row + self.height
    }

    pub fn end_col(self) -> usize {
        self.col + self.width
    }

    pub fn len(self) -> usize {
        self.height * self.width
    }

    pub fn is_empty(self) -> bool {
        self.height == 0 || self.width == 0
    }

    pub fn contains_cell(self, cell_index: u32, grid_width: usize) -> bool {
        let index = cell_index as usize;
        let row = index / grid_width;
        let col = index % grid_width;
        (self.row..self.end_row()).contains(&row) && (self.col..self.end_col()).contains(&col)
    }
}

/// A unit of spatial work.
///
/// `model_positions` addresses the compact, active-cell arrays used by the
/// current model implementations. `grid_cell_indexes` addresses the native
/// regular raster and is empty for legacy point/cell configurations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialTile {
    pub ordinal: usize,
    pub window: Option<GridWindow>,
    pub model_positions: Vec<usize>,
    pub grid_cell_indexes: Vec<u32>,
}

impl SpatialTile {
    pub fn len(&self) -> usize {
        self.model_positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.model_positions.is_empty()
    }

    /// Fold another tile into this one.
    ///
    /// The window becomes the bounding box of both. Cells are kept in model
    /// order so a merged tile walks its arrays as sequentially as a plain one.
    fn absorb(&mut self, other: SpatialTile) {
        self.window = match (self.window, other.window) {
            (Some(mine), Some(theirs)) => {
                let row = mine.row.min(theirs.row);
                let col = mine.col.min(theirs.col);
                Some(GridWindow {
                    row,
                    col,
                    height: mine.end_row().max(theirs.end_row()) - row,
                    width: mine.end_col().max(theirs.end_col()) - col,
                })
            }
            (mine, theirs) => mine.or(theirs),
        };
        let mut cells: Vec<(usize, u32)> = self
            .model_positions
            .drain(..)
            .zip(self.grid_cell_indexes.drain(..))
            .chain(
                other
                    .model_positions
                    .into_iter()
                    .zip(other.grid_cell_indexes),
            )
            .collect();
        cells.sort_unstable_by_key(|(position, _)| *position);
        (self.model_positions, self.grid_cell_indexes) = cells.into_iter().unzip();
    }
}

/// A raster tile holding fewer than this fraction of a full window is merged
/// with a neighbour in its tile row.
///
/// Windows over an active-cell mask leave coastal and border tiles with a
/// handful of cells. Each costs a checkpoint file, an output scratch and a
/// coordinate registration per timestep however small it is, and none of
/// them can keep a thread busy. Merging stops once a tile reaches this size
/// and never produces a tile larger than a full window, so the largest tile,
/// which the memory budget is sized from, is unchanged.
const SMALL_TILE_DIVISOR: usize = 4;

/// Merge the small tiles of one tile row into their neighbours.
///
/// The smallest tile goes first, into whichever adjacent tile is smaller, as
/// long as the result stays within `max_len`. A tile with no neighbour that
/// has room is left as it is.
fn merge_small_tiles(
    mut row: Vec<SpatialTile>,
    min_len: usize,
    max_len: usize,
) -> Vec<SpatialTile> {
    let mut frozen = vec![false; row.len()];
    loop {
        let Some(index) = (0..row.len())
            .filter(|&index| !frozen[index] && row[index].len() < min_len)
            .min_by_key(|&index| row[index].len())
        else {
            return row;
        };
        let fits = |candidate: usize| row[candidate].len() + row[index].len() <= max_len;
        let left = index.checked_sub(1).filter(|&left| fits(left));
        let right = (index + 1 < row.len())
            .then_some(index + 1)
            .filter(|&right| fits(right));
        let partner = match (left, right) {
            (Some(left), Some(right)) if row[left].len() <= row[right].len() => left,
            (_, Some(right)) => right,
            (Some(left), None) => left,
            (None, None) => {
                frozen[index] = true;
                continue;
            }
        };
        let small = row.remove(index);
        frozen.remove(index);
        let partner = if partner > index {
            partner - 1
        } else {
            partner
        };
        row[partner].absorb(small);
        // A grown tile may now host another neighbour, so it is reconsidered.
        frozen[partner] = false;
    }
}

/// Build the same tile contract for raster-native and legacy point models.
#[derive(Clone, Debug)]
pub enum TilePlan {
    Raster {
        tiles: Vec<SpatialTile>,
        grid: RasterGrid,
    },
    Cells {
        tiles: Vec<SpatialTile>,
        cell_count: usize,
    },
}

impl TilePlan {
    pub fn raster(
        grid: RasterGrid,
        active_cell_indexes: &[u32],
        tile_height: usize,
        tile_width: usize,
    ) -> Result<Self, RISICOError> {
        if tile_height == 0 || tile_width == 0 {
            return Err("streaming tile dimensions must be greater than zero".into());
        }

        let grid_len = grid
            .width
            .checked_mul(grid.height)
            .ok_or("raster dimensions overflow usize")?;
        if let Some(index) = active_cell_indexes
            .iter()
            .find(|index| **index as usize >= grid_len)
        {
            return Err(format!("active cell index {index} is outside the raster grid").into());
        }

        let tile_cols = grid.width.div_ceil(tile_width);
        let tile_rows = grid.height.div_ceil(tile_height);
        let tile_count = tile_cols
            .checked_mul(tile_rows)
            .ok_or("tile count overflows usize")?;
        let mut positions = vec![Vec::new(); tile_count];
        let mut indexes = vec![Vec::new(); tile_count];

        for (model_position, &cell_index) in active_cell_indexes.iter().enumerate() {
            let cell = cell_index as usize;
            let row = cell / grid.width;
            let col = cell % grid.width;
            let tile_row = row / tile_height;
            let tile_col = col / tile_width;
            let tile_index = tile_row * tile_cols + tile_col;
            positions[tile_index].push(model_position);
            indexes[tile_index].push(cell_index);
        }

        let nominal = tile_height * tile_width;
        let mut tiles = Vec::new();
        for tile_row in 0..tile_rows {
            let mut row_tiles = Vec::new();
            for tile_col in 0..tile_cols {
                let source_index = tile_row * tile_cols + tile_col;
                if positions[source_index].is_empty() {
                    continue;
                }
                let row = tile_row * tile_height;
                let col = tile_col * tile_width;
                row_tiles.push(SpatialTile {
                    ordinal: 0,
                    window: Some(GridWindow {
                        row,
                        col,
                        height: tile_height.min(grid.height - row),
                        width: tile_width.min(grid.width - col),
                    }),
                    model_positions: std::mem::take(&mut positions[source_index]),
                    grid_cell_indexes: std::mem::take(&mut indexes[source_index]),
                });
            }
            tiles.extend(merge_small_tiles(
                row_tiles,
                nominal / SMALL_TILE_DIVISOR,
                nominal,
            ));
        }

        // Largest first: tiles are handed to a work-stealing pool, so the big
        // ones start early and the tail of each timestep, where threads wait
        // for the last tile, is spent on the small ones.
        tiles.sort_by_key(|tile| std::cmp::Reverse(tile.len()));
        for (ordinal, tile) in tiles.iter_mut().enumerate() {
            tile.ordinal = ordinal;
        }

        Ok(Self::Raster { tiles, grid })
    }

    pub fn cells(cell_count: usize, cells_per_tile: usize) -> Result<Self, RISICOError> {
        if cells_per_tile == 0 {
            return Err("streaming cells_per_tile must be greater than zero".into());
        }
        let tiles = (0..cell_count)
            .step_by(cells_per_tile)
            .enumerate()
            .map(|(ordinal, start)| {
                let end = (start + cells_per_tile).min(cell_count);
                SpatialTile {
                    ordinal,
                    window: None,
                    model_positions: (start..end).collect(),
                    grid_cell_indexes: Vec::new(),
                }
            })
            .collect();
        Ok(Self::Cells { tiles, cell_count })
    }

    pub fn tiles(&self) -> &[SpatialTile] {
        match self {
            Self::Raster { tiles, .. } | Self::Cells { tiles, .. } => tiles,
        }
    }

    pub fn cell_count(&self) -> usize {
        match self {
            Self::Raster { tiles, .. } => tiles.iter().map(SpatialTile::len).sum(),
            Self::Cells { cell_count, .. } => *cell_count,
        }
    }
}

/// A stable, little-endian, plane-oriented scratch file.
///
/// It deliberately does not map Rust model structs: model adapters can assign
/// fields to planes without depending on compiler struct layout.
///
/// Planes are moved with positioned I/O on one handle. A write is one
/// contiguous copy per plane and a read lands straight in the caller's `f32`
/// buffer, so there is no mapping to fault in page by page and no second
/// buffer to copy through. Nothing is ever synced: the file is consumed by
/// this process from the page cache and deleted once the timestep is joined,
/// so durability would only add latency. Positioned reads and writes leave
/// the shared file offset untouched, which is what makes them safe for the
/// concurrent readers that join a timestep.
pub struct F32PlaneFile {
    path: PathBuf,
    file: File,
    plane_count: usize,
    cell_count: usize,
}

/// Native-domain model output collected before any output-grid resampling.
///
/// Every enabled model produces the common `Output` type, so this boundary is
/// shared by all model adapters.
pub struct NativeOutputScratch {
    variables: Vec<OutputVariableName>,
    planes: F32PlaneFile,
}

impl NativeOutputScratch {
    pub fn create(
        path: impl AsRef<Path>,
        variables: Vec<OutputVariableName>,
        cell_count: usize,
    ) -> Result<Self, RISICOError> {
        if variables.is_empty() {
            return Err("native output scratch requires at least one variable".into());
        }
        let planes = F32PlaneFile::create(path, variables.len(), cell_count)?;
        Ok(Self { variables, planes })
    }

    /// Store a whole tile's output, one dense plane per variable.
    ///
    /// The scratch covers exactly this tile, so every plane is a contiguous
    /// copy rather than a scatter across the whole domain.
    pub fn write_output(&self, output: &Output) -> Result<(), RISICOError> {
        if output.data.len() != self.planes.cell_count() {
            return Err(format!(
                "tile output has {} cells for a {} cell scratch",
                output.data.len(),
                self.planes.cell_count()
            )
            .into());
        }
        for plane in 0..self.variables.len() {
            let variable = self.variables[plane];
            let values = output.get(&variable).ok_or_else(|| {
                RISICOError::from(format!("model output does not expose variable {variable}"))
            })?;
            self.planes.write_plane(
                plane,
                values.as_slice().expect("model output is contiguous"),
            )?;
        }
        Ok(())
    }

    fn has_variable(&self, variable: OutputVariableName) -> bool {
        self.variables.contains(&variable)
    }

    /// Read one variable's whole plane into `destination`.
    fn read_variable_into(
        &self,
        variable: OutputVariableName,
        destination: &mut Vec<f32>,
    ) -> Result<(), RISICOError> {
        let plane = self
            .variables
            .iter()
            .position(|candidate| *candidate == variable)
            .ok_or_else(|| {
                RISICOError::from(format!(
                    "native output scratch does not contain variable {variable}"
                ))
            })?;
        self.planes.read_plane_into(plane, destination)
    }

    pub fn path(&self) -> &Path {
        self.planes.path()
    }
}

/// One tile's output scratch together with the domain positions it covers.
pub struct TileNativeOutput<'a> {
    outputs: NativeOutputScratch,
    positions: &'a [usize],
}

impl<'a> TileNativeOutput<'a> {
    pub fn new(outputs: NativeOutputScratch, positions: &'a [usize]) -> Self {
        Self { outputs, positions }
    }

    pub fn path(&self) -> &Path {
        self.outputs.path()
    }
}

/// The whole domain's output for one timestep, held as one scratch per tile.
///
/// Keeping tiles separate lets each one write its own file densely and
/// without sharing a file with its neighbours. Consumers read it one tile at
/// a time, so a variable is never materialized for the whole domain at once.
pub struct TiledNativeOutputs<'a> {
    cell_count: usize,
    tiles: Vec<TileNativeOutput<'a>>,
}

impl<'a> TiledNativeOutputs<'a> {
    pub fn new(cell_count: usize, tiles: Vec<TileNativeOutput<'a>>) -> Result<Self, RISICOError> {
        let covered: usize = tiles.iter().map(|tile| tile.positions.len()).sum();
        if covered != cell_count {
            return Err(format!(
                "tile outputs cover {covered} cells but the domain has {cell_count}"
            )
            .into());
        }
        Ok(Self { cell_count, tiles })
    }

    pub fn cell_count(&self) -> usize {
        self.cell_count
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn has_variable(&self, variable: OutputVariableName) -> bool {
        self.tiles
            .first()
            .is_some_and(|tile| tile.outputs.has_variable(variable))
    }

    /// Read one tile's values for a variable, with the domain positions they
    /// belong to.
    ///
    /// The destination buffer is reused across tiles, so postprocessing a whole
    /// variable costs one tile, not one domain.
    pub fn read_tile(
        &self,
        tile: usize,
        variable: OutputVariableName,
        destination: &mut Vec<f32>,
    ) -> Result<&'a [usize], RISICOError> {
        let tile = self
            .tiles
            .get(tile)
            .ok_or_else(|| RISICOError::from(format!("tile {tile} is outside the timestep")))?;
        tile.outputs.read_variable_into(variable, destination)?;
        Ok(tile.positions)
    }
}

impl F32PlaneFile {
    pub fn create(
        path: impl AsRef<Path>,
        plane_count: usize,
        cell_count: usize,
    ) -> Result<Self, RISICOError> {
        if plane_count == 0 {
            return Err("plane scratch file requires at least one plane".into());
        }
        let value_count = plane_count
            .checked_mul(cell_count)
            .ok_or("plane scratch value count overflows usize")?;
        let byte_len = value_count
            .checked_mul(FLOAT_BYTES)
            .ok_or("plane scratch byte length overflows usize")?;
        let byte_len_u64 =
            u64::try_from(byte_len).map_err(|_| "plane scratch file is too large")?;
        if byte_len == 0 {
            return Err("plane scratch file cannot be empty".into());
        }

        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "cannot create plane scratch file {}: {error}",
                    path.display()
                )
            })?;
        // Sizing the file up front makes every plane addressable before it is
        // written, so a read of a plane that was never written is zeros rather
        // than a short read.
        file.set_len(byte_len_u64).map_err(|error| {
            format!("cannot size plane scratch file {}: {error}", path.display())
        })?;

        Ok(Self {
            path,
            file,
            plane_count,
            cell_count,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cell_count(&self) -> usize {
        self.cell_count
    }

    /// Overwrite a whole plane with one contiguous positioned write.
    pub fn write_plane(&self, plane: usize, values: &[f32]) -> Result<(), RISICOError> {
        let range = self.plane_byte_range(plane)?;
        if values.len() != self.cell_count {
            return Err(format!(
                "cannot write {} values into a {} cell plane",
                values.len(),
                self.cell_count
            )
            .into());
        }
        let write = |bytes: &[u8]| {
            self.file
                .write_all_at(bytes, range.start as u64)
                .map_err(|error| {
                    RISICOError::from(format!(
                        "cannot write plane scratch file {}: {error}",
                        self.path.display()
                    ))
                })
        };
        if cfg!(target_endian = "little") {
            // SAFETY: `f32` has no padding and no invalid bit patterns, so a
            // slice of them can always be read as bytes. On a little-endian
            // target those bytes are already the stored encoding, making the
            // whole plane a single write. Lengths were checked above.
            let source = unsafe {
                std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), values.len() * FLOAT_BYTES)
            };
            write(source)
        } else {
            let mut encoded = Vec::with_capacity(values.len() * FLOAT_BYTES);
            for value in values {
                encoded.extend_from_slice(&value.to_le_bytes());
            }
            write(&encoded)
        }
    }

    /// Read a whole plane into `destination`, which is resized to the plane.
    ///
    /// The buffer is the caller's, so a consumer that walks every tile of a
    /// timestep can hold one tile at a time instead of one domain. On a
    /// little-endian target the read lands directly in that buffer.
    pub fn read_plane_into(
        &self,
        plane: usize,
        destination: &mut Vec<f32>,
    ) -> Result<(), RISICOError> {
        let range = self.plane_byte_range(plane)?;
        let read_at = |buffer: &mut [u8]| {
            self.file
                .read_exact_at(buffer, range.start as u64)
                .map_err(|error| {
                    RISICOError::from(format!(
                        "cannot read plane scratch file {}: {error}",
                        self.path.display()
                    ))
                })
        };
        destination.clear();
        destination.resize(self.cell_count, 0.0);
        if cfg!(target_endian = "little") {
            // SAFETY: every bit pattern is a valid `f32` and the stored
            // encoding is the native one, so the read may fill the `f32`
            // buffer in place. The byte length is exactly the plane's.
            let buffer = unsafe {
                std::slice::from_raw_parts_mut(
                    destination.as_mut_ptr().cast::<u8>(),
                    range.end - range.start,
                )
            };
            read_at(buffer)
        } else {
            let mut encoded = vec![0_u8; range.end - range.start];
            read_at(&mut encoded)?;
            for (slot, bytes) in destination
                .iter_mut()
                .zip(encoded.chunks_exact(FLOAT_BYTES))
            {
                *slot = f32::from_le_bytes(
                    bytes.try_into().expect("f32 byte range has a fixed length"),
                );
            }
            Ok(())
        }
    }

    fn plane_byte_range(&self, plane: usize) -> Result<Range<usize>, RISICOError> {
        if plane >= self.plane_count {
            return Err(format!(
                "scratch plane {plane} is outside {} configured planes",
                self.plane_count
            )
            .into());
        }
        let len = self
            .cell_count
            .checked_mul(FLOAT_BYTES)
            .ok_or("scratch plane byte length overflows usize")?;
        let start = plane
            .checked_mul(len)
            .ok_or("scratch byte offset overflows usize")?;
        Ok(start..start + len)
    }
}

impl std::fmt::Debug for F32PlaneFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("F32PlaneFile")
            .field("path", &self.path)
            .field("plane_count", &self.plane_count)
            .field("cell_count", &self.cell_count)
            .finish()
    }
}

pub fn remove_scratch_file(path: impl AsRef<Path>) -> io::Result<()> {
    std::fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ndarray::Array1;
    use rayon::prelude::*;
    use risico::models::output::OutputElement;

    fn grid(width: usize, height: usize) -> RasterGrid {
        RasterGrid {
            width,
            height,
            epsg: 4326,
            transform: [10.0, 1.0, 0.0, 50.0, 0.0, -1.0],
        }
    }

    fn scratch_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "risico-streaming-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn raster_plan_groups_active_cells_into_spatial_squares() {
        let plan = TilePlan::raster(grid(5, 4), &[0, 1, 4, 5, 7, 13, 19], 2, 2).unwrap();
        let tiles = plan.tiles();
        assert_eq!(tiles.len(), 5);
        assert_eq!(
            tiles[0].window,
            Some(GridWindow {
                row: 0,
                col: 0,
                height: 2,
                width: 2
            })
        );
        assert_eq!(tiles[0].model_positions, vec![0, 1, 3]);
        assert_eq!(tiles[0].grid_cell_indexes, vec![0, 1, 5]);
        assert_eq!(plan.cell_count(), 7);
        assert!(tiles.iter().all(|tile| {
            tile.grid_cell_indexes
                .iter()
                .all(|index| tile.window.unwrap().contains_cell(*index, 5))
        }));
    }

    #[test]
    fn raster_plan_clips_edge_windows_and_skips_empty_tiles() {
        let plan = TilePlan::raster(grid(5, 3), &[4, 14], 2, 3).unwrap();
        let windows: Vec<_> = plan
            .tiles()
            .iter()
            .map(|tile| tile.window.unwrap())
            .collect();
        assert_eq!(
            windows,
            vec![
                GridWindow {
                    row: 0,
                    col: 3,
                    height: 2,
                    width: 2
                },
                GridWindow {
                    row: 2,
                    col: 3,
                    height: 1,
                    width: 2
                },
            ]
        );
    }

    #[test]
    fn raster_plan_orders_tiles_largest_first() {
        // Three windows holding 1, 3 and 2 cells; tiles of 1x1 cannot merge.
        let plan = TilePlan::raster(grid(3, 1), &[0, 1, 2], 1, 1).unwrap();
        assert_eq!(plan.tiles().len(), 3);
        let plan = TilePlan::raster(grid(6, 1), &[0, 1, 3, 4, 5], 1, 2).unwrap();
        let lens: Vec<_> = plan.tiles().iter().map(SpatialTile::len).collect();
        assert_eq!(lens, vec![2, 2, 1]);
        assert_eq!(
            plan.tiles().iter().map(|t| t.ordinal).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn raster_plan_merges_small_tiles_with_their_row_neighbours() {
        // A 12x4 grid in 4x4 windows: window 0 holds 2 cells, window 1 holds
        // 3, window 2 is full. Full is 16, so anything under 4 is small.
        let mut cells: Vec<u32> = vec![0, 13, 4, 5, 18];
        cells.extend((0..4).flat_map(|row| (8..12).map(move |col| row * 12 + col)));
        cells.sort_unstable();
        let plan = TilePlan::raster(grid(12, 4), &cells, 4, 4).unwrap();
        let tiles = plan.tiles();
        assert_eq!(tiles.len(), 2, "the two small windows merge into one tile");
        assert_eq!(tiles[0].len(), 16);
        let merged = &tiles[1];
        assert_eq!(merged.len(), 5);
        assert_eq!(
            merged.window,
            Some(GridWindow {
                row: 0,
                col: 0,
                height: 4,
                width: 8
            })
        );
        assert_eq!(merged.grid_cell_indexes, vec![0, 4, 5, 13, 18]);
        assert!(merged
            .model_positions
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        assert_eq!(plan.cell_count(), cells.len());
    }

    #[test]
    fn raster_plan_never_merges_past_a_full_window() {
        // A 1-cell window beside a full one has nowhere to go.
        let mut cells: Vec<u32> = vec![0];
        cells.extend((0..4).flat_map(|row| (4..8).map(move |col| row * 8 + col)));
        cells.sort_unstable();
        let plan = TilePlan::raster(grid(8, 4), &cells, 4, 4).unwrap();
        let lens: Vec<_> = plan.tiles().iter().map(SpatialTile::len).collect();
        assert_eq!(lens, vec![16, 1]);
    }

    #[test]
    fn small_tiles_merge_into_the_smaller_neighbour_and_keep_growing() {
        let tile = |len: usize, col: usize| SpatialTile {
            ordinal: 0,
            window: Some(GridWindow {
                row: 0,
                col,
                height: 1,
                width: 1,
            }),
            model_positions: (col * 100..col * 100 + len).collect(),
            grid_cell_indexes: (col * 100..col * 100 + len).map(|i| i as u32).collect(),
        };
        // Sizes 6, 1, 2, 9 with min 4 and max 10: the 1 joins the 2 (its
        // smaller neighbour), the 3 then joins the 6, and the 9 is untouched.
        let merged = merge_small_tiles(vec![tile(6, 0), tile(1, 1), tile(2, 2), tile(9, 3)], 4, 10);
        let lens: Vec<_> = merged.iter().map(SpatialTile::len).collect();
        assert_eq!(lens, vec![9, 9]);
        assert_eq!(merged[0].window.unwrap().width, 3);
    }

    #[test]
    fn cell_plan_covers_every_legacy_model_position_once() {
        let plan = TilePlan::cells(7, 3).unwrap();
        assert_eq!(
            plan.tiles()
                .iter()
                .flat_map(|tile| tile.model_positions.iter().copied())
                .collect::<Vec<_>>(),
            (0..7).collect::<Vec<_>>()
        );
        assert_eq!(plan.tiles().last().unwrap().len(), 1);
    }

    #[test]
    fn planes_roundtrip_dense_writes_through_a_read() {
        let path = scratch_path("planes");
        let planes = F32PlaneFile::create(&path, 2, 3).unwrap();
        planes.write_plane(0, &[10.0, 40.0, 70.0]).unwrap();
        planes.write_plane(1, &[1.5, 2.5, 6.5]).unwrap();

        // Planes stay independent, and reading one reuses the caller's buffer.
        let mut buffer = vec![9.0_f32; 17];
        planes.read_plane_into(0, &mut buffer).unwrap();
        assert_eq!(buffer, vec![10.0, 40.0, 70.0]);

        planes.read_plane_into(1, &mut buffer).unwrap();
        assert_eq!(buffer, vec![1.5, 2.5, 6.5]);

        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn every_bit_pattern_survives_the_roundtrip() {
        let path = scratch_path("planes-patterns");
        let planes = F32PlaneFile::create(&path, 1, 6).unwrap();
        let values = [1.0, -2.5, 3.25, f32::MAX, f32::MIN_POSITIVE, -0.0];
        planes.write_plane(0, &values).unwrap();

        let mut buffer = Vec::new();
        planes.read_plane_into(0, &mut buffer).unwrap();
        assert_eq!(
            buffer.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            values.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );

        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn a_plane_that_was_never_written_reads_as_zeros() {
        let path = scratch_path("planes-unwritten");
        let planes = F32PlaneFile::create(&path, 2, 3).unwrap();
        planes.write_plane(1, &[1.0, 2.0, 3.0]).unwrap();

        let mut buffer = Vec::new();
        planes.read_plane_into(0, &mut buffer).unwrap();
        assert_eq!(buffer, vec![0.0, 0.0, 0.0]);
        assert!(planes.read_plane_into(2, &mut buffer).is_err());

        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn planes_read_back_correctly_under_concurrent_reads() {
        // Output variables are joined in parallel, so many planes of one
        // scratch are read at once. A seek-based read would race on the
        // shared file offset; the positioned read must not. Enough planes and
        // cells that a mistaken offset would corrupt the result.
        let plane_count = 16;
        let cell_count = 4096;
        let path = scratch_path("planes-concurrent");
        let planes = F32PlaneFile::create(&path, plane_count, cell_count).unwrap();
        for plane in 0..plane_count {
            let values: Vec<f32> = (0..cell_count)
                .map(|cell| (plane * cell_count + cell) as f32)
                .collect();
            planes.write_plane(plane, &values).unwrap();
        }

        let mismatches: usize = (0..plane_count)
            .into_par_iter()
            .map(|plane| {
                let mut buffer = Vec::new();
                planes.read_plane_into(plane, &mut buffer).unwrap();
                buffer
                    .iter()
                    .enumerate()
                    .filter(|(cell, &value)| value != (plane * cell_count + cell) as f32)
                    .count()
            })
            .sum();
        assert_eq!(mismatches, 0, "a concurrent read landed on the wrong plane");

        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn dense_plane_writes_reject_a_length_mismatch() {
        let path = scratch_path("planes-mismatch");
        let planes = F32PlaneFile::create(&path, 1, 3).unwrap();
        assert!(planes.write_plane(0, &[1.0, 2.0]).is_err());
        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn per_tile_scratches_read_back_with_their_domain_positions() {
        let variables = vec![OutputVariableName::temperature, OutputVariableName::rain];
        let element = |temperature: f32, rain: f32| OutputElement {
            temperature,
            rain,
            ..OutputElement::default()
        };
        let time = Utc.with_ymd_and_hms(2026, 7, 23, 12, 0, 0).unwrap();

        // Two tiles interleaved across a six cell domain, as a raster tiling
        // produces: each tile owns increasing but non-contiguous positions.
        let layout: [(&str, Vec<usize>, Vec<(f32, f32)>); 2] = [
            (
                "a",
                vec![0, 2, 4],
                vec![(1.0, 10.0), (3.0, 30.0), (5.0, 50.0)],
            ),
            (
                "b",
                vec![1, 3, 5],
                vec![(2.0, 20.0), (4.0, 40.0), (6.0, 60.0)],
            ),
        ];

        let mut paths = Vec::new();
        let mut stores = Vec::new();
        for (name, _, values) in &layout {
            let path = scratch_path(&format!("join-{name}"));
            let store = NativeOutputScratch::create(&path, variables.clone(), 3).unwrap();
            store
                .write_output(&Output::new(
                    time,
                    Array1::from_vec(
                        values
                            .iter()
                            .map(|(temperature, rain)| element(*temperature, *rain))
                            .collect::<Vec<_>>(),
                    ),
                ))
                .unwrap();
            paths.push(path);
            stores.push(store);
        }

        let tile_views: Vec<TileNativeOutput> = stores
            .into_iter()
            .zip(&layout)
            .map(|(store, (_, positions, _))| TileNativeOutput::new(store, positions.as_slice()))
            .collect();

        let joined = TiledNativeOutputs::new(6, tile_views).unwrap();
        assert!(joined.has_variable(OutputVariableName::temperature));
        assert!(!joined.has_variable(OutputVariableName::dffm));

        // Walking the tiles reconstructs the domain without ever holding it.
        let mut buffer = Vec::new();
        for variable in [OutputVariableName::temperature, OutputVariableName::rain] {
            let mut domain = vec![0.0_f32; 6];
            for tile in 0..joined.tile_count() {
                let positions = joined.read_tile(tile, variable, &mut buffer).unwrap();
                assert_eq!(buffer.len(), positions.len());
                for (&value, &position) in buffer.iter().zip(positions) {
                    domain[position] = value;
                }
            }
            let expected = match variable {
                OutputVariableName::temperature => vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                _ => vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
            };
            assert_eq!(domain, expected);
        }

        drop(joined);
        for path in paths {
            remove_scratch_file(path).unwrap();
        }
    }

    #[test]
    fn joining_rejects_tiles_that_do_not_cover_the_domain() {
        let path = scratch_path("join-partial");
        let store =
            NativeOutputScratch::create(&path, vec![OutputVariableName::temperature], 2).unwrap();
        let positions = vec![0_usize, 1];
        let tile = TileNativeOutput::new(store, &positions);

        let error = TiledNativeOutputs::new(5, vec![tile]);
        assert!(error.is_err(), "a partial cover must not be joinable");

        remove_scratch_file(path).unwrap();
    }
}
