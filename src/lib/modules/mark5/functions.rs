use chrono::{DateTime, Datelike, Utc};
use itertools::izip;

use crate::models::{input::InputElement, output::OutputElement};

use super::{
    config::Mark5ModelConfig,
    constants::*,
    models::{Mark5PropertiesElement, Mark5StateElement},
};

// Keetch-Byram Drought Index
pub fn kbdi_update(
    kbdi: f32,
    daily_rain: f32,  // mm
    temp_max: f32,  // °C
    mean_rain: f32,  // mm
    runoff: bool
) -> f32 {
    let effective_rain = if runoff {
        daily_rain - 5
    } else {
        daily_rain
    };
    let evapo_transp: f32 =  ((203.2-kbdi) * (0.968*f32::exp(0.0875*temp_max+1.5552)-8.3) / (1+10.88*f32::exp(-0.00173*mean_rain)))*10e-3;
    let new_kbdi = kbdi - effective_rain + evapo_transp;
    new_kbdi
}

// Drougth Factor
pub fn rainfall_effect(
    smd: f32,  // Soil Moisture Deficit
    rainfall_event: f32,  // mm
    event_age: i64,  // days
) -> f32 {
    let xlim = if smd < 20.0 {
        1.0 / (1.0 + 0.1135*smd)
    } else {
        75.0 / (270.525 - 1.267*smd)
    };
    let x = if rainfall_event < 2 {
        1.0
    } else {
        if event_age == 0 {
            f32::powf(0.8, 1.3) / (f32::powf(0.8, 1.3) + rainfall_event -2.0)
        } else {
            f32::powf(event_age, 1.3) / (f32::powf(event_age, 1.3) + rainfall_event -2.0)
        }
    };
    let x_eff = f32::min(x, xlim);
    x_eff
}


pub fn drought_factor(
    smd: f32,  // Soil Moisture Deficit
    rainfall_effect: f32,  // rainfall effect
) -> f32 {
    let df = 10.5*(1-f32::exp(-(smd+30.0)/40.0))*((41.0*f32::powf(rainfall_effect, 2) + x_eff) / (40.0*f32::powf(rainfall_effect, 2) + rainfall_effect + 1.0));
    // normalize taking the minimum value between df and 10
    f32::min(df, 10.0)
}


// Forest Fire Danger Index - FFDI
pub fn ffdi(
    temp_max: f32,  // °C ????
    rh_min: f32,  // %
    wind_speed_max: f32,  // m/h
    drought_factor: f32,
) -> f32 {
    // conversion of wind speed from m/h to km/h
    let wind_speed = wind_speed_max * 3.6;
    // calculation of the FFDI
    let ffdi = 2.0*f32::exp(-0.45+0.987*f32::log(drought_factor)-0.0345*rh_min+0.00338*temp_max+0.0234*wind_speed);
    ffdi
}


pub fn store_day(
    state: &mut Mark5StateElement,
    input: &InputElement
) {
    state.cum_rain += input.rain;
    if input.temp > state.max_temp {
        state.max_temp = input.temp;
    }
    if input.rh < state.min_rh {
        state.min_rh = input.rh;
    }
    if input.wind > state.max_wind {
        state.max_wind = input.wind;
    }
}


#[allow(non_snake_case)]
pub fn update_state_fn(
    state: &mut Mark5StateElement,
    props: &Mark5PropertiesElement,
    time: &DateTime<Utc>,
) {
    // runoff check for KBDI is true if the last element of daily_rainfall is 0 mm
    let runoff_check = state.daily_rain.last().unwrap() == &0.0;

    // store the datetime and cumulated rain
    state.update(*time, state.cum_rain);

    // calculate the KBDI
    let new_kbdi = kbdi_update(
        state.kbdi,
        state.cum_rain,
        state.max_temp,
        props.mean_rain,
        runoff_check,
    );
    // store the new KBDI
    state.kbdi = new_kbdi;
}


pub fn rain_events(time: DateTime<Utc>, dates: Vec<DateTime<Utc>>, daily_rain: Vec<f32>) -> Vec<(f32, i64)> {
    let mut rain_events = vec![];
    // a rainy day happens when the daily rain is greater than 2 mm
    let rainy_days = daily_rain.iter().enumerate().filter(|(_, r)| **r > 2.0);
    // a rain event is defines as a set of consecutibe rainy days, and characterized by the total rain and
    // the number of days of distance between the day with maximum rain cumulation and current time
    let mut idx = 0;
    while idx < rainy_days.len() {
        let mut j = idx;
        let mut rain_cum = 0.0;
        let mut max_rain = 0.0;
        let mut day_max_rain: DateTime<Utc> = time;
        while j < rainy_days.len() && rainy_days[j] {
            rain_cum += daily_rain[j];
            if daily_rain[j] > max_rain {
                max_rain = daily_rain[j];
                day_max_rain = dates[j];
            }
            j += 1;
        }
        idx = j;
        let n_days = (time - day_max_rain).num_days();
        rain_events.push((rain_cum, days));
    }
    rain_events
}


#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &Mark5StateElement,
    config: &Mark5ModelConfig,
    time: &DateTime<Utc>,
) -> OutputElement {
    
    let (dates, sum_rains) = state.get_time_window(time);
    
    // calculate the rainfall effect
    let rainfall_events = rain_events(*time, dates, sum_rains);
    
    let rainfall_effects: Vec<f32> = rainfall_events
        .iter()
        .map(|(rain, age)| rainfall_effect(new_kbdi, *rain, *age))
        .collect();
    let min_rainfall_effect = rainfall_effects.iter().min();

    // calculate the drought factor
    let df = drought_factor(&state.kbdi, min_rainfall_effect);
    // calculate the FFDI
    let new_ffdi = ffdi(state.max_temp, state.min_rh, state.max_wind, df);
}
