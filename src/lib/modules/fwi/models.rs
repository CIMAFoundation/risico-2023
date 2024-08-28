use chrono::prelude::*;
use ndarray::{Array1, Zip};
use rayon::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use strum_macros::{Display, EnumProperty, EnumString};

use crate::modules::fwi::constants::NODATAVAL;

use super::{
    config::ModelConfig,
    functions::{get_output_fn, update_moisture_fn},
};


// CELLS PROPERTIES
#[derive(Debug)]
pub struct PropertiesElement {
    pub lon: f32,
    pub lat: f32
}

#[derive(Debug)]
pub struct Properties {
    pub data: Array1<PropertiesElement>,
    pub len: usize,
}

pub struct CellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>
}

impl Properties {
    pub fn new(
        props: CellPropertiesContainer,
    ) -> Self {
        let data: Array1<PropertiesElement> = props
            .iter()
            .enumerate()
            .map(|(idx, v)| PropertiesElement {
                lon: props.lons[idx],
                lat: props.lats[idx]
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


// INPUT STRUCTURE
#[derive(Debug)]
pub struct InputElement {
    /// rain in mm
    pub rain: f32,
    /// temperature in celsius
    pub temperature: f32,
    /// wind speed in m/h
    pub wind_speed: f32,
    /// relative humidity in %
    pub humidity: f32
}

impl Default for InputElement {
    fn default() -> Self {
        Self {
            rain: NODATAVAL,
            temperature: NODATAVAL,
            wind_speed: NODATAVAL,
            humidity: NODATAVAL
        }
    }
}

pub struct Input {
    pub time: DateTime<Utc>,
    pub data: Array1<InputElement>,
}


// OUTPUT STRUCTURE
#[allow(non_snake_case)]
pub struct OutputElement {
    /// Fine Fuel Moisture Code
    pub ffmc: f32,
    /// Duff Moisture Code
    pub dmc: f32,
    /// Dought Code
    pub dc: f32,
    /// Initial Spread  Index
    pub isi: f32,
    /// Build Up Index
    pub bui: f32,
    /// Fire Weather Index
    pub fwi: f32,
    /// Input rain in mm
    pub rain: f32,
    /// Input temperature in celsius
    pub temperature: f32,
    /// Input relative humidity in %
    pub humidity: f32,
    /// Input wind speed in km/h
    pub wind_speed: f32
}

impl Default for OutputElement {
    fn default() -> Self {
        Self {
            ffmc: NODATAVAL,
            dmc: NODATAVAL,
            dc: NODATAVAL,
            isi: NODATAVAL,
            bui: NODATAVAL,
            fwi: NODATAVAL,
            rain: NODATAVAL,
            temperature: NODATAVAL,
            humidity: NODATAVAL,
            wind_speed: NODATAVAL
        }
    }
}

pub struct Output {
    pub time: DateTime<Utc>,
    pub data: Array1<OutputElement>,
}

#[allow(non_snake_case)]
impl Output {
    pub fn new(time: DateTime<Utc>, data: Array1<OutputElement>) -> Self {
        Self { time, data }
    }

    pub fn get_array(&self, func: fn(&OutputElement) -> f32) -> Array1<f32> {
        let vec = self.data.par_iter().map(func).collect::<Vec<_>>();
        Array1::from_vec(vec)
    }

    pub fn get(&self, variable: &OutputVariableName) -> Option<Array1<f32>> {
        use OutputVariableName::*;
        match variable {
            // Output variables
            ffmc => Some(self.get_array(|o| o.ffmc)),
            dmc => Some(self.get_array(|o| o.dmc)),
            dc => Some(self.get_array(|o| o.dc)),
            isi => Some(self.get_array(|o| o.isi)),
            bui => Some(self.get_array(|o| o.bui)),
            fwi => Some(self.get_array(|o| o.fwi)),

            // Input variables
            rain => Some(self.get_array(|o| o.rain)),
            temperature => Some(self.get_array(|o| o.temperature)),
            humidity => Some(self.get_array(|o| o.humidity)),
            windSpeed => Some(self.get_array(|o| o.wind_speed))
        }
    }
}


// WARM STATE
#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct WarmState {
    pub ffmc: f32,
    pub dmc: f32,
    pub dc: f32
}

impl Default for WarmState {
    fn default() -> Self {
        WarmState {
            ffmc: 80.0,
            dmc: 15.0,
            dc: 255.0,
        }
    }
}


// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct StateElement {
    pub ffmc: f32,
    pub dmc: f32,
    pub dc: f32
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
                    dc: w.dc
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
    fn update_moisture(&mut self, input: &Input) {

        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input_data| {
                update_moisture_fn(state, input_data, &self.config)
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &State, input: &Input) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&self.data)
            .and(&input.data)
            .par_map_collect(|state, input| {
                get_output_fn(state, input, &self.config)
            });

        Output::new(*time, output_data)
    }

    /// Update the state of the cells
    pub fn update(&mut self, input: &Input) {
        let new_time = &input.time;
        self.time = *new_time;
        self.update_moisture(input);
    }

    pub fn output(&self, input: &Input) -> Output {
        self.get_output(input)
    }
}






#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(
    Debug,
    PartialEq,
    Eq,
    Hash,
    Copy,
    Clone,
    EnumString,
    EnumProperty,
    Display,
    Serialize,
    Deserialize,
)]
#[strum(ascii_case_insensitive)]
pub enum OutputVariableName {
    /// Fine Fuel Moisture Code
    #[strum(props(long_name = "Fine Fuel Moisture Code", units = "-"))]
    ffmc,
    /// Duff Moisture Code
    #[strum(props(long_name = "Duff Moisture Code", units = "-"))]
    dmc,
    /// Drought Code
    #[strum(props(long_name = "Drought Code", units = "-"))]
    dc,
    /// Initial Spread Index
    #[strum(props(long_name = "Initial Spread Index", units = "-"))]
    isi,
    /// Build Up Index
    #[strum(props(long_name = "Build Up Index", units = "-"))]
    bui,
    /// Fire Weather Index
    #[strum(props(long_name = "Fire Weather Index", units = "-"))]
    fwi,

    /// Input Rain
    #[strum(props(long_name = "Input Rain", units = "mm"))]
    rain,
    /// Input Temperature
    #[strum(props(long_name = "Input Temperature", units = "°C"))]
    temperature,
    /// Input Relative Humidity
    #[strum(props(long_name = "Input Relative Humidity", units = "%"))]
    humidity,
    /// Input Wind Speed
    #[strum(props(long_name = "Input Wind Speed", units = "m/s"))]
    windSpeed

}