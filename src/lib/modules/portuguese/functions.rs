use chrono::{DateTime, Utc, Timelike};
use chrono_tz::Tz;
use tzf_rs::DefaultFinder;
use lazy_static::lazy_static;

use crate::models::{input::InputElement, output::OutputElement};
use super::{
    constants::*,
    models::{PortuguesePropertiesElement, PortugueseStateElement},
};


lazy_static! {
    static ref TZ_FINDER: DefaultFinder = DefaultFinder::new();
}

// Store the daily info at 12:00 local time
pub fn store_day_fn(
    state: &mut PortugueseStateElement,
    input: &InputElement,
    prop: &PortuguesePropertiesElement,
    time: &DateTime<Utc>,
) {
    // cumulate rain
    if input.rain > 0.0 {
        state.cum_rain += input.rain;
    }
    // store the other daily info at 12:00 local time
    let tz_name = TZ_FINDER.get_tz_name(prop.lon as f64, prop.lat as f64);
    let tz : Tz = tz_name.parse().expect("Invalid timezone name");
    let local_time = time.with_timezone(&tz);
    if local_time.hour() == TIME_WEATHER {
        state.temp_12 = input.temperature;
        state.temp_dew_12 = input.temp_dew;
    }
}


// Ignition Index
pub fn ignition_index(
    temp_12: f32,  // temperature [°C] at 12:00
    temp_dew_12: f32,  // dew temperature [°C] at 12:00
) -> f32 {
    let ign = temp_12 * (temp_12 - temp_dew_12);
    ign
}


pub fn update_fn(
    state: &mut PortugueseStateElement,
) {
    // ignition index
    let ign = state.temp_12 * (state.temp_12 - state.temp_dew_12);
    state.ign = ign;
    state.sum_ign += ign;  // add to the sum for the warm state
    // fire index
    state.fire_index = state.ign + state.cum_index;
    // compute the rain coefficient
    let rain_coeff = if state.cum_rain <= 1.0 {
        1.0
    } else if state.cum_rain <= 2.0{
        0.8
    } else if state.cum_rain <= 3.0 {
        0.6
    } else if state.cum_rain <= 4.0 {
        0.4
    } else if state.cum_rain <= 10.0 {
        0.2
    } else {
        0.1
    };
    // update the cumulative index
    state.cum_index = rain_coeff*state.sum_ign;
}


// Output function
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &PortugueseStateElement,
) -> OutputElement {
    OutputElement {
        portuguese_ignition: state.ign,
        portuguese_fdi: state.fire_index,
        temperature: state.temp_12,
        temp_dew_point: state.temp_dew_12,
        rain: state.cum_rain,
        ..OutputElement::default()
    }
}
