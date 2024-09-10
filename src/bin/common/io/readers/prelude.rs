use std::error::Error;

use chrono::{DateTime, Utc};
use ndarray::Array1;
use serde_derive::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

/// Trait defining the behavior of an InputHandler for models
pub trait InputHandler {
    fn set_coordinates(&mut self, lats: &[f32], lons: &[f32]) -> Result<(), Box<dyn Error>>;

    /// get the desired variable at the desired date
    fn get_values(&self, var: InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;
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
}
