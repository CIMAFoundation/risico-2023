use std::collections::HashMap;

pub trait GridFunctions {
    fn get_index(&mut self, lat: f32, lon: f32) -> usize;
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
    fn get_index(&mut self, lat: f32, lon: f32) -> usize {
        let i = ((lat - self.min_lat) / self.step_lat) as usize;
        let j = ((lon - self.min_lon) / self.step_lon) as usize;
        i * self.ncols + j
    }
}

#[derive(Debug)]
pub struct IrregularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub lats: Vec<f32>,
    pub lons: Vec<f32>,

    cache: HashMap<([u8;8]), usize>,
}

impl IrregularGrid {
    pub fn new(nrows: usize, ncols: usize, lats: Vec<f32>, lons: Vec<f32>) -> IrregularGrid {
        IrregularGrid {
            nrows: nrows,
            ncols: ncols,
            lats: lats,
            lons: lons,
            cache: HashMap::new(),
        }
    }
}

impl GridFunctions for IrregularGrid {
    fn get_index(&mut self, lat: f32, lon: f32) -> usize {
        let lat_bytes = lat.to_le_bytes();
        let lon_bytes = lon.to_le_bytes();
        let mut key:[u8;8] = [0; 8];
        key[0..4].copy_from_slice(&lat_bytes);
        key[4..8].copy_from_slice(&lon_bytes);

        let maybe_index = self.cache.get(&key);
        if let Some(index) = maybe_index {
            return *index;
        }

        let mut minerr = f32::MAX; //confronto con maxfloat

        let mut minI = self.nrows / 2;
        let mut minJ = self.ncols / 2;

        let mut minidx = minI * self.ncols + minJ;
        minerr =
            f32::powf(self.lons[minidx] - lon, 2.0) + 
            f32::powf(self.lats[minidx] - lat, 2.0);

        let mut dobreak = false;
        while !dobreak {
            let mut found = false;
            let I = minI;
            let J = minJ;
            for ii in I-1..I+2 {
                for jj in J-1..J+2 {
                    let pI = usize::min(ii, self.nrows - 1);
                    let pJ = usize::min(jj, self.ncols - 1);
                    let idx2 = pI * self.ncols + (pJ);
                    let err = f32::powf(self.lons[idx2] - lon, 2.0)
                        + f32::powf(self.lats[idx2] - lat, 2.0);

                    if err < minerr {
                        minerr = err;
                        minidx = idx2;
                        minI = pI;
                        minJ = pJ;
                        found = true;
                    }
                }
            }
            if !found {
                dobreak = true;
            }
        }
        self.cache.insert(key, minidx);

        minidx
    }
}

#[derive(Debug)]
pub enum Grid {
    Regular(RegularGrid),
    Irregular(IrregularGrid),
}

impl Grid {
    pub fn new_regular(
        nrows: usize,
        ncols: usize,
        min_lat: f32,
        min_lon: f32,
        max_lat: f32,
        max_lon: f32,
    ) -> Grid {
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
        Grid::Irregular(IrregularGrid::new(nrows, ncols, lats, lons))
    }

    pub fn get_index(&mut self, lat: f32, lon: f32) -> usize {
        match self {
            Grid::Regular(grid) => grid.get_index(lat, lon),
            Grid::Irregular(grid) => grid.get_index(lat, lon),
        }
    }
}
