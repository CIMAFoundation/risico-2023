use std::path::Path;

use geotiff_reader::GeoTiffFile;
use sha2::{Digest, Sha256};

use crate::common::helpers::RISICOError;

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
        let index = cell_index as usize;
        let row = index / self.width;
        let col = index % self.width;
        let x = self.transform[0]
            + (col as f64 + 0.5) * self.transform[1]
            + (row as f64 + 0.5) * self.transform[2];
        let y = self.transform[3]
            + (col as f64 + 0.5) * self.transform[4]
            + (row as f64 + 0.5) * self.transform[5];
        (x as f32, y as f32)
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

#[derive(Debug)]
pub struct RasterDomain {
    pub grid: RasterGrid,
    pub cell_indexes: Vec<u32>,
    pub grid_hash: String,
}

impl RasterDomain {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RISICOError> {
        let path = path.as_ref();
        let layer = RasterLayer::open(path)?;
        let cell_indexes: Vec<u32> = layer
            .values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                (!layer.is_nodata(*value) && *value != 0.0).then_some(index as u32)
            })
            .collect();
        if cell_indexes.is_empty() {
            return Err(format!("domain mask {} contains no active cells", path.display()).into());
        }

        let grid_hash = grid_hash(&layer.grid, &cell_indexes);
        Ok(Self {
            grid: layer.grid,
            cell_indexes,
            grid_hash,
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
        let layer = RasterLayer::open(path)?;
        if !self.grid.matches(&layer.grid) {
            return Err(format!(
                "static layer {name} ({}) is not aligned with the domain mask",
                path.display()
            )
            .into());
        }

        self.cell_indexes
            .iter()
            .map(|index| {
                let value = layer.values[*index as usize];
                if layer.is_nodata(value) {
                    Err(format!(
                        "static layer {name} ({}) has nodata at active cell {}",
                        path.display(),
                        index
                    )
                    .into())
                } else {
                    Ok(value)
                }
            })
            .collect()
    }
}

struct RasterLayer {
    grid: RasterGrid,
    values: Vec<f32>,
    nodata: Option<f64>,
}

impl RasterLayer {
    fn open(path: &Path) -> Result<Self, RISICOError> {
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
        let values = read_f32_band(&file, path)?;
        let expected_len = grid.width * grid.height;
        if values.len() != expected_len {
            return Err(format!(
                "GeoTIFF {} decoded {} samples, expected {expected_len}",
                path.display(),
                values.len()
            )
            .into());
        }

        Ok(Self {
            grid,
            values,
            nodata,
        })
    }

    fn is_nodata(&self, value: f32) -> bool {
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

fn read_f32_band(file: &GeoTiffFile, path: &Path) -> Result<Vec<f32>, RISICOError> {
    macro_rules! try_type {
        ($type:ty) => {
            if let Ok(values) = file.read_band::<$type>(0) {
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
        "GeoTIFF {} uses an unsupported sample type or could not be decoded",
        path.display()
    )
    .into())
}

fn grid_hash(grid: &RasterGrid, cell_indexes: &[u32]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"risico-grid-v1");
    hash.update((grid.width as u64).to_le_bytes());
    hash.update((grid.height as u64).to_le_bytes());
    hash.update(grid.epsg.to_le_bytes());
    for value in grid.transform {
        hash.update(value.to_le_bytes());
    }
    for index in cell_indexes {
        hash.update(index.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
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
    fn grid_hash_changes_with_the_mask() {
        let grid = RasterGrid {
            width: 2,
            height: 2,
            epsg: 4326,
            transform: [10.0, 0.5, 0.0, 45.0, 0.0, -0.5],
        };
        assert_ne!(grid_hash(&grid, &[0, 1]), grid_hash(&grid, &[0, 2]));
    }
}
