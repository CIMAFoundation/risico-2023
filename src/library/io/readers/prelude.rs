use chrono::{DateTime, Utc};
use ndarray::Array1;

pub trait InputHandler {
    fn get_values(&self, var: &str, date: &DateTime<Utc>) -> Option<Array1<f32>>;
    fn get_timeline(&self) -> Vec<DateTime<Utc>>;
    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<String>;
}
