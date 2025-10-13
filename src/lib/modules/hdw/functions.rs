use super::{constants::NODATAVAL, models::HdwStateElement};
use crate::models::output::OutputElement;

// Hot-Dry-Wind index
pub fn hdw(
    vpd: f32,        // vpor pressure deficit [hPa]
    wind_speed: f32, // wind speed [m/h]
) -> f32 {
    let ws = wind_speed / 3600.0; // wind speed [m/s] -> required by the formula

    ws * vpd
}

pub fn get_output_fn(state: &HdwStateElement) -> OutputElement {
    if (state.vpd == NODATAVAL) || (state.wind_speed == NODATAVAL) {
        return OutputElement::default();
    }
    let hdw = hdw(state.vpd, state.wind_speed);
    // return the output element
    let ws_out = state.wind_speed / 3600.0;
    OutputElement {
        hdw,                // [-]
        vpd: state.vpd,     // [hPa]
        wind_speed: ws_out, // [m/s]
        ..OutputElement::default()
    }
}
