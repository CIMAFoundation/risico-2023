use std::error::Error;

use chrono::{DateTime, Utc};
use ndarray::Array1;
use risico::models::input::InputVariableName;

/// Trait defining the behavior of an InputHandler for models
pub trait InputHandler {
    fn set_coordinates(&mut self, lats: &[f32], lons: &[f32]) -> Result<(), Box<dyn Error>>;

    /// get the desired variable at the desired date
    fn get_values(&self, var: InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;
}
