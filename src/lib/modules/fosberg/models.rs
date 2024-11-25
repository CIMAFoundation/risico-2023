use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::get_output_fn,
};

/// Fosberg  Fire Weather index
/// Source: https://wikifire.wsl.ch/tiki-indexb1d5.html?page=Fosberg+fire+weather+index&structure=Fire

// CELLS PROPERTIES
#[derive(Debug)]
pub struct FosbergPropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct FosbergProperties {
    pub data: Array1<FosbergPropertiesElement>,
    pub len: usize,
}

pub struct FosbergCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl FosbergProperties {
    pub fn new(props: FosbergCellPropertiesContainer) -> Self {
        let data: Array1<FosbergPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| FosbergPropertiesElement {
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
pub struct FosbergStateElement {
    pub temp: f32,  // temperature [°C]
    pub humidity: f32,  // relative humidity [%]
    pub wind_speed: f32,  // wind speed [m/h]
}


#[derive(Debug)]
pub struct FosbergState {
    pub time: DateTime<Utc>,
    pub data: Array1<FosbergStateElement>,
    len: usize,
}

impl FosbergState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(time: &DateTime<Utc>, n_cells: usize) -> FosbergState {
        let data: Array1<FosbergStateElement> = Array1::from(
            (0..n_cells)
                .map(|_| FosbergStateElement {
                    temp: NODATAVAL,
                    humidity: NODATAVAL,
                    wind_speed: NODATAVAL,
                })
                .collect::<Vec<_>>(),
        );

        FosbergState {
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
    fn update_fn(&mut self, input: &Input) {
        self.time = input.time;  // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input_data| {
                state.temp = input_data.temperature;
                state.humidity = input_data.humidity;
                state.wind_speed = input_data.wind_speed;
            });
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

    pub fn update(&mut self, input: &Input) {
        self.update_fn(input);
    }

    pub fn output(&mut self) -> Output {
        self.get_output()
    }
}