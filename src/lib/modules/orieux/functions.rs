
use chrono::{DateTime, Utc};
use crate::models::{input::InputElement, output::OutputElement};
use crate::modules::functions::*;
use super::{
    constants::*,
    models::{OrieuxPropertiesElement, OrieuxStateElement},
};

// Orieux fire class
// Source: Zacharakis, I.; Tsihrintzis, V.A. Environmental Forest Fire Danger Rating Systems and Indices around the Globe: A Review. Land 2023, 12, 194. https://doi.org/10.3390/land12010194
pub fn fire_class(
    orieux_wr: f32,  // oriex water reserve [mm]
    wind_speed: f32  // wind speed [m/s]
) -> f32 {
    let ws_kh = wind_speed * 3.6;  // wind speed [km/h]
    // Define the classes as a 2D array
    let classes = [
        [1, 2, 3], // Row 0: < 30 mm
        [1, 2, 3], // Row 1: 30 - 50 mm
        [1, 1, 2], // Row 2: 50 - 100 mm
        [0, 0, 0], // Row 3: 100 - 150 mm
    ];
    // Determine the row based on Orieux index
    let row = if orieux_wr < 30.0 {
        0
    } else if orieux_wr < 50.0 {
        1
    } else if orieux_wr < 100.0 {
        2
    } else if orieux_wr <= 150.0 {
        3
    } else {
        panic!("Orieux index out of range!"); // Invalid input
    };
    // Determine the column based on wind speed
    let col = if ws_kh < 20.0 {
        0
    } else if ws_kh <= 40.0 {
        1
    } else {
        2
    };
    // Return the class
    classes[row][col] as f32
}


pub fn store_day_fn(
    state: &mut OrieuxStateElement,
    input: &InputElement,
) {
    // cumulated rain per day
    if input.rain > 0.0 {
        state.cum_rain += input.rain;
    }
    // maximum temperature per day
    if (state.max_temp == NODATAVAL) || (input.temperature > state.max_temp) {
        state.max_temp = input.temperature;
    }
    // minimum temperature per day
    if (state.min_temp == NODATAVAL) || (input.temperature < state.min_temp) {
        state.min_temp = input.temperature;
    }
    // maximum wind speed per day
    if (state.max_wind_speed == NODATAVAL) || (input.wind_speed > state.max_wind_speed) {
        state.max_wind_speed = input.wind_speed;
    }
}

pub fn update_fn(
    state: &mut OrieuxStateElement,
    prop: &OrieuxPropertiesElement,
    time: &DateTime<Utc>,
) {
    // compute potential evapotranspiration - Thornthwaite equation
    // temperature corrected from Pereira & Pruitt (2004)
    // see: https://wikifire.wsl.ch/tiki-index3aa5.html?page=Potential+evapotranspiration&structure=Fire
    if (state.min_temp) != NODATAVAL && (state.max_temp != NODATAVAL) {
        let temp_eff = 0.5*0.72*(3.0*state.max_temp - state.min_temp);
        let hlight = daylight_hours(prop.lat, *time);  // daylight hours
        let temp_eff_corr = temp_eff*(hlight/(24.0-hlight));
        let pet = evapotranspiration_thornthwaite(
            temp_eff_corr,
            prop.lat,
            *time,
            prop.heat_index,
        );
        state.pet = pet;
        let mut orieux_wr_new = f32::min(R_MAX, state.orieux_wr + state.cum_rain - (state.orieux_wr/R_MAX)*state.pet);
        // clip at R_MAX
        if orieux_wr_new < 0.0 {
            orieux_wr_new = 0.0;
        } else if orieux_wr_new > R_MAX {
            orieux_wr_new = R_MAX;
        }
        state.orieux_wr = orieux_wr_new;
    }
    // compute the fire danger index
    if state.max_wind_speed != NODATAVAL {
        state.orieux_fd = fire_class(state.orieux_wr, state.max_wind_speed);
    } else {
        state.orieux_fd = NODATAVAL;
    }
}


pub fn get_output_fn(
    state: &OrieuxStateElement,
) -> OutputElement {
    OutputElement {
        orieux_wr: state.orieux_wr,  // [mm]
        pet_t: state.pet,  // [mm]
        orieux_fd: state.orieux_fd,  // [adim]
        ..OutputElement::default()
    }
}