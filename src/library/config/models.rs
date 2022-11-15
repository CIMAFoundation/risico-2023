use std::collections::HashMap;

use crate::library::state::models;

use super::data::{read_cells_properties, read_vegetation};

pub struct Config {
    pub cells: Vec<models::Properties>,
    pub vegetations: HashMap<String, models::Vegetation>,
}

impl Config {
    pub fn new(cells_file: &str, veg_file: &str) -> Config {
        let cells = match read_cells_properties(cells_file){
            Ok(cells) => cells,
            Err(error) => panic!("Error reading cells file {cells_file}: \n{error}")
        };
        
        let vegetations = match read_vegetation(veg_file) {
            Ok(vegetations) => vegetations,
            Err(error) => panic!("Error reading vegetation file {veg_file}: \n {error}")
        };
        
        Config {
            cells: cells,
            vegetations: vegetations
        }
    }

    pub fn init_state(&self) -> Vec<models::Cell> {
        let mut cells: Vec<models::Cell> = Vec::new();
        for cell in self.cells.iter() {
            let vegetation = match self.vegetations.get(&cell.vegetation){
                Some(vegetation) => vegetation,
                None => panic!("Vegetation not found: {}", cell.vegetation)
            };
            let cell = models::Cell::new(cell, vegetation);
            cells.push(cell);
        }
        cells
    }
}