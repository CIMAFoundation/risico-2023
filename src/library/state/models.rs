use chrono::prelude::*;
use chrono::Duration;

use super::functions::get_ffm;

const UPDATE_TIME: i64 = 100;
#[derive(Debug)]
pub struct Properties {
    pub lon: f64,
    pub lat: f64,
    pub slope: f64,
    pub aspect: f64,
    pub vegetation: String
}

#[derive(Debug)]
pub struct Vegetation {
    pub id: String,	
    pub d0: f64,
    pub d1: f64,
    pub hhv: f64,	
    pub umid: f64,
    pub v0: f64,
    pub T0: f64,
    pub	sat: f64,
    pub name: String
}

#[derive(Debug)]
#[derive(Clone)]
pub struct CellState {
    pub ffm: f64,
}


#[derive(Debug)]
pub struct Cell<'a> {
    // The cell's properties
    pub properties: &'a Properties,
    // The cell's current state.
    pub vegetation: &'a Vegetation,
    pub state: CellState,
    // The cell's next state.
}

impl Cell<'_> {
    pub fn new<'a>(properties: &'a Properties, vegetation: &'a Vegetation) -> Cell<'a> {
        Cell {
            properties,
            vegetation,
            state: CellState { ffm: 0.0 },
        }
    }
    pub fn update(&self) -> Cell {
        Cell {
            properties: self.properties,
            vegetation: self.vegetation,
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
    /// Create a new state.
    pub fn new(cells: Vec<Cell>, time: DateTime<Utc>) -> State {
        State { cells, time }
    }
    

    /// Update the state of the cells.
    pub fn update<'a>(&'a self) -> State<'a> {
    // determine the new time for the state
    
        let new_time = self.time + Duration::seconds(UPDATE_TIME);

        // execute the update function on each cell
        let cells = self.cells.iter()
                    .map(|cell| cell.update())
                    .collect::<Vec<Cell>>();
        //return the new state
        //State { cells: cells, time: new_time }

        State::new(cells, new_time)
    }
}   
