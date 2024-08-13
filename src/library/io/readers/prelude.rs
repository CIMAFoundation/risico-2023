use chrono::{DateTime, Utc};
use ndarray::Array1;

use crate::library::helpers::InputVariableName;

/// Trait defining the behavior of an InputDataSupplier for RISICO
pub trait InputHandler {
    /// get the desired variable at the desired date
    fn get_values(&self, var: &InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;

    // returns the variables available at a given time
    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<String>;
}
