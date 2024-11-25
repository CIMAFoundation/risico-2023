use chrono::{DateTime, Utc, Timelike};
use chrono_tz::Tz;
use tzf_rs::DefaultFinder;
use lazy_static::lazy_static;

use crate::models::{input::InputElement, output::OutputElement};
use super::{
    constants::*,
    models::{NesterovPropertiesElement, NesterovStateElement},
};


lazy_static! {
    static ref TZ_FINDER: DefaultFinder = DefaultFinder::new();
}

// Store the daily info
pub fn store_day_fn(
    state: &mut NesterovStateElement,
    input: &InputElement,
    prop: &NesterovPropertiesElement,
    time: &DateTime<Utc>,
) {
    // cumulate rain
    if input.rain > 0.0 {
        state.cum_rain += input.rain;
    }
    // store the other daily info -> values at 3pm local time
    let tz_name = TZ_FINDER.get_tz_name(prop.lon as f64, prop.lat as f64);
    let tz : Tz = tz_name.parse().expect("Invalid timezone name");
    let local_time = time.with_timezone(&tz);
    // Store the daily info at 15 local time
    if local_time.hour() == TIME_WEATHER {
        state.temp_15 = input.temperature;
        state.temp_dew_15 = input.temp_dew;
    }
}


// Nesterov Ignition Index
pub fn nesterov_update(
    nesterov: f32,  // Nesterov Index [-]
    temp: f32,  // temperature at 3pm [°C]
    temp_dew: f32,  // dew temperature at 3pm [°C]
    daily_rain: f32,  // daily rain [mm]
) -> f32 {
    let new_nesterov: f32 = if daily_rain > RAIN_TH {
        0.0
    } else {
        let nest = nesterov + temp*(temp - temp_dew);
        if nest < 0.0 {
            0.0
        } else {
            nest
        }
    };
    new_nesterov
}


pub fn update_fn(
    state: &mut NesterovStateElement,
) {
    // update the Nesterov index
    state.nesterov = nesterov_update(state.nesterov, state.temp_15, state.temp_dew_15, state.cum_rain);
}

// Output function
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &NesterovStateElement,
) -> OutputElement {
    OutputElement {
        nesterov: state.nesterov,
        temperature: state.temp_15,
        temp_dew_point: state.temp_dew_15,
        rain: state.cum_rain,
        ..OutputElement::default()
    }
}
