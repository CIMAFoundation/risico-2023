use std::fs::{File, OpenOptions};
use std::io;
use std::ops::Range;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use memmap2::{MmapMut, MmapOptions};
use ndarray::Array1;
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

        let mut tiles = Vec::new();
        for tile_row in 0..tile_rows {
            for tile_col in 0..tile_cols {
                let source_index = tile_row * tile_cols + tile_col;
                if positions[source_index].is_empty() {
                    continue;
                }
                let row = tile_row * tile_height;
                let col = tile_col * tile_width;
                tiles.push(SpatialTile {
                    ordinal: tiles.len(),
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
/// fields to planes without depending on compiler struct layout. The operating
/// system controls which pages are resident.
pub struct MappedF32Planes {
    path: PathBuf,
    file: File,
    /// `None` once the scratch has been released; planes are then read back
    /// from the file on demand.
    map: Option<MmapMut>,
    plane_count: usize,
    cell_count: usize,
}

/// Native-domain model output collected before any output-grid resampling.
///
/// Every enabled model produces the common `Output` type, so this boundary is
/// shared by all model adapters.
pub struct MappedNativeOutputs {
    variables: Vec<OutputVariableName>,
    planes: MappedF32Planes,
}

impl MappedNativeOutputs {
    pub fn create(
        path: impl AsRef<Path>,
        variables: Vec<OutputVariableName>,
        cell_count: usize,
    ) -> Result<Self, RISICOError> {
        if variables.is_empty() {
            return Err("native output scratch requires at least one variable".into());
        }
        let planes = MappedF32Planes::create(path, variables.len(), cell_count)?;
        Ok(Self { variables, planes })
    }

    /// Store a whole tile's output, one dense plane per variable.
    ///
    /// The scratch covers exactly this tile, so every plane is a contiguous
    /// copy rather than a scatter across the whole domain.
    pub fn write_output(&mut self, output: &Output) -> Result<(), RISICOError> {
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

    fn scatter_plane(
        &self,
        variable: OutputVariableName,
        positions: &[usize],
        destination: &mut [f32],
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
        self.planes.scatter_plane(plane, positions, destination)
    }

    pub fn sync_all(&self) -> Result<(), RISICOError> {
        self.planes.sync_all()
    }

    /// Write this tile's output back to disk and drop it from memory.
    pub fn release_pages(&mut self) -> Result<(), RISICOError> {
        self.planes.release_pages()
    }

    pub fn path(&self) -> &Path {
        self.planes.path()
    }
}

/// One tile's output scratch together with the domain positions it covers.
pub struct TileNativeOutput<'a> {
    outputs: MappedNativeOutputs,
    positions: &'a [usize],
}

impl<'a> TileNativeOutput<'a> {
    pub fn new(outputs: MappedNativeOutputs, positions: &'a [usize]) -> Self {
        Self { outputs, positions }
    }

    pub fn path(&self) -> &Path {
        self.outputs.path()
    }
}

/// The whole domain's output for one timestep, held as one scratch per tile.
///
/// Keeping tiles separate lets each one write its own mapping densely and
/// without sharing a mapping with its neighbours. The domain is reassembled a
/// single variable at a time, so only one plane is ever materialized.
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

    /// Reassemble one variable across every tile into a domain-ordered array.
    pub fn join_variable(&self, variable: OutputVariableName) -> Result<Array1<f32>, RISICOError> {
        let mut joined = vec![0.0_f32; self.cell_count];
        for tile in &self.tiles {
            tile.outputs
                .scatter_plane(variable, tile.positions, &mut joined)?;
        }
        Ok(Array1::from(joined))
    }

    pub fn sync_all(&self) -> Result<(), RISICOError> {
        for tile in &self.tiles {
            tile.outputs.sync_all()?;
        }
        Ok(())
    }
}

impl MappedF32Planes {
    pub fn create(
        path: impl AsRef<Path>,
        plane_count: usize,
        cell_count: usize,
    ) -> Result<Self, RISICOError> {
        if plane_count == 0 {
            return Err("mapped scratch file requires at least one plane".into());
        }
        let value_count = plane_count
            .checked_mul(cell_count)
            .ok_or("mapped scratch value count overflows usize")?;
        let byte_len = value_count
            .checked_mul(FLOAT_BYTES)
            .ok_or("mapped scratch byte length overflows usize")?;
        let byte_len_u64 =
            u64::try_from(byte_len).map_err(|_| "mapped scratch file is too large")?;
        if byte_len == 0 {
            return Err("mapped scratch file cannot be empty".into());
        }

        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "cannot create mapped scratch file {}: {error}",
                    path.display()
                )
            })?;
        file.set_len(byte_len_u64).map_err(|error| {
            format!(
                "cannot size mapped scratch file {}: {error}",
                path.display()
            )
        })?;

        // SAFETY: the file is held by this object for at least as long as the
        // mapping, its length is fixed before mapping, and this type is the
        // only mutable mapping created by this constructor.
        let map = unsafe { MmapOptions::new().len(byte_len).map_mut(&file) }.map_err(|error| {
            format!("cannot memory-map scratch file {}: {error}", path.display())
        })?;

        Ok(Self {
            path,
            file,
            map: Some(map),
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

    /// Overwrite a whole plane with one contiguous copy.
    pub fn write_plane(&mut self, plane: usize, values: &[f32]) -> Result<(), RISICOError> {
        self.validate_plane(plane)?;
        if values.len() != self.cell_count {
            return Err(format!(
                "cannot write {} values into a {} cell plane",
                values.len(),
                self.cell_count
            )
            .into());
        }
        let range = self.plane_byte_range(plane)?;
        let map = self
            .map
            .as_mut()
            .ok_or_else(|| RISICOError::from("cannot write to a released scratch"))?;
        let destination = &mut map[range];
        if cfg!(target_endian = "little") {
            // SAFETY: `f32` has no padding and no invalid bit patterns, so a
            // slice of them can always be read as bytes. On a little-endian
            // target those bytes are already the stored encoding, making the
            // whole plane a single copy. Lengths were checked above.
            let source = unsafe {
                std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), values.len() * FLOAT_BYTES)
            };
            destination.copy_from_slice(source);
        } else {
            for (slot, value) in destination.chunks_exact_mut(FLOAT_BYTES).zip(values) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
        }
        Ok(())
    }

    /// Scatter a plane into `destination` at the given positions.
    ///
    /// Reads the plane in storage order and needs no intermediate buffer, so
    /// joining many tile scratches into one domain array stays allocation-free.
    pub fn scatter_plane(
        &self,
        plane: usize,
        positions: &[usize],
        destination: &mut [f32],
    ) -> Result<(), RISICOError> {
        self.validate_plane(plane)?;
        if positions.len() != self.cell_count {
            return Err(format!(
                "cannot scatter a {} cell plane through {} positions",
                self.cell_count,
                positions.len()
            )
            .into());
        }
        let domain_len = destination.len();
        self.with_plane_bytes(plane, |stored| {
            // SAFETY: the plane starts at a multiple of four bytes inside a
            // page-aligned mapping, so it is `f32` aligned; `align_to` still
            // reports any mismatch and the byte-wise path below covers it.
            let (prefix, floats, suffix) = unsafe { stored.align_to::<f32>() };
            if cfg!(target_endian = "little") && prefix.is_empty() && suffix.is_empty() {
                for (&value, &position) in floats.iter().zip(positions) {
                    let slot = destination.get_mut(position).ok_or_else(|| {
                        RISICOError::from(format!(
                            "scatter position {position} is outside a {domain_len} cell domain"
                        ))
                    })?;
                    *slot = value;
                }
                return Ok(());
            }

            for (bytes, &position) in stored.chunks_exact(FLOAT_BYTES).zip(positions) {
                let slot = destination.get_mut(position).ok_or_else(|| {
                    RISICOError::from(format!(
                        "scatter position {position} is outside a {domain_len} cell domain"
                    ))
                })?;
                *slot = f32::from_le_bytes(
                    bytes.try_into().expect("f32 byte range has a fixed length"),
                );
            }
            Ok(())
        })
    }

    pub fn flush(&self) -> Result<(), RISICOError> {
        // A released scratch is already on disk, so there is nothing to flush.
        let Some(map) = self.map.as_ref() else {
            return Ok(());
        };
        map.flush().map_err(|error| {
            format!(
                "cannot flush mapped scratch file {}: {error}",
                self.path.display()
            )
            .into()
        })
    }

    pub fn sync_all(&self) -> Result<(), RISICOError> {
        self.flush()?;
        self.file.sync_all().map_err(|error| {
            format!(
                "cannot sync mapped scratch file {}: {error}",
                self.path.display()
            )
            .into()
        })
    }

    /// Write the scratch back to disk and unmap it.
    ///
    /// Every tile's scratch stays alive until the whole timestep is joined, so
    /// without this the resident set grows by the size of the entire domain's
    /// output over a timestep however few tiles run at once. Unmapping trades
    /// that for reading the plane back from the file at join time.
    ///
    /// This unmaps rather than advising the pages away because `MADV_DONTNEED`
    /// is honoured on Linux but is close to a no-op for shared file mappings on
    /// macOS, where it measurably freed nothing.
    pub fn release_pages(&mut self) -> Result<(), RISICOError> {
        let Some(map) = self.map.as_ref() else {
            return Ok(());
        };
        // Flushing first is what makes the unmap lossless: it forces the dirty
        // pages out to the file that the join will read them back from.
        map.flush().map_err(|error| {
            format!(
                "cannot flush mapped scratch file {}: {error}",
                self.path.display()
            )
        })?;
        self.map = None;
        Ok(())
    }

    /// Read one plane's bytes, whether or not the scratch is still mapped.
    fn with_plane_bytes<T>(
        &self,
        plane: usize,
        read: impl FnOnce(&[u8]) -> Result<T, RISICOError>,
    ) -> Result<T, RISICOError> {
        let range = self.plane_byte_range(plane)?;
        match self.map.as_ref() {
            Some(map) => read(&map[range]),
            None => {
                // Released scratches are read back a plane at a time, which is
                // the point: one tile-sized buffer instead of the whole domain.
                //
                // Planes are joined in parallel, so this reads at an absolute
                // offset rather than seeking: a positioned read leaves the
                // shared file offset untouched and is safe for concurrent
                // readers on the one handle. A cloned handle would not be, since
                // the clone shares that offset.
                let mut buffer = vec![0_u8; range.end - range.start];
                self.file
                    .read_exact_at(&mut buffer, range.start as u64)
                    .map_err(|error| {
                        format!(
                            "cannot read released scratch file {}: {error}",
                            self.path.display()
                        )
                    })?;
                read(&buffer)
            }
        }
    }

    fn validate_plane(&self, plane: usize) -> Result<(), RISICOError> {
        if plane >= self.plane_count {
            Err(format!(
                "scratch plane {plane} is outside {} configured planes",
                self.plane_count
            )
            .into())
        } else {
            Ok(())
        }
    }

    fn plane_byte_range(&self, plane: usize) -> Result<Range<usize>, RISICOError> {
        let start = self.value_byte_offset(plane, 0)?;
        let len = self
            .cell_count
            .checked_mul(FLOAT_BYTES)
            .ok_or("scratch plane byte length overflows usize")?;
        Ok(start..start + len)
    }

    fn value_byte_offset(&self, plane: usize, cell: usize) -> Result<usize, RISICOError> {
        if cell >= self.cell_count {
            return Err(format!(
                "scratch cell {cell} is outside {} configured cells",
                self.cell_count
            )
            .into());
        }
        let value_index = plane
            .checked_mul(self.cell_count)
            .and_then(|offset| offset.checked_add(cell))
            .ok_or("scratch value offset overflows usize")?;
        value_index
            .checked_mul(FLOAT_BYTES)
            .ok_or_else(|| "scratch byte offset overflows usize".into())
    }
}

impl std::fmt::Debug for MappedF32Planes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MappedF32Planes")
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
    use rayon::prelude::*;
    use chrono::{TimeZone, Utc};
    use ndarray::Array1;
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
    fn mapped_planes_roundtrip_dense_writes_through_a_scatter() {
        let path = scratch_path("planes");
        let mut planes = MappedF32Planes::create(&path, 2, 3).unwrap();
        planes.write_plane(0, &[10.0, 40.0, 70.0]).unwrap();
        planes.write_plane(1, &[1.5, 2.5, 6.5]).unwrap();

        // Planes stay independent and land at the requested domain positions.
        let mut domain = vec![0.0_f32; 8];
        planes.scatter_plane(0, &[1, 4, 7], &mut domain).unwrap();
        assert_eq!(domain, vec![0.0, 10.0, 0.0, 0.0, 40.0, 0.0, 0.0, 70.0]);

        let mut domain = vec![0.0_f32; 8];
        planes.scatter_plane(1, &[0, 2, 6], &mut domain).unwrap();
        assert_eq!(domain, vec![1.5, 0.0, 2.5, 0.0, 0.0, 0.0, 6.5, 0.0]);

        planes.sync_all().unwrap();
        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn releasing_pages_keeps_the_written_values_readable() {
        let path = scratch_path("planes-released");
        let mut planes = MappedF32Planes::create(&path, 2, 4).unwrap();
        planes.write_plane(0, &[1.0, -2.5, 3.25, f32::MAX]).unwrap();
        planes.write_plane(1, &[0.0, 0.125, -7.5, 1e-8]).unwrap();

        // Dropping the pages must cost nothing but a page fault on the way back.
        planes.release_pages().unwrap();

        let mut domain = vec![0.0_f32; 4];
        planes.scatter_plane(0, &[0, 1, 2, 3], &mut domain).unwrap();
        assert_eq!(domain, vec![1.0, -2.5, 3.25, f32::MAX]);

        let mut domain = vec![0.0_f32; 4];
        planes.scatter_plane(1, &[0, 1, 2, 3], &mut domain).unwrap();
        assert_eq!(domain, vec![0.0, 0.125, -7.5, 1e-8]);

        // Releasing twice, and after reading back, stays harmless.
        planes.release_pages().unwrap();
        planes.release_pages().unwrap();
        let mut domain = vec![0.0_f32; 4];
        planes.scatter_plane(0, &[0, 1, 2, 3], &mut domain).unwrap();
        assert_eq!(domain, vec![1.0, -2.5, 3.25, f32::MAX]);

        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn released_planes_read_back_correctly_under_concurrent_scatter() {
        // Output variables are joined in parallel, so many planes of one
        // released scratch are read at once. A seek-based read would race on
        // the shared file offset; the positioned read must not. Enough planes
        // and cells that a mistaken offset would corrupt the result.
        let plane_count = 16;
        let cell_count = 4096;
        let path = scratch_path("planes-concurrent");
        let mut planes = MappedF32Planes::create(&path, plane_count, cell_count).unwrap();
        for plane in 0..plane_count {
            let values: Vec<f32> = (0..cell_count)
                .map(|cell| (plane * cell_count + cell) as f32)
                .collect();
            planes.write_plane(plane, &values).unwrap();
        }
        planes.release_pages().unwrap();

        let positions: Vec<usize> = (0..cell_count).collect();
        let mismatches: usize = (0..plane_count)
            .into_par_iter()
            .map(|plane| {
                let mut domain = vec![f32::NAN; cell_count];
                planes.scatter_plane(plane, &positions, &mut domain).unwrap();
                domain
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
        let mut planes = MappedF32Planes::create(&path, 1, 3).unwrap();
        assert!(planes.write_plane(0, &[1.0, 2.0]).is_err());
        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn per_tile_scratches_rejoin_into_domain_order() {
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
            let mut store = MappedNativeOutputs::create(&path, variables.clone(), 3).unwrap();
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
        assert_eq!(
            joined
                .join_variable(OutputVariableName::temperature)
                .unwrap()
                .to_vec(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        );
        assert_eq!(
            joined
                .join_variable(OutputVariableName::rain)
                .unwrap()
                .to_vec(),
            vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0]
        );

        drop(joined);
        for path in paths {
            remove_scratch_file(path).unwrap();
        }
    }

    #[test]
    fn joining_rejects_tiles_that_do_not_cover_the_domain() {
        let path = scratch_path("join-partial");
        let store =
            MappedNativeOutputs::create(&path, vec![OutputVariableName::temperature], 2).unwrap();
        let positions = vec![0_usize, 1];
        let tile = TileNativeOutput::new(store, &positions);

        let error = TiledNativeOutputs::new(5, vec![tile]);
        assert!(error.is_err(), "a partial cover must not be joinable");

        remove_scratch_file(path).unwrap();
    }
}
