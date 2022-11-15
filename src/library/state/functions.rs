//! functions to work on the state of the risico model
//! 
// import duration from chron
use super::models::Properties;



pub fn get_ffm(_cell: &Properties, ffm: f64) -> f64 {
    ffm + 1.0
}



