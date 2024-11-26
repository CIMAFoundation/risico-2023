use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::get_output_fn,
};


/// Sharples fire index
/// Source: https://wikifire.wsl.ch/tiki-index91e2.html?page=Sharples+fuel+moisture+and+fire+danger+rating+indices&structure=Fire


// CELLS PROPERTIES
#[derive(Debug)]
pub struct SharplesPropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct SharplesProperties {
    pub data: Array1<SharplesPropertiesElement>,
    pub len: usize,
}

pub struct SharplesCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl SharplesProperties {
    pub fn new(props: SharplesCellPropertiesContainer) -> Self {
        let data: Array1<SharplesPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| SharplesPropertiesElement {
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


// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct SharplesStateElement {
    pub temp: f32,  // temperature [°C]
    pub humidity: f32,  // relative humidity [%]
    pub wind_speed: f32,  // wind speed [m/h]
}


#[derive(Debug)]
pub struct SharplesState {
    pub time: DateTime<Utc>,
    pub data: Array1<SharplesStateElement>,
    len: usize,
}

impl SharplesState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state
    pub fn new(time: &DateTime<Utc>, n_cells: usize) -> SharplesState {
        let data: Array1<SharplesStateElement> = Array1::from(
            (0..n_cells)
                .map(|_| SharplesStateElement {
                    temp: NODATAVAL,
                    humidity: NODATAVAL,
                    wind_speed: NODATAVAL,
                })
                .collect::<Vec<_>>(),
        );
        SharplesState {
            time: *time,
            data,
            len: n_cells,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(non_snake_case)]
    pub fn get_output(&mut self) -> Output {
        let time = &self.time;
        let output_data = self.data
                    .map(|state| {
                        get_output_fn(state)
                    });
        Output::new(*time, output_data)
    }

    pub fn store(&mut self, input: &Input) {
        self.time = input.time;  // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input_data| {
                state.temp = input_data.temperature;
                state.humidity = input_data.humidity;
                state.wind_speed = input_data.wind_speed;
            });
    }

    pub fn output(&mut self) -> Output {
        self.get_output()
    }
}