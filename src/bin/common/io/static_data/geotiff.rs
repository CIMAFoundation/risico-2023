use std::path::{Path, PathBuf};

use geotiff_reader::GeoTiffFile;

use crate::common::helpers::RISICOError;
use crate::common::io::streaming::GridWindow;

const GRID_TOLERANCE: f64 = 1.0e-12;
#[allow(dead_code)]
pub const STATIC_NODATA: f32 = -9999.0;

#[derive(Clone, Debug)]
pub struct RasterGrid {
    pub width: usize,
    pub height: usize,
    pub epsg: u32,
    /// GDAL-style, corner-based affine transform.
    pub transform: [f64; 6],
}

impl RasterGrid {
    fn from_file(file: &GeoTiffFile, path: &Path) -> Result<Self, RISICOError> {
        let epsg = file
            .epsg()
            .ok_or_else(|| format!("GeoTIFF {} has no EPSG code", path.display()))?;
        if epsg != 4326 {
            return Err(format!(
                "GeoTIFF {} uses EPSG:{epsg}; static model grids currently require EPSG:4326",
                path.display()
            )
            .into());
        }

        let transform = file
            .transform()
            .ok_or_else(|| format!("GeoTIFF {} has no affine transform", path.display()))?;
        if transform.pixel_width <= 0.0
            || transform.pixel_height >= 0.0
            || transform.skew_x.abs() > GRID_TOLERANCE
            || transform.skew_y.abs() > GRID_TOLERANCE
        {
            return Err(format!(
                "GeoTIFF {} must be north-up, unrotated, with positive X and negative Y pixel size",
                path.display()
            )
            .into());
        }

        Ok(Self {
            width: file.width() as usize,
            height: file.height() as usize,
            epsg,
            transform: [
                transform.origin_x,
                transform.pixel_width,
                transform.skew_x,
                transform.origin_y,
                transform.skew_y,
                transform.pixel_height,
            ],
        })
    }

    pub fn cell_center(&self, cell_index: u32) -> (f32, f32) {
        let (x, y) = self.cell_center_f64(cell_index);
        (x as f32, y as f32)
    }

    pub fn cell_center_f64(&self, cell_index: u32) -> (f64, f64) {
        let index = cell_index as usize;
        let row = index / self.width;
        let col = index % self.width;
        let x = self.transform[0]
            + (col as f64 + 0.5) * self.transform[1]
            + (row as f64 + 0.5) * self.transform[2];
        let y = self.transform[3]
            + (col as f64 + 0.5) * self.transform[4]
            + (row as f64 + 0.5) * self.transform[5];
        (x, y)
    }

    /// Return the row-major cell whose centre is nearest to the coordinate.
    pub fn nearest_cell_index(&self, x: f64, y: f64) -> Option<usize> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let col = ((x - self.transform[0]) / self.transform[1] - 0.5).round();
        let row = ((y - self.transform[3]) / self.transform[5] - 0.5).round();
        if col < 0.0 || row < 0.0 || col >= self.width as f64 || row >= self.height as f64 {
            None
        } else {
            Some(row as usize * self.width + col as usize)
        }
    }

    pub fn x_coordinates(&self) -> Vec<f64> {
        (0..self.width)
            .map(|col| self.transform[0] + (col as f64 + 0.5) * self.transform[1])
            .collect()
    }

    pub fn y_coordinates(&self) -> Vec<f64> {
        (0..self.height)
            .map(|row| self.transform[3] + (row as f64 + 0.5) * self.transform[5])
            .collect()
    }

    fn matches(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.epsg == other.epsg
            && self
                .transform
                .iter()
                .zip(other.transform.iter())
                .all(|(left, right)| {
                    let scale = left.abs().max(right.abs()).max(1.0);
                    (left - right).abs() <= GRID_TOLERANCE * scale
                })
    }
}

