
use chrono::{DateTime, Utc, Timelike};
use chrono_tz::Tz;
use tzf_rs::DefaultFinder;
use lazy_static::lazy_static;
use crate::models::{input::InputElement, output::OutputElement};
use super::{
    constants::*,
    models::{AngstromPropertiesElement, AngstromStateElement},
};

lazy_static! {
    static ref TZ_FINDER: DefaultFinder = DefaultFinder::new();
}

// Store the daily info -> values at 13:00 local time
pub fn store_day_fn(
    state: &mut AngstromStateElement,
    input: &InputElement,
    prop: &AngstromPropertiesElement,
    time: &DateTime<Utc>,
) {
    // check the timezone
    let tz_name = TZ_FINDER.get_tz_name(prop.lon as f64, prop.lat as f64);
    let tz : Tz = tz_name.parse().expect("Invalid timezone name");
    let local_time = time.with_timezone(&tz);
    // store the input
    if local_time.hour() == TIME_WEATHER {
        state.temp_13 = input.temperature;
        state.humidity_13 = input.humidity;
    }
}

// Compute the Angstrom index
pub fn angstrom_index(
    humidity_13: f32,  // humidity [%] at 13:00 local time
    temp_13: f32,  // temperature [°C] at 13:00 local time
) -> f32 {
    let angstrom = (humidity_13 / 20.0) + ((27.0 - temp_13) / 10.0);
    angstrom
}

pub fn get_output_fn(
    state: &AngstromStateElement,
) -> OutputElement {
    let angstrom = angstrom_index(state.humidity_13, state.temp_13);
    // return the output element
    OutputElement {
        angstrom,
        temperature: state.temp_13,
        humidity: state.humidity_13,
        ..OutputElement::default()
    }
}