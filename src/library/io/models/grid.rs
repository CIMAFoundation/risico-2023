use std::{collections::HashMap, fmt::Debug};

use crate::library::{
    config::models::{read_config, ConfigError},
    state::constants::NODATAVAL,
};
use itertools::izip;

#[derive(Debug)]
pub enum ClusterMode {
    Mean,
    Median,
    Min,
    Max,
}

pub trait Grid {
    fn index(&self, lat: &f32, lon: &f32) -> usize;
    fn shape(&self) -> (usize, usize);
    fn build_cache(&mut self, lats: &[f32], lons: &[f32]);
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

    pub fn project_to_grid(
        &self,
        lats: &[f32],
        lons: &[f32],
        values: &[f32],
        mode: &ClusterMode,
    ) -> Vec<f32> {
        let (nrows, ncols) = self.shape();

        let mut grid_values = vec![0.0; nrows * ncols];
        let mut count = vec![0; nrows * ncols];
        izip!(lats, lons, values).for_each(|(lat, lon, value)| {
            let idx = self.index(&lat, &lon);
            grid_values[idx] += value;
            count[idx] += 1;
        });
        match mode {
            _ => {
                for i in 0..grid_values.len() {
                    if count[i] > 0 {
                        grid_values[i] /= count[i] as f32;
                    } else {
                        grid_values[i] = NODATAVAL as f32;
                    }
                }
            }
        }
        grid_values
    }

    pub fn from_txt_file(grid_file: &str) -> Result<RegularGrid, ConfigError> {
        // read the file as text
        let config_map = read_config(grid_file)?;

        let nrows = config_map
            .get("GRIDNROWS")
            .and_then(|value| value.get(0))
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let ncols = config_map
            .get("GRIDNCOLS")
            .and_then(|value| value.get(0))
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let minlat = config_map
            .get("MINLAT")
            .and_then(|value| value.get(0))
            .unwrap()
            .parse::<f32>()
            .unwrap();
        let minlon = config_map
            .get("MINLON")
            .and_then(|value| value.get(0))
            .unwrap()
            .parse::<f32>()
            .unwrap();
        let maxlat = config_map
            .get("MAXLAT")
            .and_then(|value| value.get(0))
            .unwrap()
            .parse::<f32>()
            .unwrap();
        let maxlon = config_map
            .get("MAXLON")
            .and_then(|value| value.get(0))
            .unwrap()
            .parse::<f32>()
            .unwrap();

        let grid = RegularGrid::new(nrows, ncols, minlat, minlon, maxlat, maxlon);

        Ok(grid)
    }
}

impl Grid for RegularGrid {
    fn index(&self, lat: &f32, lon: &f32) -> usize {
        let i = ((lat - self.min_lat) / self.step_lat).round() as usize;
        let j = ((lon - self.min_lon) / self.step_lon).round() as usize;
        i * self.ncols + j
    }

    fn shape(&self) -> (usize, usize) {
        (self.nrows, self.ncols)
    }

    fn build_cache(&mut self, _lats: &[f32], _lons: &[f32]) {}

}

#[derive(Debug)]
pub struct IrregularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub lats: Vec<f32>,
    pub lons: Vec<f32>,

    cached: bool,
    cache: HashMap<([u8; 8]), usize>,
}

impl IrregularGrid {
    pub fn new(nrows: usize, ncols: usize, lats: Vec<f32>, lons: Vec<f32>) -> IrregularGrid {
        IrregularGrid {
            nrows,
            ncols,
            lats,
            lons,
            cache: HashMap::new(),
            cached: false,
        }
    }

    fn get_key(&self, lat: &f32, lon: &f32) -> [u8; 8] {
        let lat_bytes = lat.to_le_bytes();
        let lon_bytes = lon.to_le_bytes();
        let mut key: [u8; 8] = [0; 8];
        key[0..4].copy_from_slice(&lat_bytes);
        key[4..8].copy_from_slice(&lon_bytes);
        key
    }

    fn get_cached_index(&self, lat: &f32, lon: &f32) -> Option<usize> {
        let key = self.get_key(lat, lon);
        self.cache.get(&key).map(|index| *index)
    }

    fn index_non_cached(&self, lat: &f32, lon: &f32) -> usize {
        let mut min_i = self.nrows / 2;
        let mut min_j = self.ncols / 2;

        let mut minidx = min_i * self.ncols + min_j;
        let mut minerr = f32::powf(self.lons[minidx] - lon, 2.0) + f32::powf(self.lats[minidx] - lat, 2.0);

        let mut dobreak = false;
        while !dobreak {
            let mut found = false;
            let i = min_i;
            let j = min_j;
            for ii in i - 1..i + 2 {
                for jj in j - 1..j + 2 {
                    let p_i = usize::min(ii, self.nrows - 1);
                    let p_j = usize::min(jj, self.ncols - 1);
                    let idx2 = p_i * self.ncols + (p_j);
                    let err = f32::powf(self.lons[idx2] - lon, 2.0)
                        + f32::powf(self.lats[idx2] - lat, 2.0);

                    if err < minerr {
                        minerr = err;
                        minidx = idx2;
                        min_i = p_i;
                        min_j = p_j;
                        found = true;
                    }
                }
            }
            if !found {
                dobreak = true;
            }
        }

        minidx
    }
}

impl Grid for IrregularGrid {
    fn index(&self, lat: &f32, lon: &f32) -> usize {
        let maybe_index = self.get_cached_index(lat, lon);
        if let Some(index) = maybe_index {
            return index;
        };
        self.index_non_cached(lat, lon)
    }

    fn shape(&self) -> (usize, usize) {
        (self.nrows, self.ncols)
    }

    fn build_cache(&mut self, lats: &[f32], lons: &[f32]) {
        if self.cached {
            return;
        }

        izip!(lats, lons)
            .map(|(lat, lon)| (self.get_key(lat, lon), self.index_non_cached(lat, lon)))
            .collect::<Vec<_>>()
            .iter()
            .for_each(|(key, index)| {
                self.cache.insert(*key, *index);
            });
        self.cached = true;
    }

}
