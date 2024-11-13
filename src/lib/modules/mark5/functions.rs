use chrono::{DateTime, Utc};

use crate::models::{input::InputElement, output::OutputElement};

use super::{
    config::Mark5ModelConfig,
    constants::*,
    models::{Mark5PropertiesElement, Mark5StateElement},
};

// Keetch-Byram Drought Index
// Source: WikiFire
// It is expressed as soil moisture deficit in mm, and ranges from 0 mm (wet) to 200 mm (dry)
pub fn kbdi_update(
    state: &mut Mark5StateElement,
    props: &Mark5PropertiesElement,
    time: &DateTime<Utc>,
) {
    // the rain of today
    let rain_of_day = state.cum_rain;
    // get the last rains in the time windows
    let (_, daily_rains) = state.get_time_window(time);
    
    // calculate the rain of the last days (in case they are consecutive)
    let mut last_rain = 0.0;
    let mut idx = daily_rains.len() - 1;
    while daily_rains[idx] > 0.0 {
        last_rain += daily_rains[idx];
        idx -= 1;
    }
    // effective rain of the day
    let effective_rain = f32::max(0.0, rain_of_day - f32::max(0.0, KBDI_RAIN_RUNOFF - last_rain));
    let dt: f32 = 1.0;  // DAILY COMPUTATION
    let kbdi: f32 = state.smd;  // previous KBDI value
    let evapo_transp: f32 =  (((203.2-kbdi) * (0.968*f32::exp(0.0875*state.temperature+1.5552)-8.3) * dt) / (1.0+10.88*f32::exp(-0.001736*props.mean_rain)))*10e-3;
    let new_kbdi = (kbdi - effective_rain) + evapo_transp;
    // store the new KBDI as soil moisture deficit value
    state.smd = new_kbdi;
}


// Forest Fire Danger Index - FFDI
pub fn ffdi(
    temp: f32,  // °C
    rh: f32,  // %
    wind_speed_max: f32,  // m/h
    drought_factor: f32,
) -> f32 {
    // conversion of wind speed from m/h to km/h
    let wind_speed = wind_speed_max * 3.6;
    // calculation of the FFDI
    let ffdi = 2.0*f32::exp(-0.45+0.987*f32::ln(drought_factor)-0.0345*rh+0.0338*temp+0.0234*wind_speed);
    ffdi
}


// Drougth Factor
pub fn drought_factor(
    smd: f32,  // Soil Moisture Deficit
    rainfall_effect: f32,  // rainfall effect
) -> f32 {
    let df = 10.5*(1.0-f32::exp(-(smd+30.0)/40.0))*((41.0*f32::powf(rainfall_effect, 2.0) + rainfall_effect) / (40.0*f32::powf(rainfall_effect, 2.0) + rainfall_effect + 1.0));
    // normalize taking the minimum value between df and 10
    f32::min(df, 10.0)
}

pub fn rainfall_effect(
    smd: f32,  // Soil Moisture Deficit
    rainfall_event: f32,  // mm, sum of rainfall within the rain event
    event_age: i64,  // number of days since the day with the largest daily rainfall amount within the rain event
) -> f32 {
    let x = if rainfall_event < RAIN_TH {
        1.0
    } else {
        if event_age == 0 {
            f32::powf(0.8, 1.3) / (f32::powf(0.8, 1.3) + rainfall_event - RAIN_TH)
        } else {
            f32::powf(event_age as f32, 1.3) / (f32::powf(event_age as f32, 1.3) + rainfall_event - RAIN_TH)
        }
    };
    // limitation used operationally, see Finkele et al. 2006
    let xlim = if smd < 20.0 {
        1.0 / (1.0 + 0.1135*smd)
    } else {
        75.0 / (270.525 - 1.267*smd)
    };
    let x_eff = f32::min(x, xlim);
    x_eff
}


pub fn rain_events(time: DateTime<Utc>, dates: Vec<DateTime<Utc>>, daily_rain: Vec<f32>) -> Vec<(f32, i64)> {
    // a rain event is defines as a set of consecutive rainy days, and characterized by:
    // 1. the total rain;
    // 2. the number of days of distance between the day with maximum rain cumulation and current time
    // A rainy day happens when the daily rain is greater than rain threshold
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
        idx = j;
        if rain_cum > 0.0 {
            let n_days = (time - dates[idx_max_rain]).num_days();
            rain_events.push((rain_cum, n_days));
        }
    }
    rain_events
}


pub fn store_day_extremes(
    state: &mut Mark5StateElement,
    input: &InputElement,
    _time: &DateTime<Utc>,
) {
    // maximum temperature per day
    if (state.temperature == NODATAVAL) || (input.temperature > state.temperature) {
        state.temperature = input.temperature;
    }
    // minimum relative humidity per day
    if (state.humidity == NODATAVAL) || (input.humidity < state.humidity) {
        state.humidity = input.humidity;
    }
    // maximum wind per day
    if (state.wind_speed == NODATAVAL) || (input.wind_speed > state.wind_speed) {
        state.wind_speed = input.wind_speed;
    }
}


pub fn store_day_fn(
    state: &mut Mark5StateElement,
    input: &InputElement,
    config: &Mark5ModelConfig,
    time: &DateTime<Utc>,
) {
    // cumulated rain per day
    state.cum_rain += input.rain;
    // store the other daily info
    // usual options: extremes / values at 3pm local time
    config.store_day(state, input, time);
}


pub fn update_state_fn(
    state: &mut Mark5StateElement,
    props: &Mark5PropertiesElement,
    config: &Mark5ModelConfig,
    time: &DateTime<Utc>,
) {
    // update the soil moisture deficit
    config.update_smd(state, props, time);
    // store the datetime and cumulated rain for the day of the run
    state.update(time, state.cum_rain);
}


#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &Mark5StateElement,
    config: &Mark5ModelConfig,
    time: &DateTime<Utc>,
) -> OutputElement {
    // get the last rains in the time windows
    let (dates, daily_rains) = state.get_time_window(time);
    // calculate the rainfall events
    let rainfall_events = rain_events(*time, dates, daily_rains);
    // calculate the rainfall effects
    let rainfall_effects: Vec<f32> = rainfall_events
        .iter()
        .map(|(rain, age)| rainfall_effect(state.smd, *rain, *age))
        .collect();
    // get the minimum rainfall effect among the rainfall_effects vector
    let min_rainfall_effect = rainfall_effects.iter().cloned().min_by(|a, b| a.partial_cmp(b).unwrap());

    // calculate the drought factor
    let df = drought_factor(state.smd, min_rainfall_effect.unwrap_or(1.0));
    // calculate the FFDI
    let ffdi = ffdi(state.temperature, state.humidity, state.wind_speed, df);

    // return output
    config.get_output(state, df, ffdi)
}


pub fn kbdi_output(
    state: &Mark5StateElement,
    df: f32,
    ffdi: f32,
) -> OutputElement {
    OutputElement {
        kbdi: state.smd,
        df,
        ffdi,
        temperature: state.temperature,
        rain: state.cum_rain,
        wind_speed: state.wind_speed,
        humidity: state.humidity,
        ..OutputElement::default()
    }
}