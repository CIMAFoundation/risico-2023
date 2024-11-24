use crate::models::output::OutputElement;
use super::models::HdwStateElement;


// Hot-Dry-Wind index
pub fn hdw(
    vpd: f32,  // vpor pressure deficit [hPa]
    wind_speed: f32,  // wind speed [m/h]
) -> f32 {
    let ws = wind_speed / 3600.0; // wind speed [m/s]
    let hdw = ws*vpd;
    hdw
}


pub fn get_output_fn(
    state: &HdwStateElement,
) -> OutputElement {
    let hdw = hdw(state.vpd, state.wind_speed);
    // return the output element
    let ws = state.wind_speed / 3600.0;
    OutputElement {
        hdw,
        vpd: state.vpd,
        wind_speed: ws,
        ..OutputElement::default()
    }
}
