use std::collections::HashMap;

pub trait GridFunctions {
    fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize);
}

#[derive(Debug)]
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

impl GridFunctions for RegularGrid {
    fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize) {
        let i = ((lat - self.min_lat) / self.step_lat) as usize;
        let j = ((lon - self.min_lon) / self.step_lon) as usize;
        (i, j)
    }
}
#[derive(Debug)]
pub struct IrregularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub lats : Vec<f32>,
    pub lons : Vec<f32>,
}

impl GridFunctions for IrregularGrid {
    fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize) {
        let i = self.lats.iter().position(|&x| x == lat).unwrap();
        let j = self.lons.iter().position(|&x| x == lon).unwrap();
        (i, j)
    }
}

#[derive(Debug)]
pub enum Grid {
    Regular(RegularGrid),
    Irregular(IrregularGrid),
}

impl Grid {
    pub fn new_regular(nrows: usize, ncols: usize, min_lat: f32, min_lon: f32, max_lat: f32, max_lon: f32) -> Grid {
        let step_lat = (max_lat - min_lat) / (nrows - 1) as f32;
        let step_lon = (max_lon - min_lon) / (ncols - 1) as f32;
        Grid::Regular(RegularGrid {
            nrows: nrows,
            ncols: ncols,
            min_lat: min_lat,
            min_lon: min_lon,
            max_lat: max_lat,
            max_lon: max_lon,
            step_lat: step_lat,
            step_lon: step_lon,
        })
    }

    pub fn new_irregular(nrows: usize, ncols: usize, lats: Vec<f32>, lons: Vec<f32>) -> Grid {
        Grid::Irregular(IrregularGrid {
            nrows: nrows,
            ncols: ncols,
            lats: lats,
            lons: lons,
        })
    }

    pub fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize) {
        match self {
            Grid::Regular(grid) => grid.get_i_j(lat, lon),
            Grid::Irregular(grid) => grid.get_i_j(lat, lon),
        }
    }
}
   

#[derive(Debug)]
pub struct InputData<'a> {
    pub values: Vec<f32>,
    pub grid: &'a Grid
}

