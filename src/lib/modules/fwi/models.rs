use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    config::FWIModelConfig,
    functions::{get_output_fn, update_state_fn},
};

// CELLS PROPERTIES
#[derive(Debug)]
pub struct FWIPropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct FWIProperties {
    pub data: Array1<FWIPropertiesElement>,
    pub len: usize,
}

pub struct FWICellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl FWIProperties {
    pub fn new(props: FWICellPropertiesContainer) -> Self {
        let data: Array1<FWIPropertiesElement> = props
            .lons
            .into_iter()
            .zip(props.lats)
            .map(|(lon, lat)| FWIPropertiesElement { lon, lat })
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
pub struct FWIWarmState {
    pub ffmc_history: Vec<(DateTime<Utc>, f32)>,
    pub dmc_history: Vec<(DateTime<Utc>, f32)>,
    pub dc_history: Vec<(DateTime<Utc>, f32)>,
    pub rain_history: Vec<(DateTime<Utc>, f32)>
}

impl Default for FWIWarmState {
    fn default() -> Self {
        FWIWarmState 
        {
            ffmc_history: vec![],
            dmc_history: vec![],
            dc_history: vec![],
            rain_history: vec![],
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct FWIStateElement {
    pub ffmc_history: Vec<(DateTime<Utc>, f32)>,
    pub dmc_history: Vec<(DateTime<Utc>, f32)>,
    pub dc_history: Vec<(DateTime<Utc>, f32)>,
    pub rain_history: Vec<(DateTime<Utc>, f32)>
}

impl FWIStateElement {

    pub fn get_24h(&self, variable: &str, time: &DateTime<Utc>) -> Vec<(DateTime<Utc>, f32)> {
        match variable {
            "ffmc" => self.ffmc_history
                        .iter()
                        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP)
                        .map(|(t, r)| (*t, *r))
                        .collect(),
            "dmc" => self.dmc_history
                        .iter()
                        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP)
                        .map(|(t, r)| (*t, *r))
                        .collect(),
            "dc" => self.dc_history
                        .iter()
                        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP)
                        .map(|(t, r)| (*t, *r))
                        .collect(),
            "rain" => self.rain_history
                        .iter()
                        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP)
                        .map(|(t, r)| (*t, *r))
                        .collect(),
            _ => Vec::new(),
        }
    }

    pub fn add_value(&mut self, variable: &str, time: &DateTime<Utc>, value: f32) {
        match variable {
            "ffmc" => {
                self.ffmc_history.push((*time, value));
                // Remove values older than 24 hours
                self.ffmc_history.retain(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP);
            },
            "dmc" => {
                self.dmc_history.push((*time, value));
                // Remove values older than 24 hours
                self.dmc_history.retain(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP);
            },
            "dc" => {
                self.dc_history.push((*time, value));
                // Remove values older than 24 hours
                self.dc_history.retain(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP);
            },
            "rain" => {
                self.rain_history.push((*time, value));
                // Remove values older than 24 hours
                self.rain_history.retain(|(t, _)| time.signed_duration_since(*t).num_hours() <= TIME_STEP);
            },
            _ => (),
        }
    }

}

#[derive(Debug)]
pub struct FWIState {
    pub time: DateTime<Utc>,
    pub data: Array1<FWIStateElement>,
    len: usize,
    config: FWIModelConfig,
}

impl FWIState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &[FWIWarmState], time: &DateTime<Utc>, config: FWIModelConfig) -> FWIState {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| FWIStateElement {
                    ffmc_history: w.ffmc_history.clone(),
                    dmc_history: w.dmc_history.clone(),
                    dc_history: w.dc_history.clone(),
                    rain_history: w.rain_history.clone()
                })
                .collect(),
        );

        FWIState {
            time: *time,
            data,
            len: warm_state.len(),
            config,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(non_snake_case)]
    fn update_state(&mut self, props: &FWIProperties, input: &Input) {
        let time = &self.time;
        Zip::from(&mut self.data)
            .and(&props.data)
            .and(&input.data)
            .par_for_each(|state, props, input_data| {
                update_state_fn(state, props, input_data, time, &self.config)
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &FWIState, input: &Input) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&self.data)
            .and(&input.data)
            .par_map_collect(|state, input| get_output_fn(state, input, &self.config));

        Output::new(*time, output_data)
    }

    /// Update the state of the cells
    pub fn update(&mut self, props: &FWIProperties, input: &Input) {
        let new_time = &input.time;
        self.time = *new_time;
        self.update_state(props, input);
    }

    pub fn output(&self, input: &Input) -> Output {
        self.get_output(input)
    }
}
