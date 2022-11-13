// import state from lib
mod library;

use library::state::models::{State, Cell};
use chrono::prelude::*;

use crate::library::state::{functions::update_state, models::{CellProperties, CellState}};

fn main() {
    
    let time: DateTime<Utc> = Utc::now() ;
    let cells: Vec<Cell> = vec![
        Cell {
            properties: &CellProperties {
                lon: 0.0,
                lat: 0.0,
                height: 0.0,
                width: 0.0,
                altitude: 0.0,
                slope: 0.0,
                aspect: 0.0,
                vegetation: 0
            },
            state: CellState { ffm: 0.0 },
        }
    
    ];
    let state: State = State { cells, time };
    println!("{:?}", state);
    let state = update_state(&state);
    println!("{:?}", state);

}
