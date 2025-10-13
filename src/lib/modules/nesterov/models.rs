use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::{get_output_fn, store_day_fn, update_fn},
};

/// Nesterov index
/// Source: https://wikifire.wsl.ch/tiki-indexfa8e.html?page=Nesterov+ignition+index&structure=Fire

// CELLS PROPERTIES
#[derive(Debug)]
pub struct NesterovPropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct NesterovProperties {
    pub data: Array1<NesterovPropertiesElement>,
    pub len: usize,
}

pub struct NesterovCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl NesterovProperties {
    pub fn new(props: NesterovCellPropertiesContainer) -> Self {
        let data: Array1<NesterovPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| NesterovPropertiesElement {
                lon: *lon,
                lat: props.lats[idx],
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
pub struct NesterovWarmState {
    pub nesterov: f32, // Nesterov index of the previous day
}

impl Default for NesterovWarmState {
    fn default() -> Self {
        Self {
            nesterov: NESTEROV_INIT,
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct NesterovStateElement {
    pub nesterov: f32,    // Nesterov index
    pub temp_15: f32,     // temperature [°C] at 15:00
    pub temp_dew_15: f32, // dew point temperature [°C] at 15:00
    pub cum_rain: f32,    // cumulated daily rain [mm]
}

impl NesterovStateElement {
    pub fn clean_day(&mut self) {
        self.temp_15 = NODATAVAL;
        self.temp_dew_15 = NODATAVAL;
        self.cum_rain = 0.0;
    }
}

#[derive(Debug)]
pub struct NesterovState {
    pub time: DateTime<Utc>,
    pub data: Array1<NesterovStateElement>,
    len: usize,
}

impl NesterovState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state
    pub fn new(warm_state: &[NesterovWarmState], time: &DateTime<Utc>) -> NesterovState {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| NesterovStateElement {
                    nesterov: w.nesterov,
                    temp_15: NODATAVAL,
                    temp_dew_15: NODATAVAL,
                    cum_rain: 0.0,
                })
                .collect(),
        );
        NesterovState {
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

    // Store the daily info at 15:00 local time
    #[allow(non_snake_case)]
    fn store_day(&mut self, input: &Input, prop: &NesterovProperties) {
        let time = input.time; // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .and(&prop.data)
            .par_for_each(|state, input_data, prop_data| {
                store_day_fn(state, input_data, prop_data, &time);
            });
        self.time = time;
    }

    fn get_update(&mut self) {
        self.data.map_inplace(|state| {
            update_fn(state);
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

    // Update the state of the cells
    pub fn store(&mut self, input: &Input, prop: &NesterovProperties) {
        self.store_day(input, prop);
    }

    pub fn update(&mut self) {
        self.get_update();
    }

    pub fn output(&mut self) -> Output {
        self.get_output()
    }
}
