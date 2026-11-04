use std::error::Error;

use chrono::{DateTime, Utc};
use ndarray::Array1;
use risico::models::input::InputVariableName;

/// Trait defining the behavior of an InputHandler for models
///
/// Tile mappings are registered up front and then addressed by selection, so
/// reading stays an immutable operation and many tiles can read at once.
pub trait InputHandler: Sync {
    /// Precompute and retain the source-grid indexes for one spatial tile.
    fn register_coordinates(&mut self, lats: &[f32], lons: &[f32])
        -> Result<usize, Box<dyn Error>>;

    /// Discard tile mappings registered for a previous model run.
    fn clear_registered_coordinates(&mut self);

    /// Decode every source field this timestamp needs.
    ///
    /// Warming the cache before tiles read it keeps concurrent readers from
    /// racing to decode the same file.
    fn preload(&mut self, date: &DateTime<Utc>);

    /// Release decoded source fields after every tile has consumed a timestamp.
    fn clear_cached_values(&mut self);

    /// get the desired variable at the desired date for a registered tile
    fn get_values(
        &self,
        selection: usize,
        var: InputVariableName,
        date: &DateTime<Utc>,
    ) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;

    /// Return the list of input files and associated variables
    fn info_input(&self) -> String;
}
