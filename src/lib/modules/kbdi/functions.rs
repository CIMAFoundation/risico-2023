
use crate::models::{input::InputElement, output::OutputElement};
use chrono::{DateTime, Utc};
use super::{
    config::KBDIModelConfig,
    constants::*,
    models::{KBDIPropertiesElement, KBDIStateElement},
};



pub fn store_day_fn(
    state: &mut KBDIStateElement,
    input: &InputElement,
) {
    // cumulated rain per day
    state.cum_rain += input.rain;
    // store the maximum temperature per day
    if (state.temperature == NODATAVAL) || (input.temperature > state.temperature) {
        state.temperature = input.temperature;
    }
}


// Keetch-Byram Drought Index
// It is expressed as soil moisture deficit in mm, and ranges from 0 mm (wet) to 200 mm (dry)
// Source: WikiFire, Finkele et al. 2006
pub fn kbdi_update_mm(
    kbdi: f32,  // previous KBDI value [mm]
    temp: f32,  // temperature [°C]
    history_rain: &Vec<f32>,  // daily rain of the last days + rain of today [mm] (ordered from oldest to newest)
    mean_annual_rain: f32,  // mean annual rain [mm]
) -> f32 {
    // rain of the day
    let day_rain = history_rain[history_rain.len() - 1];
    // calculate the rain of the last days (in case they are consecutive)
    let mut last_rain = 0.0;
    if history_rain.len() >= 2 {
        let mut idx = history_rain.len() - 2;  // starting from yesterday
        while (idx > 0) && (history_rain[idx] > 0.0) {
            last_rain += history_rain[idx];
            idx -= 1;
        }
    }
    // effective rain of the day
    let effective_rain = f32::max(0.0, day_rain - f32::max(0.0, KBDI_RAIN_RUNOFF - last_rain));    
    let dt: f32 = 1.0;  // DAILY COMPUTATION
    let evapo_transp: f32 =  (((203.2-kbdi) * (0.968*f32::exp(0.0875*temp+1.5552)-8.3) * dt) / (1.0+10.88*f32::exp(-0.001736*mean_annual_rain)))*10e-3;
    kbdi - effective_rain + evapo_transp
}


pub fn get_output_fn(
    state: &mut KBDIStateElement,
    prop: &KBDIPropertiesElement,
    config: &KBDIModelConfig,
    time: &DateTime<Utc>,
) -> OutputElement {
    // store the datetime and cumulated rain for the day of the run
    state.update(time, state.cum_rain);
    // get the last rains in the time windows -> they are already ordered from oldest to newest
    let (_, daily_rains) = state.get_time_window(time);
    let new_kbdi = config.update_kbdi(state.kbdi, state.temperature, &daily_rains, prop.mean_rain);
    // store the new KBDI value
    state.kbdi = new_kbdi;
    // return the output element
    OutputElement {
        kbdi: new_kbdi,
        rain: state.cum_rain,
        temperature: state.temperature,
        ..OutputElement::default()
    }
}