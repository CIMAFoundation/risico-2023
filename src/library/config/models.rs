use std::{collections::HashMap, fs::File, u8::{self, MAX}, io::{self, BufRead}, path::Path, clone};

use chrono::{DateTime, Utc};
use chrono::*;

use crate::library::{state::models, io::{models::{InputData, Grid}, readers::read_input_from_file}};

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

type VariableMap = HashMap<String, InputData>;

#[derive(Debug)]
pub struct GridRegistry {
   pub grids: HashMap<String, Grid>,
}
impl GridRegistry {
    pub fn new() -> GridRegistry {
        GridRegistry {
            grids: HashMap::new(),
        }
    }

    pub fn has(&self, grid_name: &str) -> bool {
        self.grids.contains_key(grid_name)
    }

    pub fn add(&mut self, name: String, grid: Grid) {
        if self.grids.contains_key(&name) { return; }
        self.grids.insert(name, grid);
    }

    pub fn get_grid(&self, name: &str) -> Option<&Grid> {
        self.grids.get(name)
    }
    
}

#[derive(Debug)]
pub struct InputDataHandler {
    pub grid_registry: GridRegistry,
    pub data_map: HashMap<DateTime<Utc>, VariableMap>,
}

impl InputDataHandler{
    pub fn new(file_path: &str) -> InputDataHandler{
        let mut grid_registry = GridRegistry::new();
        let mut data_map: HashMap<DateTime<Utc>, VariableMap> = HashMap::new();

        let file = File::open(file_path).expect(&format!("Can't open input file {}", file_path));


        // file is a text file in which each line is a file with the following structure:
        // directory/<YYYYmmDDHHMM>_<grid_name>_<variable>.<extension>
        // read the file and parse the lines
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line.unwrap();
            
            if !line.ends_with(".zbin") { continue; }

            // extract only the file name using path manipulation
            let file_name = Path::new(&line).file_name().unwrap().to_str().unwrap();

            // split the file name into its components
            let components: Vec<&str> = file_name.split('_').collect();
            let date = components[0];
            let grid_name = components[1];
            let variable = components[2].split('.').collect::<Vec<&str>>()[0];

            // parse the date
            println!("Parsing date: {}", date);
            let date = match NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")  {
                Ok(date) => DateTime::<Utc>::from_utc(date, Utc),
                Err(error) => panic!("Error parsing date: {error}")
            };
            
            let date = date.with_timezone(&Utc);

            // read the file
            
            let (_grid, values) = read_input_from_file(&line).unwrap();
            
            // check if the grid is already in the registry
            grid_registry.add(grid_name.to_string(), _grid);
            
            let data = InputData::new(values, grid_name.to_string());

            // add the data to the data map
            if !data_map.contains_key(&date) {
                data_map.insert(date, HashMap::new());
            }
            let data_map = data_map.get_mut(&date).unwrap();
            data_map.insert(variable.to_string(), data);

        }

        InputDataHandler {
            grid_registry: grid_registry,
            data_map: data_map
        }
        
    }
}


