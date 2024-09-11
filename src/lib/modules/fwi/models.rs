use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    config::ModelConfig,
    functions::{get_output_fn, update_state_fn},
};

// CELLS PROPERTIES
#[derive(Debug)]
pub struct PropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct Properties {
    pub data: Array1<PropertiesElement>,
    pub len: usize,
}

pub struct CellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl Properties {
    pub fn new(props: CellPropertiesContainer) -> Self {
        let data: Array1<PropertiesElement> = props
            .lons
            .into_iter()
            .zip(props.lats)
            .map(|(lon, lat)| PropertiesElement { lon, lat })
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
pub struct WarmState {
    pub ffmc: f32,
    pub dmc: f32,
    pub dc: f32,
}

impl Default for WarmState {
    fn default() -> Self {
        WarmState {
            ffmc: 85.0,
            dmc: 6.0,
            dc: 15.0,
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct StateElement {
    pub ffmc: f32,
    pub dmc: f32,
    pub dc: f32,
}

#[derive(Debug)]
pub struct State {
    pub time: DateTime<Utc>,
    pub data: Array1<StateElement>,
    len: usize,
    config: ModelConfig,
}

impl State {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &[WarmState], time: &DateTime<Utc>, config: ModelConfig) -> State {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| StateElement {
                    ffmc: w.ffmc,
                    dmc: w.dmc,
                    dc: w.dc,
                })
                .collect(),
        );

        State {
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
    fn update_state(&mut self, props: &Properties, input: &Input) {
        let time = &self.time;
        Zip::from(&mut self.data)
            .and(&props.data)
            .and(&input.data)
            .par_for_each(|state, props, input_data| {
                update_state_fn(state, props, input_data, time, &self.config)
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &State, input: &Input) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&self.data)
            .and(&input.data)
            .par_map_collect(|state, input| get_output_fn(state, input, &self.config));

        Output::new(*time, output_data)
    }

    /// Update the state of the cells
    pub fn update(&mut self, props: &Properties, input: &Input) {
        let new_time = &input.time;
        self.time = *new_time;
        self.update_state(props, input);
    }

    pub fn output(&self, input: &Input) -> Output {
        self.get_output(input)
    }
}
