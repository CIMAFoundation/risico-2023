use chrono::{DateTime, Utc};
use ndarray::Array1;

use serde_derive::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

use crate::constants::NODATAVAL;

#[derive(Debug)]
pub struct InputElement {
    /// temperature in celsius
    pub temperature: f32,
    /// rain in mm
    pub rain: f32,
    /// wind speed in m/h
    pub wind_speed: f32,
    /// wind direction in radians
    pub wind_dir: f32,
    /// relative humidity in %
    pub humidity: f32,
    /// snow cover
    pub snow_cover: f32,
    /// dew point temperature
    pub temp_dew_point: f32,
    // vapor pressure deficit
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
            temp_dew_point: NODATAVAL,
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
    /// Air Humidity
    H,
    /// Observed Temperature
    K,
    /// Forecasted Temperature
    T,
    /// SNOW Cover
    SNOW,
    /// Observed Air Humidity
    F,
    // Forecasted dew point temperature
    R,
    /// Observed Precipitation
    O,
    /// Forecasted Precipitation
    P,
    /// Wind Speed
    W,
    /// Wind Direction
    D,
    /// NDWI Value
    NDWI,
    /// NDVI Value
    NDVI,
    /// MSI Value
    M,
    /// U component of the wind
    U,
    /// V value of the wind
    V,
    /// SWI Value
    SWI,
    /// Forecasted Specific Humidity at 2m
    Q2,
    /// Forecasted Pressure at surface level
    PSFC,
}
