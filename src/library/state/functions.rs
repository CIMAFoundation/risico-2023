//! functions to work on the state of the risico model
//! 
// import duration from chron
use chrono::Duration;

use crate::library::state::models::{State, Cell};

use super::models::CellProperties;

const UPDATE_TIME: i64 = 100;

pub fn get_ffm(cell: &CellProperties, ffm: f64) -> f64 {
    ffm + 1.0
}


pub fn update_state<'a>(state: &'a State<'a>) -> State<'a> {
    /*

     */
    // determine the new time for the state
    
    let new_time = state.time + Duration::seconds(UPDATE_TIME);

    // execute the update function on each cell
    let cells = state.cells.iter()
                .map(|cell| cell.update())
                .collect();
    // return the new state
    State { cells: cells, time: new_time }

}
