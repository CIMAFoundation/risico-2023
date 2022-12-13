use std::{collections::HashMap, fs::File, io::{self, BufRead}, path::Path, error::{Error, self}};

use chrono::{DateTime, Utc};
use chrono::*;

use crate::library::state::models;
use crate::library::io::readers::read_input_from_file;

use crate::library::io::models::grid::Grid;

use super::data::{read_cells_properties, read_vegetation};

#[derive(Debug)]
pub struct ConfigError {
    msg: String,
}

impl From<String> for ConfigError {
    fn from(msg: String) -> Self {
        ConfigError {
            msg
        }
    }
}

impl From<&str> for ConfigError {
    fn from(msg: &str) -> Self {
        ConfigError {
            msg: msg.into(),
        }
    }
}

/// Read the config file and return a map of key-value pairs
pub fn read_config(file_name: impl Into<String>) -> Result<HashMap<String, Vec<String>>, ConfigError>{
    let file_name = file_name.into();
    // open file as text and read it using a buffered reader
    let file = File::open(&file_name).map_err(|error| format!("error opening config file: {error}"))?;
    let reader = io::BufReader::new(file);
    let lines = reader.lines();    
    
    let mut hashmap: HashMap<String, Vec<String>> = HashMap::new();
    
    for (i, line) in lines.enumerate() {
        let line = line.map_err(|error| format!("error line: {i} \n {error}"))?;
        let line = line.trim().to_string();

        if line.starts_with("%") || line.is_empty() {
            continue;
        }
        if !line.contains("=") {
            return Err(format!("error parsing config file {file_name} at line {i}.").into());
        }
        // implement split using regex
        let mut parts = line.split("=");
        let key = parts.next().ok_or(
            format!("error parsing on line[{i}] {line}.")
        )?;
        let value = parts.next().ok_or(
            format!("error parsing value for key {key}: line[{i}] {line}.")
        )?;

        if hashmap.contains_key(key) {
            hashmap.get_mut(key).expect("It must have a value here!").push(value.into());
        } else {
            hashmap.insert(key.into(), vec![value.into()]);
        }

    }
    Ok(hashmap)
}

#[derive(Default, Debug, Clone)]
struct ConfigOutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: String,
    precision: String,
}

#[derive(Default, Debug, Clone)]
pub struct ConfigOutputType {
    name: String,
    path: String,
    grid: String,
    format: String,
    variables: Vec<ConfigOutputVariable>,
}



#[derive(Default, Debug)]
pub struct ConfigBuilder {
    pub model_name: String,
    pub warm_state_path: String,
    pub cells_file: String,
    pub vegetation_file: String,
    pub ppf_file: Option<String>,
    pub cache_path: String,
    pub use_temperature_effect: bool,
    pub use_ndvi: bool,
    pub outputs: Vec<ConfigOutputType>,
}

const MODEL_NAME_KEY: &str = "MODELNAME";
const WARM_STATE_PATH_KEY: &str = "STATO0";
const CELLS_FILE_KEY: &str = "CELLE";
const VEGETATION_FILE_KEY: &str = "VEG";
const PPF_FILE_KEY: &str = "PPF";
const CACHE_PATH_KEY: &str = "BUFFERS";
const USE_TEMPERATURE_EFFECT_KEY: &str = "USETCONTR";
const USE_NDVI_KEY: &str = "USENDVI";
const OUTPUTS_KEY: &str = "MODEL";
const VARIABLES_KEY: &str = "VARIABLE";


impl ConfigBuilder {
    pub fn new(config_file: &str) -> Result<ConfigBuilder, ConfigError> {
        let config_map = read_config(config_file)?;

        let mut config_builder = ConfigBuilder::default();

        
        // try to get the model name, expect it to be there
        let model_name = config_map
            .get(MODEL_NAME_KEY)
            .ok_or(format!("Error: {MODEL_NAME_KEY} not found in config"))?
            .get(0)
            .expect("msg");
        let warm_state_file = config_map.get(WARM_STATE_PATH_KEY).unwrap().get(0).unwrap();
        let cells_file = config_map.get(CELLS_FILE_KEY).unwrap().get(0).unwrap();
        let vegetation_file = config_map.get(VEGETATION_FILE_KEY).unwrap().get(0).unwrap();
        let ppf_file = config_map.get(PPF_FILE_KEY).unwrap().get(0).unwrap();
        let cache_path = config_map.get(CACHE_PATH_KEY).unwrap().get(0).unwrap();
        let use_temperature_effect = config_map
            .get(USE_TEMPERATURE_EFFECT_KEY)
            .unwrap()
            .get(0)
            .unwrap();
        let use_ndvi = config_map.get(USE_NDVI_KEY).unwrap().get(0).unwrap();

        let output_types_defs = config_map.get(OUTPUTS_KEY).ok_or("Outputs not found")?;
        let variables_defs = config_map.get(VARIABLES_KEY).ok_or("Variables not found")?;

        let mut output_types_map: HashMap<String, ConfigOutputType> = HashMap::new();
        for output_def in output_types_defs {
            let parts = output_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid output definition".into());
            }
            let (internal_name, name, path, grid, format) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);
            let mut output_type = ConfigOutputType::default();

            output_type.name = name.to_string();
            output_type.path = path.to_string();
            output_type.grid = grid.to_string();
            output_type.format = format.to_string();

