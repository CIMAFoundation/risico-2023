use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::{store_day_fn, update_fn, get_output_fn},
};

// CELLS PROPERTIES
#[derive(Debug)]
pub struct PortuguesePropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct PortugueseProperties {
    pub data: Array1<PortuguesePropertiesElement>,
    pub len: usize,
}

pub struct PortugueseCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl PortugueseProperties {
    pub fn new(props: PortugueseCellPropertiesContainer) -> Self {
        let data: Array1<PortuguesePropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| PortuguesePropertiesElement {
                lon: *lon,
                lat: props.lats[idx],
            })
            .collect();
    
        let len = data.len();
        Self {
            data,
            len,
        }
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
pub struct PortugueseWarmState {
    pub sum_ign: f32,  // sum of the ignition indices
    pub cum_index: f32,  // coefficient B index of the previous day
}

impl Default for PortugueseWarmState {
    fn default() -> Self {
        Self {
            sum_ign: 0.0,
            cum_index: 0.0
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct PortugueseStateElement {
    pub ign: f32,  // ignition index
    pub sum_ign: f32,  // sum of the ignition indices
    pub cum_index: f32,  // cumulative index
    pub fire_index: f32,  // fire index
    pub temp_12: f32,  // temperature [°C] at 12pm info on the run day
    pub temp_dew_12: f32,  // dew point temperature [°C] at 12pm info on the run day
    pub cum_rain: f32,  // cumulated rain [mm] of the run day
}


impl PortugueseStateElement {

    pub fn clean_day(
        &mut self
    ) {
        self.ign = NODATAVAL;
        self.fire_index = NODATAVAL;
        self.temp_12 = NODATAVAL;
        self.temp_dew_12 = NODATAVAL;
        self.cum_rain = 0.0;
    }
}

#[derive(Debug)]
pub struct PortugueseState {
    pub time: DateTime<Utc>,
    pub data: Array1<PortugueseStateElement>,
    len: usize,
}

impl PortugueseState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &[PortugueseWarmState], time: &DateTime<Utc>) -> PortugueseState {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| PortugueseStateElement {
                    sum_ign: w.sum_ign.clone(),
                    cum_index: w.cum_index.clone(),
                    ign: NODATAVAL,
                    fire_index: NODATAVAL,
                    temp_12: NODATAVAL,
                    temp_dew_12: NODATAVAL,
                    cum_rain: 0.0,
                })
                .collect(),
        );

        PortugueseState {
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
    fn store_day(&mut self, input: &Input, prop: &PortugueseProperties) {
        let time = input.time;  // reference time of the input
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
        let output_data = self.data
            .map(|state| {
                get_output_fn(state)
            });
        // clean the daily values
        self.data.iter_mut().for_each(|state| state.clean_day());
        Output::new(*time, output_data)
    }

    // Update the state of the cells
    pub fn store(&mut self, input: &Input, prop: &PortugueseProperties) {
        self.store_day(input, prop);
    }

    pub fn update(&mut self) {
        self.get_update();
    }

    pub fn output(&mut self) -> Output {
        self.get_output()
    }
}
