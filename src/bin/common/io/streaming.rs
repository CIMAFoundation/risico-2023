use std::fs::{File, OpenOptions};
use std::io;
use std::ops::Range;
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
    map: MmapMut,
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

    pub fn variables(&self) -> &[OutputVariableName] {
        &self.variables
    }

    pub fn write_tile(
        &mut self,
        model_positions: &[usize],
        output: &Output,
    ) -> Result<(), RISICOError> {
        if output.data.len() != model_positions.len() {
            return Err(format!(
                "tile output has {} cells for {} model positions",
                output.data.len(),
                model_positions.len()
            )
            .into());
        }
        for (plane, variable) in self.variables.iter().enumerate() {
            let values = output.get(variable).ok_or_else(|| {
                RISICOError::from(format!("model output does not expose variable {variable}"))
            })?;
            self.planes.write_indexed(
                plane,
                model_positions,
                values.as_slice().expect("model output is contiguous"),
            )?;
        }
        Ok(())
    }

    pub fn read_variable(
        &self,
        variable: OutputVariableName,
        model_positions: &[usize],
    ) -> Result<Vec<f32>, RISICOError> {
        let plane = self
            .variables
            .iter()
            .position(|candidate| *candidate == variable)
            .ok_or_else(|| {
                RISICOError::from(format!(
                    "native output scratch does not contain variable {variable}"
                ))
            })?;
        self.planes.read_indexed(plane, model_positions)
    }

    pub fn read_variable_all(
        &self,
        variable: OutputVariableName,
    ) -> Result<Array1<f32>, RISICOError> {
        let plane = self
            .variables
            .iter()
            .position(|candidate| *candidate == variable)
            .ok_or_else(|| {
                RISICOError::from(format!(
                    "native output scratch does not contain variable {variable}"
                ))
            })?;
        Ok(Array1::from(self.planes.read_plane(plane)?))
    }

    pub fn sync_all(&self) -> Result<(), RISICOError> {
        self.planes.sync_all()
    }

    pub fn path(&self) -> &Path {
        self.planes.path()
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
            map,
            plane_count,
            cell_count,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn plane_count(&self) -> usize {
        self.plane_count
    }

    pub fn cell_count(&self) -> usize {
        self.cell_count
    }

    pub fn fill_plane(&mut self, plane: usize, value: f32) -> Result<(), RISICOError> {
        self.validate_plane(plane)?;
        let bytes = value.to_le_bytes();
        let range = self.plane_byte_range(plane)?;
        self.map[range]
            .chunks_exact_mut(FLOAT_BYTES)
            .for_each(|destination| destination.copy_from_slice(&bytes));
        Ok(())
    }

    pub fn write_indexed(
        &mut self,
        plane: usize,
        indexes: &[usize],
        values: &[f32],
    ) -> Result<(), RISICOError> {
        self.validate_plane(plane)?;
        if indexes.len() != values.len() {
            return Err(format!(
                "cannot write {} indexes with {} values",
                indexes.len(),
                values.len()
            )
            .into());
        }
        for (&index, &value) in indexes.iter().zip(values) {
            let offset = self.value_byte_offset(plane, index)?;
            self.map[offset..offset + FLOAT_BYTES].copy_from_slice(&value.to_le_bytes());
        }
        Ok(())
    }

    pub fn read_indexed(&self, plane: usize, indexes: &[usize]) -> Result<Vec<f32>, RISICOError> {
        self.validate_plane(plane)?;
        indexes
            .iter()
            .map(|&index| {
                let offset = self.value_byte_offset(plane, index)?;
                let bytes: [u8; FLOAT_BYTES] = self.map[offset..offset + FLOAT_BYTES]
                    .try_into()
                    .expect("f32 byte range has a fixed length");
                Ok(f32::from_le_bytes(bytes))
            })
            .collect()
    }

    pub fn read_plane(&self, plane: usize) -> Result<Vec<f32>, RISICOError> {
        self.validate_plane(plane)?;
        let range = self.plane_byte_range(plane)?;
        Ok(self.map[range]
            .chunks_exact(FLOAT_BYTES)
            .map(|bytes| {
                f32::from_le_bytes(bytes.try_into().expect("f32 byte range has a fixed length"))
            })
            .collect())
    }

    pub fn flush(&self) -> Result<(), RISICOError> {
        self.map.flush().map_err(|error| {
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
    fn mapped_planes_roundtrip_disjoint_tiles() {
        let path = scratch_path("planes");
        let mut planes = MappedF32Planes::create(&path, 2, 8).unwrap();
        planes.fill_plane(0, -9999.0).unwrap();
        planes
            .write_indexed(0, &[1, 4, 7], &[10.0, 40.0, 70.0])
            .unwrap();
        planes
            .write_indexed(1, &[0, 2, 6], &[1.5, 2.5, 6.5])
            .unwrap();
        assert_eq!(
            planes.read_indexed(0, &[0, 1, 4, 7]).unwrap(),
            vec![-9999.0, 10.0, 40.0, 70.0]
        );
        assert_eq!(
            planes.read_indexed(1, &[0, 2, 6]).unwrap(),
            vec![1.5, 2.5, 6.5]
        );
        planes.sync_all().unwrap();
        drop(planes);
        remove_scratch_file(path).unwrap();
    }

    #[test]
    fn native_output_store_accepts_the_common_output_of_any_model_tile() {
        let path = scratch_path("native-output");
        let mut store = MappedNativeOutputs::create(
            &path,
            vec![OutputVariableName::temperature, OutputVariableName::rain],
            6,
        )
        .unwrap();
        let output = Output::new(
            Utc.with_ymd_and_hms(2026, 7, 23, 12, 0, 0).unwrap(),
            Array1::from_vec(vec![
                OutputElement {
                    temperature: 10.0,
                    rain: 1.0,
                    ..OutputElement::default()
                },
                OutputElement {
                    temperature: 20.0,
                    rain: 2.0,
                    ..OutputElement::default()
                },
            ]),
        );
        store.write_tile(&[1, 4], &output).unwrap();
        assert_eq!(
            store
                .read_variable(OutputVariableName::temperature, &[1, 4])
                .unwrap(),
            vec![10.0, 20.0]
        );
        assert_eq!(
            store
                .read_variable(OutputVariableName::rain, &[1, 4])
                .unwrap(),
            vec![1.0, 2.0]
        );
        store.sync_all().unwrap();
        drop(store);
        remove_scratch_file(path).unwrap();
    }
}
