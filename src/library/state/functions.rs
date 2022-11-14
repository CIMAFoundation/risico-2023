//! functions to work on the state of the risico model
//! 
// import duration from chron
use super::models::CellProperties;



pub fn get_ffm(_cell: &CellProperties, ffm: f64) -> f64 {
    ffm + 1.0
}



