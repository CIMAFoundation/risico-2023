
use chrono::{DateTime, Utc};
use crate::models::{input::InputElement, output::OutputElement};
use crate::modules::functions::*;
use super::{
    constants::*,
    models::{OrieuxPropertiesElement, OrieuxStateElement},
};


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
}

pub fn update_fn(
    state: &mut OrieuxStateElement,
    prop: &OrieuxPropertiesElement,
    time: &DateTime<Utc>,
) {
    // compute potential evapotranspiration
    // temperature corrected from Pereira & Pruitt (2004)
    // see: https://wikifire.wsl.ch/tiki-index3aa5.html?page=Potential+evapotranspiration&structure=Fire
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
    let orieux_new = f32::min(R_MAX, state.orieux + state.cum_rain - (state.orieux/R_MAX)*state.pet);
    state.orieux = orieux_new;
}

pub fn get_output_fn(
    state: &OrieuxStateElement,
) -> OutputElement {
    OutputElement {
        orieux: state.orieux,
        pet_t: state.pet,
        ..OutputElement::default()
    }
}