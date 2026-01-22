use crate::models::{input::Input, output::Output};
use chrono::prelude::*;
use itertools::izip;
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
}

pub type FWIStateData = (Vec<DateTime<Utc>>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>);

impl FWIStateElement {
    pub fn get_time_window(&self, time: &DateTime<Utc>) -> FWIStateData {
        // zip with dates and take only moistures where history < 24 hours
        let combined = izip!(
            self.dates.iter(),
            self.ffmc.iter(),
            self.dmc.iter(),
            self.dc.iter(),
            self.rain.iter()
        )
        .filter(|(t, _, _, _, _)| time.signed_duration_since(**t).num_hours() < TIME_WINDOW)
        .map(|(t, f, d, c, r)| (*t, *f, *d, *c, *r))
        .collect::<Vec<_>>();
        let dates: Vec<DateTime<Utc>> = combined.iter().map(|(t, _, _, _, _)| *t).collect();
        let ffmc: Vec<f32> = combined.iter().map(|(_, f, _, _, _)| *f).collect();
        let dmc: Vec<f32> = combined.iter().map(|(_, _, d, _, _)| *d).collect();
        let dc: Vec<f32> = combined.iter().map(|(_, _, _, c, _)| *c).collect();
        let rain: Vec<f32> = combined.iter().map(|(_, _, _, _, r)| *r).collect();
        (dates, ffmc, dmc, dc, rain)
    }

    pub fn get_initial_moisture(&self, time: &DateTime<Utc>) -> (f32, f32, f32) {
        // get the initial value of the moisture variables for computation
        let (_, ffmc_tw, dmc_tw, dc_tw, _) = self.get_time_window(time);
        let ffmc_initial = *ffmc_tw.first().unwrap_or(&FFMC_INIT);
        let dmc_initial = *dmc_tw.first().unwrap_or(&DMC_INIT);
        let dc_initial = *dc_tw.first().unwrap_or(&DC_INIT);
        (ffmc_initial, dmc_initial, dc_initial)
    }

    pub fn update(&mut self, time: &DateTime<Utc>, ffmc: f32, dmc: f32, dc: f32, rain: f32) {
        // add new values
        self.dates.push(*time);
        self.ffmc.push(ffmc);
        self.dmc.push(dmc);
        self.dc.push(dc);
        self.rain.push(rain);
        // get the time window
        let (new_dates, new_ffmc, new_dmc, new_dc, new_rain) = self.get_time_window(time);
        // update the values
        self.dates = new_dates;
        self.ffmc = new_ffmc;
        self.dmc = new_dmc;
        self.dc = new_dc;
        self.rain = new_rain;
    }
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
                .map(|w| FWIStateElement {
                    dates: w.dates.clone(),
                    ffmc: w.ffmc.clone(),
                    dmc: w.dmc.clone(),
                    dc: w.dc.clone(),
                    rain: w.rain.clone(),
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
    pub fn get_output(self: &FWIState, input: &Input) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&self.data)
            .and(&input.data)
            .par_map_collect(|state, input| get_output_fn(state, input, &self.config));

        Output::new(*time, output_data)
    }

    /// Update the state of the cells
    pub fn update(&mut self, props: &FWIProperties, input: &Input) {
        let new_time = &input.time;
        self.time = *new_time;
        self.update_state(props, input);
    }

    pub fn output(&self, input: &Input) -> Output {
        self.get_output(input)
    }
}
