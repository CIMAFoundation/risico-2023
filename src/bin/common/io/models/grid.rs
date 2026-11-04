use std::fmt::Debug;

use itertools::izip;
use ndarray::Array1;
use rayon::prelude::*;
use rstar::{primitives::GeomWithData, RTree};
use serde_derive::{Deserialize, Serialize};

use strum_macros::{Display, EnumString};

use risico::constants::NODATAVAL;

use crate::common::{config::builder::read_config, helpers::RISICOError};

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumString, Display, Serialize, Deserialize)]
#[strum(ascii_case_insensitive)]
pub enum ClusterMode {
    Mean,
    Median,
    Min,
    Max,
}

pub trait Grid: Sync + Send {
    fn index(&self, lat: &f32, lon: &f32) -> Option<usize>;
    fn shape(&self) -> (usize, usize);
    fn indexes(&self, lats: &[f32], lons: &[f32]) -> CellIndexes;
}

/// Marks a model cell that falls outside the source grid.
const UNMAPPED_CELL: u32 = u32::MAX;

/// Source-grid positions for a set of model cells, four bytes each.
///
/// One mapping is retained per spatial tile for the whole run, so together they
/// cover the entire domain. `Option<usize>` would spend sixteen bytes per cell
/// to address a grid of at most a few million points; a sentinel keeps the
/// same information in four.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellIndexes {
    indexes: Vec<u32>,
}

impl CellIndexes {
    pub fn len(&self) -> usize {
        self.indexes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    /// The source-grid position of a model cell, or `None` if it is unmapped.
    pub fn get(&self, position: usize) -> Option<usize> {
        match self.indexes.get(position).copied() {
            None | Some(UNMAPPED_CELL) => None,
            Some(index) => Some(index as usize),
        }
    }

    /// Gather one source value per model cell, substituting `NODATAVAL` where
    /// the cell has no counterpart on the source grid.
    ///
    /// Called per tile from the parallel tile pass, so it stays serial and
    /// leaves the coarse-grained parallelism to the caller.
    pub fn gather(&self, values: &[f32]) -> Array1<f32> {
        let gathered: Vec<f32> = self
            .indexes
            .iter()
            .map(|&index| {
                if index == UNMAPPED_CELL {
                    NODATAVAL
                } else {
                    values[index as usize]
                }
            })
            .collect();
        Array1::from(gathered)
    }
}

/// Map every coordinate pair onto its source-grid cell in parallel.
///
/// Lookups are independent and, for irregular grids, dominated by an R-tree
/// nearest-neighbour query, so this is the hot path when mapping a domain of
/// millions of cells onto the input grid.
fn indexes_in_parallel<G>(grid: &G, lats: &[f32], lons: &[f32]) -> CellIndexes
where
    G: Grid + ?Sized,
{
    let indexes: Vec<u32> = lats
        .par_iter()
        .zip(lons.par_iter())
        .map(|(lat, lon)| match grid.index(lat, lon) {
            None => UNMAPPED_CELL,
            Some(index) => {
                let packed = u32::try_from(index).unwrap_or(UNMAPPED_CELL);
                assert_ne!(
                    packed, UNMAPPED_CELL,
                    "source grid is too large to address with 32 bits"
                );
                packed
            }
        })
        .collect();
    CellIndexes { indexes }
}

impl Debug for dyn Grid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Grid {:?}", self.shape())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RegularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub min_lat: f32,
    pub min_lon: f32,
    pub max_lat: f32,
    pub max_lon: f32,
    pub step_lat: f32,
    pub step_lon: f32,
}

impl RegularGrid {
    pub fn new(
        nrows: usize,
        ncols: usize,
        min_lat: f32,
        min_lon: f32,
        max_lat: f32,
        max_lon: f32,
    ) -> Self {
        let step_lat = (max_lat - min_lat) / (nrows - 1) as f32;
        let step_lon = (max_lon - min_lon) / (ncols - 1) as f32;
        RegularGrid {
            nrows,
            ncols,
            min_lat,
            min_lon,
            max_lat,
            max_lon,
            step_lat,
            step_lon,
        }
    }

    // pub fn project_to_grid(&self, lats: &[f32], lons: &[f32]) -> Vec<Array1<usize>> {
    //     let (nrows, ncols) = self.shape();

    //     let mut grid_indexes = vec![vec![]; nrows * ncols];

    //     izip!(lats, lons)
    //         .enumerate()
    //         .for_each(|(index, (lat, lon))| {
    //             if let Some(idx) = self.index(lat, lon) {
    //                 grid_indexes[idx].push(index);
    //             }
    //         });

    //     let indexes = grid_indexes
    //         .iter()
    //         .map(|v| Array1::from(v.to_owned()))
    //         .collect();
    //     indexes
    // }

