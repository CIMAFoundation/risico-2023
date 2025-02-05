use chrono::{DateTime, Utc};
use ndarray::Array1;

use serde_derive::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

use crate::constants::NODATAVAL;

/// InputElement represents a single input element for the model
/// If the input unit provided is not the one expected by the model, the model will convert it (helpers.rs)
#[derive(Debug)]
pub struct InputElement {
    /// air temperature [°C]
    pub temperature: f32,
    /// rain [mm]
    pub rain: f32,
    /// wind speed [m/h]
    pub wind_speed: f32,
    /// wind direction [rad]
    pub wind_dir: f32,
    /// relative humidity [%]
    pub humidity: f32,
    /// snow cover [mm]
    pub snow_cover: f32,
    /// dew point temperature [°C]
    pub temp_dew: f32,
    // vapor pressure deficit [hPa]
    pub vpd: f32,

    // satellite variables
    pub ndvi: f32,
    pub ndwi: f32,
    pub msi: f32,
    pub swi: f32,
}

impl Default for InputElement {
    fn default() -> Self {
        Self {
            temperature: NODATAVAL,
            rain: NODATAVAL,
            wind_speed: NODATAVAL,
            wind_dir: NODATAVAL,
            humidity: NODATAVAL,
            snow_cover: NODATAVAL,
            temp_dew: NODATAVAL,
            vpd: NODATAVAL,
            ndvi: NODATAVAL,
            ndwi: NODATAVAL,
            msi: NODATAVAL,
            swi: NODATAVAL,
        }
    }
}

pub struct Input {
    pub time: DateTime<Utc>,
    pub data: Array1<InputElement>,
}

#[allow(clippy::upper_case_acronyms, non_camel_case_types)]
#[derive(
    Debug, PartialEq, Eq, Hash, Copy, Clone, EnumString, EnumIter, Display, Serialize, Deserialize,
)]
pub enum InputVariableName {
    /// WEATHER VARIABLES
    /// Observed relative humidity
    F,
    /// Relative humidity
    H,
    /// Observed temperature
    K,
    /// Forecasted temperature
    T,
    /// Forecasted dew point temperature
    R,
    /// Forecasted specific humidity
    Q,
    /// Forecasted pressure at surface level
    PSFC,
    /// Wind Speed
    W,
    /// Wind Direction
    D,
    /// U component of the wind
    U,
    /// V value of the wind
    V,
    /// Observed precipitation
    O,
    /// Forecasted precipitation
    P,
    /// Forecasted snow cover
    SNOW,

    /// SATELLITE VARIABLES
    /// NDWI value
    NDWI,
    /// NDVI value
    NDVI,
    /// MSI value
    M,
    /// SWI value
    SWI,

}
