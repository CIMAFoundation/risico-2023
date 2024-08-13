use chrono::{DateTime, Utc};
use ndarray::Array1;

pub trait InputHandler {
    /// get the desired variable at the desired date
    fn get_values(&self, var: &str, date: &DateTime<Utc>) -> Option<Array1<f32>>;

    /// Returns the timeline of the input data
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;

    // returns the variables available at a given time
    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<String>;
}
