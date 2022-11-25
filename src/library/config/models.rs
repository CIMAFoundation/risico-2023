use std::{collections::HashMap, fs::File, io::{self, BufRead}, path::Path};

use chrono::{DateTime, Utc};
use chrono::*;

use crate::library::state::models;
use crate::library::io::readers::read_input_from_file;

use crate::library::io::models::grid::Grid;

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

#[derive(Debug, Clone)]
pub struct ParseError{
    message: String
}
impl ParseError {
    fn new(message: String) -> ParseError {
        ParseError {
            message: message
        }
    }
}
impl Default for ParseError {
    fn default() -> Self {
        ParseError {
            message: "Parse error".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    
    #[test]
    fn parse_line_fails_for_malformed_line() {
        let line = "test/path/to/foo.txt";
        let result = parse_line(&line);
        assert!(result.is_err());
    }

    #[test]
    fn parse_line_ok() {
        let line = "test/path/to/202205060000_grid_variable.txt";
        let result = parse_line(&line);
        assert!(result.is_ok());
        let (grid, variable, date) = result.unwrap();
        assert_eq!(grid, "grid");
        assert_eq!(variable, "variable");
        assert_eq!(date, DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2022, 5, 6).and_hms(0, 0, 0), Utc));
    }
}

/// Parse an input filename and return a tuple with grid_name, variable and datetime
fn parse_line(line: &str) -> Result<(String, String, DateTime<Utc>), ParseError> {
    let filename = Path::new(&line).file_name();

    let filename = match filename {
        Some(filename) => filename.to_str(),
        None => return Err(
            ParseError::new(format!("Invalid line in input file list: {line}"))
        )
    };
    let name_and_ext = filename.unwrap().split('.').collect::<Vec<&str>>();
    
    if name_and_ext.len() == 0 || name_and_ext.len() > 2 {
        return Err(
            ParseError::new(format!("Error parsing filename {line}"))
        )
    }

    let name = name_and_ext[0];
    let components: Vec<&str> = name.split('_').collect();
    
    if components.len() != 3 {
        return Err(ParseError::new(format!("Error parsing filename {name}")));
    }
    
    let date = components[0];
    let grid_name = components[1].to_string();
    let variable = components[2].to_string();    

    // parse the date
    println!("Parsing date: {}", date);
    let date = match NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")  {
        Ok(date) => DateTime::<Utc>::from_utc(date, Utc),
        Err(error) => return Err(ParseError::new(format!("Error parsing date: {error}")))
    };

    Ok((grid_name, variable, date))
}

#[derive(Debug)]
pub struct LazyInputFile {
    pub grid_name: String,
    pub path: String,
    pub data: Option<Vec<f32>>
}

impl LazyInputFile {
    pub fn new(grid_name: String, path: String) -> LazyInputFile {
        LazyInputFile {
            grid_name: grid_name,
            path: path,
            data: None
        }
    }

    pub fn load(&mut self, grid_registry: &mut HashMap<String, Grid>) -> Result<(), ParseError> {
        if !self.data.is_none(){
            return Ok(());
        }

        let (grid, data) = match read_input_from_file(&self.path) {
            Ok((grid, data)) => (grid, data),
            Err(error) => return Err(ParseError::new(format!("Error reading input file {}: {error}", self.path)))
        };
        self.data = Some(data);
        
        // insert the grid in the registry if not already present
        if !grid_registry.contains_key(&self.grid_name) {
            grid_registry.insert(self.grid_name.clone(), grid);
        }
        
        Ok(())
    }
}

#[derive(Debug)]
pub struct InputDataHandler {
    pub grid_registry: HashMap<String, Grid>,
    pub data_map: HashMap<DateTime<Utc>, HashMap<String, LazyInputFile>>,
}

impl InputDataHandler{
    pub fn new(file_path: &str) -> InputDataHandler{
        let mut handler =  InputDataHandler {
            grid_registry: HashMap::new(),
            data_map: HashMap::new()
        };
        
        let data_map =  &mut handler.data_map;

        let file = File::open(file_path).expect(&format!("Can't open input file {}", file_path));


        // file is a text file in which each line is a file with the following structure:
        // directory/<YYYYmmDDHHMM>_<grid_name>_<variable>.<extension>
        // read the file and parse the lines
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line.unwrap();
            
            if !line.ends_with(".zbin") { continue; }
            

            let maybe_parsed = parse_line(&line);
            if maybe_parsed.is_err() {
                let err = maybe_parsed.err();
                print!("Error parsing filename {line}: {err:?}");
                continue
            }

            let (grid_name, variable, date) = maybe_parsed.unwrap();
            
            let date = date.with_timezone(&Utc);

            let lazy_input_file = LazyInputFile::new(grid_name, line);
            // add the data to the data map
            if !data_map.contains_key(&date) {
                data_map.insert(date, HashMap::new());
            }
            let data_map = data_map.get_mut(&date).unwrap();
            data_map.insert(variable.to_string(), lazy_input_file);

        }

        handler
        
    }
    
    /// Returns the data for the given date and variable on the selected coordinates
    pub fn get_value(&mut self, var:&str, date: &DateTime<Utc>, lat: f32, lon: f32) -> f32 {
        let data_map = self.data_map.get_mut(date).expect(&format!("No data for date {date}"));
        let lazy_file = data_map.get_mut(var).expect(&format!("No data for variable {var}"));

        if lazy_file.data.is_none() {
            lazy_file.load(&mut self.grid_registry).expect(&format!("Error loading file {}", lazy_file.path));
        }
        let data = &lazy_file.data;
        let data = data.as_ref().unwrap();

        let grid = self.grid_registry.get_mut(&lazy_file.grid_name).unwrap();
        let index = grid.get_index(lat, lon);

        data[index]
    }


    /// Returns the timeline
    pub fn get_timeline(&self) -> Vec<DateTime<Utc>> {
        let mut timeline: Vec<DateTime<Utc>> = Vec::new();
        for date in self.data_map.keys() {
            timeline.push(*date);            
        }
        // sort the timeline
        timeline.sort();
        timeline
    }

    // returns the variables at given time
    pub fn get_variables(&self, time: &DateTime<Utc>) -> Vec<String> {
        let mut variables: Vec<String> = Vec::new();
        let data_map = self.data_map.get(time).unwrap();
        for var in data_map.keys() {
            variables.push(var.to_string());
        }
        variables
    }
}


