use std::{collections::HashMap, fs::File, u8::{self, MAX}, io::{self, BufRead}, path::Path};

use chrono::{DateTime, Utc};

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

type VariableMap<'a> = HashMap<String, InputData<'a>>;

#[derive(Debug)]
pub struct InputDataHandler<'a> {
    pub grid_registry: HashMap<String, Grid>,
    pub data_map: HashMap<DateTime<Utc>, VariableMap<'a>>,
}

impl InputDataHandler<'_>{
    pub fn new(file_path: &str) -> InputDataHandler{
        let mut grid_registry: HashMap<String, Grid> = HashMap::new();
        let mut data_map: HashMap<DateTime<Utc>, VariableMap> = HashMap::new();

        let mut file = File::open(file_path).expect(&format!("Can't open input file {}", file_path));


        // file is a text file in which each line is a file with the following structure:
        // directory/<YYYYmmDDHHMM>_<grid_name>_<variable>.<extension>
        // read the file and parse the lines
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line.unwrap();
            // extract only the file name using path manipulation
            let file_name = Path::new(&line).file_name().unwrap().to_str().unwrap();

            // split the file name into its components
            let components: Vec<&str> = file_name.split('_').collect();
            let date = components[0];
            let grid_name = components[1];
            let variable = components[2].split('.').collect::<Vec<&str>>()[0];

            // parse the date
            let date = DateTime::parse_from_str(date, "%Y%m%d%H%M").unwrap(); 
            let date = date.with_timezone(&Utc);

            // read the file
            
            let (_grid, values) = read_input_from_file(&line).unwrap();
            
            
            let grid: &Grid;
            let mut data: InputData;
            if grid_registry.contains_key(grid_name) {
                grid = &grid_registry.get(grid_name).unwrap();
                data = InputData { values: values, grid: grid };
            }else {
                let grid = grid_registry.try_insert(grid_name.to_string(), _grid).unwrap();                
                data = InputData { values: values, grid: grid };
            }

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