/// The window a run of active cells was found in.
///
/// The domain mask is scanned window by window, so the active cells are already
/// grouped: keeping the grouping costs one entry per window and saves rebuilding
/// it, per cell, for every static layer that is read afterwards.
#[derive(Clone, Copy, Debug)]
struct CellWindow {
    window: GridWindow,
    start: usize,
    end: usize,
}

#[derive(Debug)]
pub struct RasterDomain {
    pub grid: RasterGrid,
    pub cell_indexes: Vec<u32>,
    windows: Vec<CellWindow>,
}

impl RasterDomain {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RISICOError> {
        let path = path.as_ref();
        let layer = RasterLayerReader::open(path)?;
        let mut cell_indexes = Vec::new();
        let mut windows = Vec::new();
        for window in raster_windows(&layer.grid, 512, 512) {
            let values = layer.read_window(window)?;
            let start = cell_indexes.len();
            for (local_index, value) in values.into_iter().enumerate() {
                if layer.is_nodata(value) || value == 0.0 {
                    continue;
                }
                let local_row = local_index / window.width;
                let local_col = local_index % window.width;
                let index = (window.row + local_row) * layer.grid.width + window.col + local_col;
                cell_indexes.push(index as u32);
            }
            if cell_indexes.len() > start {
                windows.push(CellWindow {
                    window,
                    start,
                    end: cell_indexes.len(),
                });
            }
        }
        if cell_indexes.is_empty() {
            return Err(format!("domain mask {} contains no active cells", path.display()).into());
        }

        Ok(Self {
            grid: layer.grid,
            cell_indexes,
            windows,
        })
    }

    pub fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.cell_indexes
            .iter()
            .map(|index| self.grid.cell_center(*index))
            .map(|(lon, lat)| (lat, lon))
            .unzip()
    }

    pub fn read_required_layer(
        &self,
        path: impl AsRef<Path>,
        name: &str,
    ) -> Result<Vec<f32>, RISICOError> {
        let path = path.as_ref();
        let layer = RasterLayerReader::open(path)?;
        if !self.grid.matches(&layer.grid) {
            return Err(format!(
                "static layer {name} ({}) is not aligned with the domain mask",
                path.display()
            )
            .into());
        }

        // Read one mask window at a time and keep only its active cells: the
        // layer never exists in memory beyond a single window, whatever the
        // raster's size.
        let mut values = vec![0.0_f32; self.cell_indexes.len()];
        for group in &self.windows {
            let window = group.window;
            let block = layer.read_window(window)?;
            let cells = &self.cell_indexes[group.start..group.end];
            for (slot, &cell) in values[group.start..group.end].iter_mut().zip(cells) {
                let cell = cell as usize;
                let row = cell / self.grid.width - window.row;
                let col = cell % self.grid.width - window.col;
                *slot = block[row * window.width + col];
            }
        }
        // for (&index, &value) in self.cell_indexes.iter().zip(&values) {
        //     if layer.is_nodata(value) {
        //         return Err(format!(
        //             "static layer {name} ({}) has nodata at active cell {}",
        //             path.display(),
        //             index
        //         )
        //         .into());
        //     }
        // }
        Ok(values)
    }
}

/// An open single-band GeoTIFF that can decode bounded pixel windows.
pub struct RasterLayerReader {
    path: PathBuf,
    file: GeoTiffFile,
    pub grid: RasterGrid,
    nodata: Option<f64>,
}

impl RasterLayerReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RISICOError> {
        let path = path.as_ref();
        let file = GeoTiffFile::open(path)
            .map_err(|error| format!("cannot open GeoTIFF {}: {error}", path.display()))?;
        if file.band_count() != 1 {
            return Err(format!(
                "static GeoTIFF {} must contain exactly one band, found {}",
                path.display(),
                file.band_count()
            )
            .into());
        }

        let grid = RasterGrid::from_file(&file, path)?;
        let nodata = file.nodata().and_then(|value| value.trim().parse().ok());

