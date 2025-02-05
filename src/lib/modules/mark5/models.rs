use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};
use itertools::izip;

use super::{
    constants::*,
    config::Mark5ModelConfig,
    functions::{store_day_fn, get_output_fn},
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
            .map(|(idx, lon)| Mark5PropertiesElement {
                lon: *lon,
                lat: props.lats[idx],
                mean_rain: props.mean_rains[idx],
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
pub struct Mark5WarmState {
    pub dates: Vec<DateTime<Utc>>,  // dates of the previous time window
    pub daily_rain: Vec<f32>,  // daily rain [mm] of the previous time window
    pub smd: f32,  // Soil Moisture Deficit [mm] of the previous day
}

impl Default for Mark5WarmState {
    fn default() -> Self {
        Self {
            dates: vec![],
            daily_rain: vec![],
            smd: SMD_INIT,
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct Mark5StateElement {
    pub dates: Vec<DateTime<Utc>>,  // dates of the previous time window
    pub daily_rain: Vec<f32>,  // daily rain [mm] of the previous time window
    pub smd: f32,  // Soil Moisture Deficit [mm]
    pub cum_rain: f32,  // cumulated rain [mm] on the run day
    pub max_temp: f32,  // maximum daily temperature [°C] info on the run day
    pub temp_15: f32,  // temperature [°C] at 3pm info on the run day
    pub humidity_15: f32,  // relative humidity [%] at 3pm
    pub wind_speed_15: f32,  // wind speed [m/h] info on the run day at 3pm
}


impl Mark5StateElement {

    pub fn get_time_window(&self, time: &DateTime<Utc>) -> (Vec<DateTime<Utc>>, Vec<f32>) {
        // zip with dates and take only cumulated rain where history < 20 days (default time window)
        let mut combined = izip!(
            self.dates.iter(),
            self.daily_rain.iter())
        .filter(|(t, _)| time.signed_duration_since(**t).num_days() <= TIME_WINDOW)
        .map(|(t, r)| (*t, *r))
        .collect::<Vec<_>>();
        // order the values according to the dates
        combined.sort_by(|a: &(DateTime<Utc>, f32), b| a.0.cmp(&b.0));
        // get the dates and daily rain
        let dates: Vec<DateTime<Utc>> = combined.iter().map(|(t, _)| *t).collect();
        let daily_rain: Vec<f32> = combined.iter().map(|(_, r)| *r).collect();
        (dates, daily_rain)
    }

    pub fn update(&mut self,
        time: &DateTime<Utc>,
        rain_of_day: f32  // mm, daily run to be add to the history
    ) {
        // add new values
        self.dates.push(*time);
        self.daily_rain.push(rain_of_day);
        // get the time window
        let (new_dates, new_rain) = self.get_time_window(time);
        // update the values
        self.dates = new_dates;
        self.daily_rain = new_rain;
    }

    pub fn clean_day(
        &mut self
    ) {
        self.cum_rain = 0.0;
        self.max_temp = NODATAVAL;
        self.temp_15 = NODATAVAL;
        self.humidity_15 = NODATAVAL;
        self.wind_speed_15 = NODATAVAL;
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
                    smd: w.smd,
                    cum_rain: 0.0,  // start with 0 cumulated rain
                    max_temp: NODATAVAL,
                    temp_15: NODATAVAL,
                    humidity_15: NODATAVAL,
                    wind_speed_15: NODATAVAL,
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
    fn store_day(&mut self, input: &Input, prop: &Mark5Properties) {
        let time = input.time;  // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .and(&prop.data)
            .par_for_each(|state, input_data, prop_data| {
                store_day_fn(state, input_data, prop_data, &time);
            });
        self.time = time;
    }

    #[allow(non_snake_case)]
    pub fn get_output(&mut self, props: &Mark5Properties) -> Output {
        let time = &self.time;
        let output_data = Zip::from(&mut self.data)
                    .and(&props.data)
                    .par_map_collect(|state, props_data| {
                        get_output_fn(state, props_data, &self.config, time)
                    });
        // clean the daily values
        self.data.iter_mut().for_each(|state| state.clean_day());
        Output::new(*time, output_data)
    }

    // Update the state of the cells
    pub fn store(&mut self, input: &Input, prop: &Mark5Properties) {
        self.store_day(input, prop);
    }

    pub fn output(&mut self, props: &Mark5Properties) -> Output {
        self.get_output(props)
    }
}
