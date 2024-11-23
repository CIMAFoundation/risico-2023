use crate::models::output::OutputElement;
use super::models::SharplesStateElement;

// fuel moisture index
pub fn index_fmi(
    temperature: f32,  // air temperature [°C]
    humidity: f32, // relative humidity [%]
) -> f32 {
    let fmi = 10.0 - 0.25*(temperature - humidity);
    fmi
}

pub fn index_f(
    fmi: f32,  // fuel moisture index [-]
    wind_speed: f32,  // wind speed [m/h]
) -> f32 {
    let ws = wind_speed / 1000.0;  // conversion to km/h
    let f = f32::max(1.0, ws) / fmi;
    f
}

pub fn get_output_fn(
    state: &SharplesStateElement,
) -> OutputElement {
    let fmi = index_fmi(state.temp, state.humidity);
    let f = index_f(fmi, state.wind_speed);
    // convert the wind speed in m/s
    let ws_out = state.wind_speed / 3600.0;
    // return the output element
    OutputElement {
        fmi,
        f,
        temperature: state.temp,
        humidity: state.humidity,
        wind_speed: ws_out,
        ..OutputElement::default()
    }
}
