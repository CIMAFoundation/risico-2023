use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::get_output_fn,
};

/// Hor-Dry-Wind Fire Weather index
/// Source: Srock, A.F.; Charney, J.J.; Potter, B.E.; Goodrick, S.L. The Hot-Dry-Windy Index: A New Fire Weather Index. Atmosphere 2018, 9, 279. https://doi.org/10.3390/atmos9070279

// CELLS PROPERTIES
#[derive(Debug)]
pub struct HdwPropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct HdwProperties {
    pub data: Array1<HdwPropertiesElement>,
    pub len: usize,
}

pub struct HdwCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl HdwProperties {
    pub fn new(props: HdwCellPropertiesContainer) -> Self {
        let data: Array1<HdwPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| HdwPropertiesElement {
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
pub struct HdwStateElement {
    pub vpd: f32,  // vapor pressure deificit [hPa]
    pub wind_speed: f32,  // wind speed [m/h]
}


#[derive(Debug)]
pub struct HdwState {
    pub time: DateTime<Utc>,
    pub data: Array1<HdwStateElement>,
    len: usize,
}

impl HdwState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(time: &DateTime<Utc>, n_cells: usize) -> HdwState {
        let data: Array1<HdwStateElement> = Array1::from(
            (0..n_cells)
                .map(|_| HdwStateElement {
                    vpd: NODATAVAL,
                    wind_speed: NODATAVAL,
                })
                .collect::<Vec<_>>(),
        );
        HdwState {
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
                state.vpd = input_data.vpd;
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