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

// Store the daily info at 15:00 local time
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
    // store the other daily info
    let tz_name = TZ_FINDER.get_tz_name(prop.lon as f64, prop.lat as f64);
    let tz : Tz = tz_name.parse().expect("Invalid timezone name");
    let local_time = time.with_timezone(&tz);
    if local_time.hour() == TIME_WEATHER {
        state.temp_15 = input.temperature;
        state.temp_dew_15 = input.temp_dew;
    }
}


// Nesterov Ignition Index
pub fn nesterov_update(
    nesterov: f32,  // Nesterov Index [-]
    temp_15: f32,  // temperature [°C] at 15:00
    temp_dew_15: f32,  // dew point temperature [°C] at 15:00
    daily_rain: f32,  // daily rain [mm]
) -> f32 {
    let new_nesterov: f32 = if daily_rain > RAIN_TH {
        0.0
    } else {
        let nest = nesterov + temp_15*(temp_15 - temp_dew_15);
        if nest < 0.0 {
            0.0
        } else {
            nest
        }
    };
    new_nesterov
}


// update the Nesterov index
pub fn update_fn(
    state: &mut NesterovStateElement,
) {
    state.nesterov = nesterov_update(state.nesterov, state.temp_15, state.temp_dew_15, state.cum_rain);
}

// Output function
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &NesterovStateElement,
) -> OutputElement {
    OutputElement {
        nesterov: state.nesterov,  // [-]
        temperature: state.temp_15,  // [°C] 
        temp_dew_point: state.temp_dew_15,  // [°C]
        rain: state.cum_rain,  // [mm]
        ..OutputElement::default()
    }
}
