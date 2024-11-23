use crate::models::output::OutputElement;
use super::models::FosbergStateElement;


// Equilibrium Moisture Content
pub fn emc(
    temperature: f32,  // air temperature [°C]
    humidity: f32,  // relative humidity [%]
) -> f32 {
    // conversion from °C to °F
    let temp = temperature * 9.0 / 5.0 + 32.0;
    let emc = if humidity < 10.0 {
        0.03229 + 0.281073 * humidity - 0.000578 * temp * humidity
    } else if humidity < 50.0 {
        2.22749 + 0.160107 * humidity - 0.014784 * temp
    } else {
        21.0606 + 0.005565 * humidity.powi(2) - 0.00035 * temp * humidity - 0.483199 * humidity
    };
    emc
}


pub fn ffwi(
    temperature: f32,  // air temperature [°C]
    humidity: f32, // relative humidity [%]
    wind_speed: f32,  // wind speed [m/h]
) -> f32 {
    let emc = emc(temperature, humidity);
    let ws_mph = wind_speed / 1609.344;  // convert from m/h to mph
    let emc_eff = 1.0 - 2.0*(emc/30.0) + 1.5*(emc/30.0).powi(2) - 0.5*(emc/30.0).powi(3);
    let ffwi = emc_eff * f32::sqrt(1.0 + ws_mph.powi(2))/0.3002;
    ffwi
}

pub fn get_output_fn(
    state: &FosbergStateElement,
) -> OutputElement {
    let ffwi = ffwi(state.temp, state.humidity, state.wind_speed);
    // return the output element
    OutputElement {
        ffwi,
        temperature: state.temp,
        humidity: state.humidity,
        wind_speed: state.wind_speed,
        ..OutputElement::default()
    }
}
