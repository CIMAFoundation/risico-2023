use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use ndarray::{Array1, Zip};
use itertools::izip;

use super::{
    constants::*,
    config::KBDIModelConfig,
    functions::{store_day_fn, get_output_fn},
};

// CELLS PROPERTIES
#[derive(Debug)]
pub struct KBDIPropertiesElement {
    pub lon: f32,
    pub lat: f32,
    pub mean_rain: f32,
}

#[derive(Debug)]
pub struct KBDIProperties {
    pub data: Array1<KBDIPropertiesElement>,
    pub len: usize,
}

pub struct KBDICellPropertiesContainer {
    pub lons: Vec<f32>,
    pub lats: Vec<f32>,
    pub mean_rains: Vec<f32>,
}

impl KBDIProperties {
    pub fn new(props: KBDICellPropertiesContainer) -> Self {
        let data: Array1<KBDIPropertiesElement> = props
            .lons
            .iter()
            .enumerate()
            .map(|(idx, lon)| KBDIPropertiesElement {
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
pub struct KBDIWarmState {
    pub dates: Vec<DateTime<Utc>>,  // dates of the previous 20 days (default time window)
    pub daily_rain: Vec<f32>,  // daily rain of the previous 20 days (default time window)
    pub kbdi: f32,  // Keetch-Byram Dorugh Index of the previous day
}

impl Default for KBDIWarmState {
    fn default() -> Self {
        Self {
            dates: vec![],
            daily_rain: vec![],
            kbdi: KBDI_INIT,
        }
    }
}

// STATE
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct KBDIStateElement {
    pub dates: Vec<DateTime<Utc>>,  // dates of the previous 20 days (default time window)
    pub daily_rain: Vec<f32>,  // daily rain of the previous 20 days (default time window)
    pub kbdi: f32,  // Soil Moisture Deficit
    pub cum_rain: f32,  // cumulated rain on the run day
    pub temperature: f32,  // temperature info on the run day
}


impl KBDIStateElement {

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
        self.temperature = NODATAVAL;
    }
}

#[derive(Debug)]
pub struct KBDIState {
    pub time: DateTime<Utc>,
    pub data: Array1<KBDIStateElement>,
    len: usize,
    config: KBDIModelConfig,
}

impl KBDIState {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &[KBDIWarmState], time: &DateTime<Utc>, config: KBDIModelConfig) -> KBDIState {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| KBDIStateElement {
                    dates: w.dates.clone(),
                    daily_rain: w.daily_rain.clone(),
                    kbdi: w.kbdi.clone(),
                    cum_rain: 0.0,  // start with 0 cumulated rain
                    temperature: NODATAVAL,
                })
                .collect(),
        );

        KBDIState {
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
    fn store_day(&mut self, input: &Input) {
        self.time = input.time;  // reference time of the input
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input_data| {
                store_day_fn(state, input_data);
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(&mut self, props: &KBDIProperties) -> Output {
        let time = &self.time;
        let output_data = Zip::from(&mut self.data)
                    .and(&props.data)
                    .par_map_collect(|state, props_data| {
                        get_output_fn(state, props_data, &self.config, &time)
                    });
        // clean the daily values
        self.data.iter_mut().for_each(|state| state.clean_day());
        Output::new(*time, output_data)
    }

    // Update the state of the cells
    pub fn store(&mut self, input: &Input) {
        self.store_day(input);
    }

    pub fn output(&mut self, props: &KBDIProperties) -> Output {
        self.get_output(props)
    }
}