        Ok(Self {
            path: path.to_path_buf(),
            file,
            grid,
            nodata,
        })
    }

    pub fn read_window(&self, window: GridWindow) -> Result<Vec<f32>, RISICOError> {
        if window.is_empty()
            || window.end_row() > self.grid.height
            || window.end_col() > self.grid.width
        {
            return Err(format!(
                "window {:?} is outside GeoTIFF {} ({}x{})",
                window,
                self.path.display(),
                self.grid.width,
                self.grid.height
            )
            .into());
        }
        let values = read_f32_band_window(&self.file, &self.path, window)?;
        if values.len() != window.len() {
            return Err(format!(
                "GeoTIFF {} decoded {} window samples, expected {}",
                self.path.display(),
                values.len(),
                window.len()
            )
            .into());
        }
        Ok(values)
    }

    pub fn is_nodata(&self, value: f32) -> bool {
        value.is_nan()
            || self.nodata.is_some_and(|nodata| {
                if nodata.is_nan() {
                    value.is_nan()
                } else {
                    value as f64 == nodata
                }
            })
    }
}

fn raster_windows(
    grid: &RasterGrid,
    tile_height: usize,
    tile_width: usize,
) -> impl Iterator<Item = GridWindow> {
    let grid_height = grid.height;
    let grid_width = grid.width;
    (0..grid_height).step_by(tile_height).flat_map(move |row| {
        (0..grid_width)
            .step_by(tile_width)
            .map(move |col| GridWindow {
                row,
                col,
                height: tile_height.min(grid_height - row),
                width: tile_width.min(grid_width - col),
            })
    })
}

fn read_f32_band_window(
    file: &GeoTiffFile,
    path: &Path,
    window: GridWindow,
) -> Result<Vec<f32>, RISICOError> {
    macro_rules! try_type {
        ($type:ty) => {
            if let Ok(values) = file.read_band_window::<$type>(
                0,
                window.row,
                window.col,
                window.height,
                window.width,
            ) {
                return Ok(values.iter().map(|value| *value as f32).collect());
            }
        };
    }

    try_type!(f32);
    try_type!(f64);
    try_type!(u8);
    try_type!(i8);
    try_type!(u16);
    try_type!(i16);
    try_type!(u32);
    try_type!(i32);

    Err(format!(
        "GeoTIFF {} uses an unsupported sample type or window {:?} could not be decoded",
        path.display(),
        window
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_centres_use_corner_based_transform() {
        let grid = RasterGrid {
            width: 2,
            height: 2,
            epsg: 4326,
            transform: [10.0, 0.5, 0.0, 45.0, 0.0, -0.5],
        };
        assert_eq!(grid.cell_center(0), (10.25, 44.75));
        assert_eq!(grid.cell_center(3), (10.75, 44.25));
    }

    #[test]
    fn nearest_cell_uses_cell_centres() {
        let grid = RasterGrid {
            width: 2,
            height: 2,
            epsg: 4326,
            transform: [10.0, 0.5, 0.0, 45.0, 0.0, -0.5],
        };
        assert_eq!(grid.nearest_cell_index(10.1, 44.9), Some(0));
        assert_eq!(grid.nearest_cell_index(10.9, 44.1), Some(3));
        assert_eq!(grid.nearest_cell_index(9.0, 44.5), None);
    }

    #[test]
    fn raster_windows_cover_edge_fragments() {
        let windows = raster_windows(
            &RasterGrid {
                width: 5,
                height: 3,
                epsg: 4326,
                transform: [10.0, 0.5, 0.0, 45.0, 0.0, -0.5],
            },
            2,
            3,
        )
        .collect::<Vec<_>>();
        assert_eq!(
            windows,
            vec![
                GridWindow {
                    row: 0,
                    col: 0,
                    height: 2,
                    width: 3,
                },
                GridWindow {
                    row: 0,
                    col: 3,
                    height: 2,
                    width: 2,
                },
                GridWindow {
                    row: 2,
                    col: 0,
                    height: 1,
                    width: 3,
                },
                GridWindow {
                    row: 2,
                    col: 3,
                    height: 1,
                    width: 2,
                },
            ]
        );
    }
}