            output_types_map.insert(internal_name.to_string(), output_type);
        }

        for variable_def in variables_defs {
            let parts = variable_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid variable definition".into());
            }
            let (output_type, internal_name, name, cluster_mode, precision) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);
            let mut variable = ConfigOutputVariable::default();
            variable.internal_name = internal_name.to_string();
            variable.name = name.to_string();
            variable.cluster_mode = cluster_mode.to_string();
            variable.precision = precision.to_string();

            let output_type = output_types_map
                .get_mut(output_type)
                .ok_or(format!("Output type not found {output_type}"))?;
            output_type.variables.push(variable);
        }

        config_builder.model_name = model_name.to_string();
        config_builder.warm_state_path = warm_state_file.to_string();
        config_builder.cells_file = cells_file.to_string();
        config_builder.vegetation_file = vegetation_file.to_string();
        config_builder.ppf_file = Some(ppf_file.to_string());
        config_builder.cache_path = cache_path.to_string();
        config_builder.use_temperature_effect = match use_temperature_effect.as_str() {
            "0" => false,
            "1" => true,
            "true" => true,
            "false" => false,
            "TRUE" => true,
            "FALSE" => false,
            _ => false,
        };
        config_builder.use_ndvi = match use_ndvi.as_str() {
            "0" => false,
            "1" => true,
            "true" => true,
            "false" => false,
            "TRUE" => true,
            "FALSE" => false,
            _ => false,
        };
        config_builder.outputs = output_types_map
            .values()
            .map(|output| output.clone())
            .collect();

        Ok(config_builder)
    }

    /// Build the config from the builder
    /// read the cells and vegetation files
    /// read the warm state file according to date
    /// read the ppf file and store the values in the cells
    /// return the config
    pub fn build(&self, date: DateTime<Utc>) -> Result<Config, ConfigError> {
        let mut cells = read_cells_properties(&self.cells_file).map_err(|error| format!("error reading {}, {error}", self.cells_file))?;
        let vegetations = read_vegetation(&self.vegetation_file).map_err(|error| format!("error reading {}, {error}", self.vegetation_file))?;
        let warm_state = read_warm_state(&self.warm_state_path, date).ok_or(vec![WarmState::default(); cells.len()]);

        
        Err(ConfigError { msg: "unimplemnted".to_string() })
    }
}





pub struct Config {
    pub model_name: String,
    pub warm_state_path: String,

    pub warm_state: Vec<WarmState>,    
    pub cells: Vec<models::Properties>,
    pub vegetations: HashMap<String, models::Vegetation>,
    
    pub outputs: Vec<ConfigOutputType>,
    pub use_temperature_effect: bool,
    pub use_ndvi: bool,


}

impl Config {

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
pub struct InputFileParseError{
    message: String
}

impl From<std::io::Error> for InputFileParseError {
    fn from(err: std::io::Error) -> Self {
        Self {
            message: err.to_string()
        }
    }
}

impl From<&str> for InputFileParseError{
    fn from(err: &str) -> Self {
        Self {
            message: err.to_string()
        }
    }
}


impl From<String> for InputFileParseError{
    fn from(err: String) -> Self {
        Self {
            message: err
        }
    }
}

/// Parse an input filename and return a tuple with grid_name, variable and datetime
fn parse_line(line: &str) -> Result<(String, String, DateTime<Utc>), InputFileParseError> {
    let filename = Path::new(&line)
                        .file_name()
                        .ok_or(format!("Invalid line in input file list: {line}"))?
                        .to_str()
                        .expect("Should be a valid string");

    let name_and_ext = filename.split('.').collect::<Vec<&str>>();
    
    if name_and_ext.len() == 0 || name_and_ext.len() > 2 {
        return Err(format!("Error parsing filename {line}").into());
    }

    let name = name_and_ext[0];
    let components: Vec<&str> = name.split('_').collect();
    
    if components.len() != 3 {
        return Err(format!("Error parsing filename {name}").into());
    }
    
    let date = components[0];
    let grid_name = components[1].to_string();
    let variable = components[2].to_string();    

    // parse the date
    println!("Parsing date: {}", date);
    let date = NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
            .map_err(|error| format!("Error parsing date: {error}"))?;

    let date = DateTime::<Utc>::from_utc(date, Utc);

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
            grid_name,
            path,
            data: None
        }
    }

    pub fn load(&mut self, grid_registry: &mut HashMap<String, Grid>) -> Result<(), InputFileParseError> {
        if !self.data.is_none(){
            return Ok(());
        }

        let (grid, data) = read_input_from_file(&self.path)
                            .map_err(|error| format!("Error reading input file {}: {error}", self.path))?;
        
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

#[derive(Default, Debug, Clone)]
pub struct WarmState {
    pub dffm: f32,    
}

fn read_warm_state(base_warm_file: &str, date: DateTime<Utc>) -> Option<Vec<WarmState>> {
    // for the last n days before date, try to read the warm state
    // compose the filename as <base_warm_file>_<YYYYmmDDHHMM>  
    let mut file: Option<File> = None;

    for days_before in 0..5 {
        let mut current_date = date.clone();
        current_date = current_date - Duration::days(days_before);
        let filename = format!("{}_{}", base_warm_file, current_date.format("%Y%m%d%H%M"));

        let file_handle = File::open(filename);
        if file_handle.is_err() {
            println!("Could not find warm state file for {}", current_date.format("%Y%m%d%H%M"));
            continue;
        }   
        file = Some(file_handle.expect("Should unwrap")); 
        break;
    }
    if file.is_none()  {
        println!("Warning warm file not found");
        return None;
    }

    let mut warm_state: Vec<WarmState> = Vec::new();
    let file = file.unwrap();
    let reader = io::BufReader::new(file);
    for line in reader.lines() {
        let line = line.unwrap();
        let components: Vec<&str> = line.split_whitespace().collect();
        let dffm = components[0].parse::<f32>().unwrap();
        warm_state.push(WarmState{dffm});
    }
    Some(warm_state)
}


