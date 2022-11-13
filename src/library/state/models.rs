use chrono::prelude::*;
use super::functions::update_state;
use super::functions::get_ffm;


#[derive(Debug)]
pub struct CellProperties {
    pub lon: f64,
    pub lat: f64,
    pub height: f64,
    pub width: f64,
    pub altitude: f64,
    
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
}   
