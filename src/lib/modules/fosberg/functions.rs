use crate::models::output::OutputElement;
use super::{constants::NODATAVAL, models::FosbergStateElement};


// Equilibrium Moisture Content [%] -> Simard formulation
// From https://wikifire.wsl.ch/tiki-indexf2f0.html?page=Equilibrium+moisture+content
pub fn emc(
    temperature: f32,  // temperature [°C]
    humidity: f32,  // relative humidity [%]
) -> f32 {
    // conversion from °C to °F -> needed for the formula
    let temp_f = temperature * 9.0 / 5.0 + 32.0;
    let emc = if humidity < 10.0 {
        0.03229 + 0.281073 * humidity - 0.000578 * temp_f * humidity
    } else if humidity < 50.0 {
        2.22749 + 0.160107 * humidity - 0.01478 * temp_f
    } else {
        21.0606 + 0.005565 * humidity.powi(2) - 0.00035 * temp_f * humidity - 0.483199 * humidity
    };
    emc
}

// Fosberg Fire Weather Index
// values range in [0 (no fire danger), 100 (high fire danger)]
// some info: https://www.spc.noaa.gov/exper/firecomp/INFO/fosbinfo.html
pub fn ffwi(
    temperature: f32,  // temperature [°C]
    humidity: f32, // relative humidity [%]
    wind_speed: f32,  // wind speed [m/h]
) -> f32 {
    let emc = emc(temperature, humidity);
    let ws_mph = wind_speed / 1609.344;  // convert from m/h to mph
    let emc_eff = 1.0 - 2.0*(emc/30.0) + 1.5*(emc/30.0).powi(2) - 0.5*(emc/30.0).powi(3);
    let ffwi = emc_eff * f32::sqrt(1.0 + ws_mph.powi(2))/0.3002;
    // clip in [0, 100]
    if ffwi < 0.0 {
        0.0
    } else if ffwi > 100.0 {
        100.0
    } else {
        ffwi
    }
}

pub fn get_output_fn(
    state: &FosbergStateElement,
) -> OutputElement {
    if (state.temp == NODATAVAL) || (state.humidity == NODATAVAL) || (state.wind_speed == NODATAVAL) {
        return OutputElement::default()
    }
    let ffwi = ffwi(state.temp, state.humidity, state.wind_speed);
    let ws_out = state.wind_speed / 3600.0;  // convert from m/h to m/s
    OutputElement {
        ffwi,  // [-]
        temperature: state.temp,  // [°C]
        humidity: state.humidity,  // [%]
        wind_speed: ws_out,  // [m/s]
        ..OutputElement::default()
    }
}
