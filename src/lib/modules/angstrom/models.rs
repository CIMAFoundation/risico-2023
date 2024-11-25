use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};

use super::{
    constants::*,
    functions::{store_day_fn, get_output_fn},
};

/// Angstrom index
/// Source: https://wikifire.wsl.ch/tiki-index8902.html?page=Angstr%C3%B6m+index&structure=Fire

// CELLS PROPERTIES
#[derive(Debug)]
pub struct AngstromPropertiesElement {
    pub lon: f32,
    pub lat: f32,
}

#[derive(Debug)]
pub struct AngstromProperties {
    pub data: Array1<AngstromPropertiesElement>,
    pub len: usize,
}

pub struct AngstromCellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
}

impl AngstromProperties {
    pub fn new(props: AngstromCellPropertiesContainer) -> Self {
        let data: Array1<AngstromPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| AngstromPropertiesElement {
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
pub struct AngstromStateElement {
    pub temp_13: f32,  // temperature at 13:00 [°C]
    pub humidity_13: f32,  // relative humidity at 13:00 [%]
}

impl AngstromStateElement {
    // clean the daily values
    pub fn clean_day(&mut self) {
        self.temp_13 = NODATAVAL;
        self.humidity_13 = NODATAVAL;
    }
}


#[derive(Debug)]
pub struct AngstromState {
    pub time: DateTime<Utc>,
    pub data: Array1<AngstromStateElement>,
    len: usize,
}

impl AngstromState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state
    pub fn new(time: &DateTime<Utc>, n_cells: usize) -> AngstromState {
        // initialize as nodata values
        let data: Array1<AngstromStateElement> = Array1::from(
            (0..n_cells)
                .map(|_| AngstromStateElement {
                    temp_13: NODATAVAL,
                    humidity_13: NODATAVAL,
                })
                .collect::<Vec<_>>(),
        );
        AngstromState {
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

    // store the daily values, check if the time is 13:00
    #[allow(non_snake_case)]
    fn store_day(&mut self, input: &Input, props: &AngstromProperties) {
        self.time = input.time;  // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .and(&props.data)
            .par_for_each(|state, input_data, prop_data| {
                store_day_fn(state, input_data, prop_data, &self.time);
            });
    }

    // compute the Angstrom index and return the output
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
    pub fn store(&mut self, input: &Input, props: &AngstromProperties) {
        self.store_day(input, props);
    }

    pub fn output(&mut self) -> Output {
        self.get_output()
    }
}
