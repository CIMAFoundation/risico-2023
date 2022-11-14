use chrono::prelude::*;
use chrono::Duration;

use super::functions::get_ffm;

const UPDATE_TIME: i64 = 100;
#[derive(Debug)]
pub struct CellProperties {
    pub lon: f64,
    pub lat: f64,
    pub slope: f64,
    pub aspect: f64,
    pub vegetation: i16
}

#[derive(Debug)]
#[derive(Clone)]
pub struct CellState {
    pub ffm: f64,
}


#[derive(Debug)]
pub struct Cell<'a> {
    // The cell's properties
    pub properties: &'a CellProperties,
    // The cell's current state.
    pub state: CellState,
    // The cell's next state.
}

impl Cell<'_> {
    pub fn new(properties: &CellProperties) -> Cell {
        Cell {
            properties,
            state: CellState { ffm: 0.0 },
        }
    }
    pub fn update(&self) -> Cell {
        Cell {
            properties: self.properties,
            state: CellState { 
                ffm: get_ffm(self.properties, self.state.ffm)
            },
        }
    }
}

#[derive(Debug)]
pub struct State<'a> {
    // The grid's cells.
    pub cells: Vec<Cell<'a>>,
    pub time: DateTime<Utc>
}

impl State<'_> {
    pub fn new<'a>(cells: &'a Vec<CellProperties>) -> State<'a> {
        let cells = cells.iter()
            .map(|cell| Cell::new(cell))
            .collect();
        State { cells, time: Utc::now() }
    }

    pub fn update(&self) -> State {
    /*

     */
    // determine the new time for the state
    
        let new_time = self.time + Duration::seconds(UPDATE_TIME);

        // execute the update function on each cell
        let cells = self.cells.iter()
                    .map(|cell| cell.update())
                    .collect();
        // return the new state
        State { cells: cells, time: new_time }

    }
}   