    pub fn from_txt_file(grid_file: &str) -> Result<RegularGrid, RISICOError> {
        // read the file as text
        let config_map = read_config(grid_file)?;

        let nrows = config_map
            .get("GRIDNROWS")
            .and_then(|value| value.first())
            .expect("GRIDNROWS not found in grid file")
            .replace("f", "")
            .parse::<usize>()
            .expect("GRIDNROWS is not a number");
        let ncols = config_map
            .get("GRIDNCOLS")
            .and_then(|value| value.first())
            .expect("GRIDNCOLS not found in grid file")
            .replace("f", "")
            .parse::<usize>()
            .expect("GRIDNCOLS is not a number");
        let minlat = config_map
            .get("MINLAT")
            .and_then(|value| value.first())
            .expect("MINLAT not found in grid file")
            .replace("f", "")
            .parse::<f32>()
            .expect("MINLAT is not a number");
        let minlon = config_map
            .get("MINLON")
            .and_then(|value| value.first())
            .expect("MINLON not found in grid file")
            .replace("f", "")
            .parse::<f32>()
            .expect("MINLON is not a number");
        let maxlat = config_map
            .get("MAXLAT")
            .and_then(|value| value.first())
            .expect("MAXLAT not found in grid file")
            .replace("f", "")
            .parse::<f32>()
            .expect("MAXLAT is not a number");
        let maxlon = config_map
            .get("MAXLON")
            .and_then(|value| value.first())
            .expect("MAXLON not found in grid file")
            .replace("f", "")
            .parse::<f32>()
            .expect("MAXLON is not a number");

        let grid = RegularGrid::new(nrows, ncols, minlat, minlon, maxlat, maxlon);

        Ok(grid)
    }
}

impl Grid for RegularGrid {
    fn index(&self, lat: &f32, lon: &f32) -> Option<usize> {
        if lat < &(self.min_lat - self.step_lat / 2.0)
            || lat > &(self.max_lat + self.step_lat / 2.0)
            || lon < &(self.min_lon - self.step_lon / 2.0)
            || lon > &(self.max_lon + self.step_lon / 2.0)
        {
            return None;
        }
        let i = ((lat - self.min_lat) / self.step_lat).round() as usize;
        let j = ((lon - self.min_lon) / self.step_lon).round() as usize;
        if i >= self.nrows || j >= self.ncols {
            return None;
        }
        Some(i * self.ncols + j)
    }

    fn shape(&self) -> (usize, usize) {
        (self.nrows, self.ncols)
    }

    fn indexes(&self, lats: &[f32], lons: &[f32]) -> CellIndexes {
        indexes_in_parallel(self, lats, lons)
    }
}

#[derive(Debug)]
pub struct IrregularGrid {
    pub nrows: usize,
    pub ncols: usize,
    // pub lats: Array1<f32>,
    // pub lons: Array1<f32>,
    tree: RTree<PointWithIndex>,
}

impl IrregularGrid {
    pub fn new(nrows: usize, ncols: usize, lats: Array1<f32>, lons: Array1<f32>) -> IrregularGrid {
        let points = izip!(&lats, &lons)
            .enumerate()
            .map(|(index, (lat, lon))| PointWithIndex::new([*lat, *lon], index))
            .collect::<Vec<_>>();
        let tree = RTree::bulk_load(points);

        IrregularGrid {
            nrows,
            ncols,
            // lats,
            // lons,
            tree,
        }
    }
}

type PointWithIndex = GeomWithData<[f32; 2], usize>;

impl Grid for IrregularGrid {
    fn index(&self, lat: &f32, lon: &f32) -> Option<usize> {
        self.tree.nearest_neighbor(&[*lat, *lon]).map(|p| p.data)
    }

    fn shape(&self) -> (usize, usize) {
        (self.nrows, self.ncols)
    }

    fn indexes(&self, lats: &[f32], lons: &[f32]) -> CellIndexes {
        indexes_in_parallel(self, lats, lons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_indexes_round_trip_mapped_and_unmapped_cells() {
        let grid = RegularGrid::new(2, 2, 0.0, 0.0, 1.0, 1.0);
        // The third pair sits far outside the grid and has no source cell.
        let indexes = grid.indexes(&[0.0, 1.0, 90.0], &[0.0, 1.0, 90.0]);

        assert_eq!(indexes.len(), 3);
        assert_eq!(indexes.get(0), Some(0));
        assert_eq!(indexes.get(1), Some(3));
        assert_eq!(indexes.get(2), None);
        assert_eq!(indexes.get(3), None);
    }

    #[test]
    fn gather_substitutes_nodata_for_unmapped_cells() {
        let grid = RegularGrid::new(2, 2, 0.0, 0.0, 1.0, 1.0);
        let indexes = grid.indexes(&[0.0, 90.0, 1.0], &[0.0, 90.0, 1.0]);
        let values = [10.0, 20.0, 30.0, 40.0];

        let gathered = indexes.gather(&values);

        assert_eq!(gathered.len(), 3);
        assert_eq!(gathered[0], 10.0);
        assert_eq!(gathered[1], NODATAVAL);
        assert_eq!(gathered[2], 40.0);
    }
}
