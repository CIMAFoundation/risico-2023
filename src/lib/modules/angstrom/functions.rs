use super::{constants::*, models::AngstromStateElement};
use crate::models::output::OutputElement;

// Compute the Angstrom index
pub fn angstrom_index(
    temp: f32,     // temperature [°C]
    humidity: f32, // humidity [%]
) -> f32 {
    (humidity / 20.0) + ((27.0 - temp) / 10.0)
}

pub fn get_output_fn(state: &AngstromStateElement) -> OutputElement {
    if (state.temp == NODATAVAL) || (state.humidity == NODATAVAL) {
        return OutputElement::default();
    }
    // compute the angstrom index
    let angstrom = angstrom_index(state.temp, state.humidity);
    // return the output element
    OutputElement {
        angstrom,                 // [-]
        temperature: state.temp,  // [°C]
        humidity: state.humidity, // [%]
        ..OutputElement::default()
    }
}
