use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    config::FWIModelConfig,
    constants::*,
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
#[derive(Debug, Clone, Default)]
pub struct FWIWarmState {
    pub dates: Vec<DateTime<Utc>>,
    pub ffmc: Vec<f32>,
    pub dmc: Vec<f32>,
    pub dc: Vec<f32>,
    pub rain: Vec<f32>,
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct FWIStateElement {
    pub dates: Vec<DateTime<Utc>>,
    pub ffmc: Vec<f32>,
    pub dmc: Vec<f32>,
    pub dc: Vec<f32>,
    pub rain: Vec<f32>,
    pub humidity: Vec<f32>,
    pub temperature: Vec<f32>,
    pub wind_speed: Vec<f32>,
    pub rain24h: Vec<f32>
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
    pub fn new(
        warm_state: &[FWIWarmState],
        time: &DateTime<Utc>,
        config: FWIModelConfig,
    ) -> FWIState {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| {
                    let n = w.dates.len();
                    FWIStateElement {
                        dates: w.dates.clone(),
                        ffmc: w.ffmc.clone(),
                        dmc: w.dmc.clone(),
                        dc: w.dc.clone(),
                        rain: w.rain.clone(),
                        humidity: vec![NODATAVAL; n],
                        temperature: vec![NODATAVAL; n],
                        wind_speed: vec![NODATAVAL; n],
                        rain24h: vec![NODATAVAL; n],
                    }
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
    pub fn get_output(&mut self, props: &FWIProperties) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&mut self.data)
            .and(&props.data)
            .par_map_collect(|state, prop| get_output_fn(state, prop, time, &self.config));

        Output::new(*time, output_data)
    }

    /// Update the state of the cells
    pub fn update(&mut self, props: &FWIProperties, input: &Input) {
        let new_time = &input.time;
        self.time = *new_time;
        self.update_state(props, input);
    }

    pub fn output(&mut self, props: &FWIProperties) -> Output {
        self.get_output(props)
    }
}
