use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};
use itertools::izip;

use super::{
    constants::*,
    config::Mark5ModelConfig,
    functions::{get_output_fn, update_state_fn},
};

// CELLS PROPERTIES
#[derive(Debug)]
pub struct Mark5PropertiesElement {
    pub lon: f32,
    pub lat: f32,
    pub mean_rain: f32,
}

#[derive(Debug)]
pub struct Mark5Properties {
    pub data: Array1<Mark5PropertiesElement>,
    pub len: usize,
}

pub struct Mark5CellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
    pub mean_rains: Vec<f32>,
}

impl Mark5Properties {
    pub fn new(props: Mark5CellPropertiesContainer) -> Self {
        let data: Array1<Mark5PropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, v)| Mark5PropertiesElement {
                lon: props.lons[idx],
                lat: props.lats[idx],
                mean_rain: props.mean_rains[idx],
            })
            .collect();
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
#[derive(Default)]
pub struct Mark5WarmState {
    pub dates: Vec<DateTime<Utc>>,  // dates of the previous 20 days
    pub daily_rain: Vec<f32>,  // daily rain of the previous 20 days
    pub kbdi: f32,  // kbdi of the previous day
}


// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct Mark5StateElement {
    pub dates: Vec<DateTime<Utc>>,
    pub daily_rain: Vec<f32>,
    pub kbdi: f32,
    pub cum_rain: f32,  // cumulation of rain per day
    pub max_temp: f32,  // max temperature per day
    pub min_rh: f32,  // min relative humidity per day
    pub max_wind: f32,  // max wind per day
}


impl Mark5StateElement {

    pub fn get_time_window(&self, time: &DateTime<Utc>) -> (Vec<DateTime<Utc>>, Vec<f32>) {
        // zip with dates and take only cumulated rain where history < 20 days
        let combined = izip!(
            self.dates.iter(),
            self.daily_rain.iter())
        .filter(|(t, _)| time.signed_duration_since(**t).num_days() <= TIME_WINDOW)
        .map(|(t, r)| (*t, *r))
        .collect::<Vec<_>>();
        let dates: Vec<DateTime<Utc>> = combined.iter().map(|(t, _)| *t).collect();
        let daily_rain: Vec<f32> = combined.iter().map(|(_, r)| *r).collect();
        (dates, daily_rain)
    }

    pub fn update(&mut self, time: &DateTime<Utc>, rain: f32) {
        // add new values
        self.dates.push(*time);
        self.daily_rain.push(rain);
        // get the time window
        let (new_dates, new_rain) = self.get_time_window(time);
        // update the values
        self.dates = new_dates;
        self.rain = new_rain;
    }

    pub fn clear_day(
        &mut self
    ) {
        self.cum_rain = 0.0;
        self.max_temp = -1000.0;
        self.min_rh = 100.0;
        self.max_wind = 0.0;
    }
}

#[derive(Debug)]
pub struct Mark5State {
    pub time: DateTime<Utc>,
    pub data: Array1<Mark5StateElement>,
    len: usize,
    config: Mark5ModelConfig,
}

impl Mark5State {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &[Mark5WarmState], time: &DateTime<Utc>, config: Mark5ModelConfig) -> Mark5State {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| Mark5StateElement {
                    dates: w.dates.clone(),
                    daily_rain: w.daily_rain.clone(),
                    kbdi: w.kbdi.clone(),
                })
                .collect(),
        );

        Mark5State {
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
    fn update_state(&mut self, props: &Mark5Properties, input: &Input) {
        let time = &self.time;
        Zip::from(&mut self.data)
            .and(&props.data)
            .and(&input.data)
            .par_for_each(|state, props, input_data| {
                update_state_fn(state, props, input_data, time, &self.config)
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &Mark5State, input: &Input) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&self.data)
            .and(&input.data)
            .par_map_collect(|state, input| get_output_fn(state, input, &self.config));

        Output::new(*time, output_data)
    }

    /// Update the state of the cells
    pub fn update(&mut self, props: &Mark5Properties, input: &Input) {
        let new_time = &input.time;
        self.time = *new_time;
        self.update_state(props, input);
    }

    pub fn output(&self, input: &Input) -> Output {
        self.get_output(input)
    }
}
