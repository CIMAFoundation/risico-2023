use std::error::Error;

use chrono::{DateTime, Utc};
use ndarray::Array1;
use risico::models::input::InputVariableName;

/// Trait defining the behavior of an InputHandler for models
pub trait InputHandler {
    fn set_coordinates(&mut self, lats: &[f32], lons: &[f32]) -> Result<(), Box<dyn Error>>;

    /// Precompute and retain the source-grid indexes for one spatial tile.
    fn register_coordinates(&mut self, lats: &[f32], lons: &[f32])
        -> Result<usize, Box<dyn Error>>;

    /// Select a previously registered spatial tile without recomputing indexes.
    fn select_coordinates(&mut self, selection: usize) -> Result<(), Box<dyn Error>>;

    /// Discard tile mappings registered for a previous model run.
    fn clear_registered_coordinates(&mut self);

    /// Release decoded source fields after every tile has consumed a timestamp.
    fn clear_cached_values(&mut self);

    /// get the desired variable at the desired date
    fn get_values(&self, var: InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;

    /// Return the list of input files and associated variables
    fn info_input(&self) -> String;
}
