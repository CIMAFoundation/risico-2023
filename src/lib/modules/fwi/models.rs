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
    pub humidity: Vec<f32>,
    pub temperature: Vec<f32>,
    pub wind_speed: Vec<f32>,
    pub rain24h: Vec<f32>
}


impl FWIStateElement {

    pub fn get_weather_time_window(&self, time: &DateTime<Utc>) -> (Vec<DateTime<Utc>>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
        // zip with dates and take only weather < 24 hours
        let combined = izip!(
            self.dates.iter(),
            self.rain.iter(),
            self.humidity.iter(),
            self.temperature.iter(),
            self.wind_speed.iter(),
            self.rain24h.iter()
        )
        .filter(|(t, _, _, _, _, _)| time.signed_duration_since(**t) < chrono::Duration::hours(TIME_WINDOW.into()))
        .map(|(t, r, h, temp , w, r24)| (*t, *r, *h, *temp, *w, *r24))
        .collect::<Vec<_>>();
        let dates: Vec<DateTime<Utc>> = combined.iter().map(|(t, _, _, _, _, _)| *t).collect();
        let rain: Vec<f32> = combined.iter().map(|(_, r, _, _, _, _)| *r).collect();
        let humidity: Vec<f32> = combined.iter().map(|(_, _, h, _, _, _)| *h).collect();
        let temperature: Vec<f32> = combined.iter().map(|(_, _, _, temp, _, _)| *temp).collect();
        let wind_speed: Vec<f32> = combined.iter().map(|(_, _, _, _, w, _)| *w).collect();
        let rain24h: Vec<f32> = combined.iter().map(|(_, _, _, _, _, r24)| *r24).collect();
        (dates, rain, humidity, temperature, wind_speed, rain24h)
    }

    pub fn update_weather(&mut self, time: &DateTime<Utc>, rain: f32, humidity: f32, temperature: f32, wind_speed: f32, rain24h: f32) {
        // add new values
        self.dates.push(*time);
        self.rain.push(rain);
        self.humidity.push(humidity);
        self.temperature.push(temperature);
        self.wind_speed.push(wind_speed);
        self.rain24h.push(rain24h);
        // get the time window
        let (new_dates, new_rain, new_humidity, new_temperature, new_wind_speed, new_rain24h) = self.get_weather_time_window(time);
        // update the values
        self.dates = new_dates;
        self.rain = new_rain;
        self.humidity = new_humidity;
        self.temperature = new_temperature;
        self.wind_speed = new_wind_speed;
        self.rain24h = new_rain24h;
    }

    pub fn get_moisture(&self) -> (f32, f32, f32) {
        let ffmc = self.ffmc.iter().copied().last().unwrap_or(FFMC_INIT);
        let dmc = self.dmc.iter().copied().last().unwrap_or(DMC_INIT);
        let dc = self.dc.iter().copied().last().unwrap_or(DC_INIT);
        (ffmc, dmc, dc)
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
