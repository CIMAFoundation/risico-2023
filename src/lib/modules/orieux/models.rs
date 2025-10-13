use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::{get_output_fn, store_day_fn, update_fn},
};

/// Orieux fire index
/// Source: https://wikifire.wsl.ch/tiki-index182c.html?page=Orieux+index&structure=Fire
/// Note: the Orieux is suitable only in summer

// CELLS PROPERTIES
#[derive(Debug)]
pub struct OrieuxPropertiesElement {
    pub lon: f32,
    pub lat: f32,
    pub heat_index: f32,
}

#[derive(Debug)]
pub struct OrieuxProperties {
    pub data: Array1<OrieuxPropertiesElement>,
    pub len: usize,
}

pub struct OrieuxCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
    pub heat_indices: Vec<f32>,
}

impl OrieuxProperties {
    pub fn new(props: OrieuxCellPropertiesContainer) -> Self {
        let data: Array1<OrieuxPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| OrieuxPropertiesElement {
                lon: *lon,
                lat: props.lats[idx],
                heat_index: props.heat_indices[idx],
            })
            .collect();
        let len = data.len();
        Self { data, len }
    }

    pub fn get_coords(&self) -> (Vec<f32>, Vec<f32>) {
        let lats: Vec<f32> = self.data.iter().map(|p| p.lat).collect();
        let lons: Vec<f32> = self.data.iter().map(|p| p.lon).collect();
        (lats, lons)
    }
}

// WARM STATE
#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct OrieuxWarmState {
    pub orieux_wr: f32, // Orieux index of the previous day [mm]
}

impl Default for OrieuxWarmState {
    fn default() -> Self {
        Self {
            orieux_wr: ORIEUX_WR_INIT,
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct OrieuxStateElement {
    pub orieux_wr: f32,      // Orieux water reserve [mm]
    pub pet: f32,            // potential evapotranspiration [mm]
    pub orieux_fd: f32,      // Orieux fire danger class [0-3]
    pub cum_rain: f32,       // daily cumulative precipitation [mm]
    pub min_temp: f32,       // min daily temperature [°C]
    pub max_temp: f32,       // max daily temperature [°C]
    pub max_wind_speed: f32, // max daily wind speed [m/s]
}

impl OrieuxStateElement {
    pub fn clean_day(&mut self) {
        self.pet = NODATAVAL;
        self.orieux_fd = NODATAVAL;
        self.cum_rain = 0.0;
        self.min_temp = NODATAVAL;
        self.max_temp = NODATAVAL;
        self.max_wind_speed = NODATAVAL;
    }
}

#[derive(Debug)]
pub struct OrieuxState {
    pub time: DateTime<Utc>,
    pub data: Array1<OrieuxStateElement>,
    len: usize,
}

impl OrieuxState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state
    pub fn new(warm_state: &[OrieuxWarmState], time: &DateTime<Utc>) -> OrieuxState {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| OrieuxStateElement {
                    orieux_wr: w.orieux_wr,
                    pet: NODATAVAL,
                    orieux_fd: NODATAVAL,
                    cum_rain: 0.0,
                    min_temp: NODATAVAL,
                    max_temp: NODATAVAL,
                    max_wind_speed: NODATAVAL,
                })
                .collect(),
        );

        OrieuxState {
            time: *time,
            data,
            len: warm_state.len(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(non_snake_case)]
    fn store_day(&mut self, input: &Input) {
        self.time = input.time; // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input_data| {
                store_day_fn(state, input_data);
            });
    }

    fn update_state(&mut self, props: &OrieuxProperties) {
        Zip::from(&mut self.data)
            .and(&props.data)
            .par_for_each(|state, prop_data| {
                update_fn(state, prop_data, &self.time);
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(&mut self) -> Output {
        let time = &self.time;
        let output_data = self.data.map(|state| get_output_fn(state));
        // clean the daily values
        self.data.iter_mut().for_each(|state| state.clean_day());
        Output::new(*time, output_data)
    }

    pub fn store(&mut self, input: &Input) {
        self.store_day(input);
    }

    pub fn update(&mut self, props: &OrieuxProperties) {
        self.update_state(props);
    }

    pub fn output(&mut self) -> Output {
        self.get_output()
    }
}
