use chrono::{DateTime, Utc};
use ndarray::Array1;
use strum_macros::EnumString;

/// Trait defining the behavior of an InputDataSupplier for RISICO
pub trait InputHandler {
    /// get the desired variable at the desired date
    fn get_values(&self, var: InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;

    // returns the variables available at a given time
    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<InputVariableName>;
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumString)]
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
