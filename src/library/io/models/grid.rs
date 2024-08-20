use std::fmt::Debug;

use crate::library::config::models::{read_config, RISICOError};
use itertools::izip;
use ndarray::Array1;
use rstar::{primitives::GeomWithData, RTree};

#[derive(Debug)]
pub enum ClusterMode {
    Mean,
    Median,
    Min,
    Max,
}
impl From<&str> for ClusterMode {
    fn from(s: &str) -> Self {
        match s {
            "MEAN" | "mean" => ClusterMode::Mean,
            "MAX" | "max" => ClusterMode::Max,
            "MIN" | "min" => ClusterMode::Min,
            _ => panic!("Invalid cluster mode"),
        }
    }
}

pub trait Grid {
    fn index(&self, lat: &f32, lon: &f32) -> Option<usize>;
    fn shape(&self) -> (usize, usize);
    fn indexes(&mut self, lats: &[f32], lons: &[f32]) -> Array1<Option<usize>>;
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

    pub fn project_to_grid(&self, lats: &[f32], lons: &[f32]) -> Vec<Array1<usize>> {
        let (nrows, ncols) = self.shape();

        let mut grid_indexes = vec![vec![]; nrows * ncols];

        izip!(lats, lons)
            .enumerate()
            .for_each(|(index, (lat, lon))| {
                if let Some(idx) = self.index(lat, lon) {
                    grid_indexes[idx].push(index);
                }
            });

        let indexes = grid_indexes
            .iter()
            .map(|v| Array1::from(v.to_owned()))
            .collect();
        indexes
    }

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
        Some(i * self.ncols + j)
    }

    fn shape(&self) -> (usize, usize) {
        (self.nrows, self.ncols)
    }

    fn indexes(&mut self, lats: &[f32], lons: &[f32]) -> Array1<Option<usize>> {
        izip!(lats, lons)
            .map(|(lat, lon)| self.index(lat, lon))
            .collect::<Array1<_>>()
    }
}

#[derive(Debug)]
pub struct IrregularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub lats: Array1<f32>,
    pub lons: Array1<f32>,
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
            lats,
            lons,
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

    fn indexes(&mut self, lats: &[f32], lons: &[f32]) -> Array1<Option<usize>> {
        izip!(lats, lons)
            .map(|(lat, lon)| self.index(lat, lon))
            .collect::<Array1<_>>()
    }
}
