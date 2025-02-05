use chrono::{DateTime, Utc, Timelike};
use chrono_tz::Tz;
use tzf_rs::DefaultFinder;
use lazy_static::lazy_static;

use crate::models::{input::InputElement, output::OutputElement};
use super::{
    config::Mark5ModelConfig,
    constants::*,
    models::{Mark5PropertiesElement, Mark5StateElement},
};


lazy_static! {
    static ref TZ_FINDER: DefaultFinder = DefaultFinder::new();
}

// Store the daily info
pub fn store_day_fn(
    state: &mut Mark5StateElement,
    input: &InputElement,
    prop: &Mark5PropertiesElement,
    time: &DateTime<Utc>,
) {
    // info for soil moisture deficit calculation
    // cumulated rain per day
    if input.rain > 0.0 {
        state.cum_rain += input.rain;
    }
    // maximum temperature per day
    if (state.max_temp == NODATAVAL) || (input.temperature > state.max_temp) {
        state.max_temp = input.temperature;
    }
    // store the other daily info -> values at 3pm local time
    let tz_name = TZ_FINDER.get_tz_name(prop.lon as f64, prop.lat as f64);
    let tz : Tz = tz_name.parse().expect("Invalid timezone name");
    let local_time = time.with_timezone(&tz);
    // Store the daily info at 15 local time
    if local_time.hour() <= TIME_WEATHER {  // save the last available data
        state.temp_15 = input.temperature;
        state.humidity_15 = input.humidity;
        state.wind_speed_15 = input.wind_speed;
    }
}


// Forest Fire Danger Index - FFDI
pub fn ffdi(
    temp: f32,  // °C
    rh: f32,  // %
    wind_speed: f32,  // m/h
    drought_factor: f32,  // [adim]
) -> f32 {
    // conversion of wind speed from m/h to km/h
    let ws_kph = wind_speed / 1000.0;
    // calculation of the FFDI
    
    2.0*f32::exp(-0.45+0.987*f32::ln(drought_factor)-0.0345*rh+0.0338*temp+0.0234*ws_kph)
}


// Drougth Factor
// Calculation from Finkele et al. 2006
pub fn drought_factor(
    time: &DateTime<Utc>,  // time of computation
    smd: f32,  // Soil Moisture Deficit [mm]
    dates: &Vec<DateTime<Utc>>,  // dates associated with the dalily_rain vector
    daily_rain: &Vec<f32>  // daily rain [mm]
) -> f32 {
    // find the rain events
    let rain_events = find_rain_events(*time, dates, daily_rain);
    // calculate the rainfall effects for each rain event
    let rain_effects: Vec<f32> = if rain_events.is_empty() {
        vec![1.0]
    } else {
        rain_events
            .iter()
            .map(|(rain, age)| rainfall_effect(*rain, *age))
            .collect()
    };
    // get the minimum rainfall effect among the rainfall_effects vector
    let min_rain_effect = rain_effects.iter().cloned().min_by(|a, b| a.partial_cmp(b).unwrap());
    // limitation used operationally by the Bureau of Meteorology (Australia), see Finkele et al. 2006
    let xlim = if smd < 20.0 {
        1.0 / (1.0 + 0.1135*smd)
    } else {
        75.0 / (270.525 - 1.267*smd)
    };
    let rain_effect_eff = f32::min(min_rain_effect.unwrap_or(0.0), xlim);
    // calculation of the drought factor
    let df = 10.5*(1.0-f32::exp(-(smd+30.0)/40.0))*((41.0*f32::powf(rain_effect_eff, 2.0) + rain_effect_eff) / (40.0*f32::powf(rain_effect_eff, 2.0) + rain_effect_eff + 1.0));
    // normalize taking the minimum value between df and 10
    f32::min(df, 10.0)
}

pub fn rainfall_effect(
    rainfall_event: f32,  // sum of rainfall within the rain event [mm]
    age_event: i64,  // number of days since the day with the largest daily rainfall amount within the rain event
) -> f32 {
    
    if rainfall_event < RAIN_TH {
        1.0
    } else {
        let age_event_eff: f32 = if age_event == 0 {
            0.8
        } else {
            age_event as f32
        };
        f32::powf(age_event_eff, 1.3) / (f32::powf(age_event_eff, 1.3) + rainfall_event - RAIN_TH)
    }
}


pub fn find_rain_events(
    time: DateTime<Utc>,  // time of computation
    dates: &Vec<DateTime<Utc>>,  // dates associated with the dalily_rain vector
    daily_rain: &Vec<f32>  // daily rain [mm]
) -> Vec<(f32, i64)> {
    // A "rain event" is defines as a set of consecutive "rainy days"
    // A "rainy day" happens when the daily rain is greater than rain threshold (RAIN_TH=2mm)
    // A "rain event" is characterized by:
    // 1. the total rain occurred;
    // 2. the number of days of distance between the day with maximum rain cumulation and current time
    let mut rain_events: Vec<(f32, i64)> = Vec::new();
    let mut idx = 0;
    while idx < daily_rain.len() {
        let mut j = idx;
        let mut rain_cum = 0.0;
        let mut max_rain = 0.0;
        let mut idx_max_rain = j;
        while (j < daily_rain.len()) && (daily_rain[j] > RAIN_TH) {
            rain_cum += daily_rain[j];
            if daily_rain[j] > max_rain {
                max_rain = daily_rain[j];
                idx_max_rain = j;
            }
            j += 1;
        }
        idx = j + 1;
        if rain_cum > 0.0 {  // there is a rain event
            let n_days = (time - dates[idx_max_rain]).num_days();
            rain_events.push((rain_cum, n_days));
        }
    }
    rain_events
}

// Output function
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &mut Mark5StateElement,
    props: &Mark5PropertiesElement,
    config: &Mark5ModelConfig,
    time: &DateTime<Utc>,
) -> OutputElement {
    // store the datetime and cumulated rain for the day of the run
    state.update(time, state.cum_rain);
    // get the last rains in the time windows -> they are already ordered from oldest to newest
    let (dates, daily_rains) = state.get_time_window(time);

    // update the soil moisture deficit
    if state.max_temp != NODATAVAL {
        state.smd = config.update_smd(state.smd, state.max_temp, &daily_rains, props.mean_rain);
    }
    // calculate the drought factor
    let mut df = drought_factor(time, state.smd, &dates, &daily_rains);
    if df < 0.0 {
        df = 0.0
    } ;
    // calculate the FFDI
    let ffdi = if (state.temp_15 == NODATAVAL) || (state.humidity_15 == NODATAVAL) || (state.wind_speed_15 == NODATAVAL) {
        NODATAVAL
    } else {
        ffdi(state.temp_15, state.humidity_15, state.wind_speed_15, df)
    };
    // return output
    config.get_output(state.smd, df, ffdi, state.temp_15, state.cum_rain, state.wind_speed_15, state.humidity_15)
}

pub fn kbdi_output(
    smd: f32, // Soil Moisture Deficit [mm]
    df: f32,  // Drought Factor [-]
    ffdi: f32,  // Forest Fire Danger Index [-]
    temperature: f32,  // Temperature [°C]
    rain: f32,  // Rain (cumulated) [mm]
    wind_speed: f32, // Wind Speed [m/h]
    humidity: f32, // Relative Humidity [%]
) -> OutputElement {
    let wind_speed_out = wind_speed / 3600.0;  // conversion from m/h to m/s
    OutputElement {
        kbdi: smd,
        df,
        ffdi,
        temperature,
        rain,
        wind_speed: wind_speed_out,
        humidity,
        ..OutputElement::default()
    }
}