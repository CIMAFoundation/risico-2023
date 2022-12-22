use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::{self, BufRead, Write},
    path::Path,
    
};

use chrono::*;
use chrono::{DateTime, Utc};
use itertools::izip;

use crate::library::{io::{readers::read_input_from_file, models::output::{OutputType, OutputVariable}}, state::{models::Cell, constants::NODATAVAL}};
use crate::{
    library::{
        io::models::grid::{ClusterMode, Grid},
        state::models::{self, State},
    }
};

use super::data::{read_cells_properties, read_vegetation};

type ConfigMap = HashMap<String, Vec<String>>;

trait ConfigMapExt {
    /// Get the first value of a key in the config map
    fn first(&self, key: &str) -> Option<String>;
    fn all(&self, key: &str) -> Option<Vec<String>>;
}

impl ConfigMapExt for ConfigMap {
    fn first(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|values| values.get(0).cloned())
    }

    fn all(&self, key: &str) -> Option<Vec<String>> {
        self.get(key).cloned()
    }
}

#[derive(Debug)]
pub struct ConfigError {
    msg: String,
}

impl From<String> for ConfigError {
    fn from(msg: String) -> Self {
        ConfigError { msg }
    }
}

impl From<&str> for ConfigError {
    fn from(msg: &str) -> Self {
        ConfigError { msg: msg.into() }
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

/// Read the config file and return a map of key-value pairs
pub fn read_config(file_name: impl Into<String>) -> Result<ConfigMap, ConfigError> {
    let file_name = file_name.into();
    // open file as text and read it using a buffered reader
    let file =
        File::open(&file_name).map_err(|error| format!("error opening config file: {error}"))?;
    let reader = io::BufReader::new(file);
    let lines = reader.lines();

    let mut config_map: ConfigMap = ConfigMap::new();

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
        let key = parts
            .next()
            .ok_or(format!("error parsing on line[{i}] {line}."))?;
        let value = parts.next().ok_or(format!(
            "error parsing value for key {key}: line[{i}] {line}."
        ))?;

        if config_map.contains_key(key) {
            config_map
                .get_mut(key)
                .expect("It must have a value here!")
                .push(value.into());
        } else {
            config_map.insert(key.into(), vec![value.into()]);
        }
    }
    Ok(config_map)
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

impl Config {
    fn parse_output_types(
        output_types_defs: Vec<String>,
        variables_defs: Vec<String>,
    ) -> Result<Vec<OutputType>, ConfigError> {
        let mut output_types_map: HashMap<String, OutputType> = HashMap::new();

        for out_type_def in output_types_defs {
            let parts = out_type_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid output definition".into());
            }
            let (internal_name, name, path, grid_path, format) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);

            let mut output_type = OutputType::new(name, path, grid_path, format)
            .map_err(|_err| 
                format!(
                    "Invalid output type definition: {out_type_def}",
                    out_type_def = out_type_def
                )
            )?;
            output_types_map.insert(internal_name.to_string(), output_type);
        }

        for variable_def in variables_defs {
            let parts = variable_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid variable definition".into());
            }
            let (output_type, internal_name, name, cluster_mode, precision) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);

            let precision = precision.parse::<i32>().map_err(|_| "Invalid precision")?;

            let cluster_mode = match cluster_mode {
                "MEAN" | "mean" => ClusterMode::Mean,
                "MAX" | "max" => ClusterMode::Max,
                "MIN" | "min" => ClusterMode::Min,
                _ => return Err("Invalid cluster mode".into()),
            };
            let mut variable = OutputVariable::new(internal_name, name, cluster_mode, precision);

            let output_type = output_types_map
                .get_mut(output_type)
                .ok_or(format!("Output type not found {output_type}"))?;
            output_type.add_variable(variable);
        }

        let output_types = output_types_map.into_values().collect();

        Ok(output_types)
    }

    pub fn new(config_file: &str, date: DateTime<Utc>) -> Result<Config, ConfigError> {
        let config_map = read_config(config_file)?;

        // try to get the model name, expect it to be there
        let model_name = config_map
            .first(MODEL_NAME_KEY)
            .ok_or(format!("Error: {MODEL_NAME_KEY} not found in config"))?;

        let warm_state_path = config_map
            .first(WARM_STATE_PATH_KEY)
            .ok_or(format!("Error: {WARM_STATE_PATH_KEY} not found in config"))?;

        let cells_file = config_map
            .first(CELLS_FILE_KEY)
            .ok_or(format!("Error: {CELLS_FILE_KEY} not found in config"))?;

        let vegetation_file = config_map
            .first(VEGETATION_FILE_KEY)
            .ok_or(format!("Error: {VEGETATION_FILE_KEY} not found in config"))?;

        let cache_path = config_map
            .first(CACHE_PATH_KEY)
            .ok_or(format!("Error: {CACHE_PATH_KEY} not found in config"))?;

        let ppf_file = config_map.first(PPF_FILE_KEY);

        let use_temperature_effect = match config_map.first(USE_TEMPERATURE_EFFECT_KEY) {
            Some(value) => match value.as_str() {
                "true" | "True" | "TRUE" | "1" => true,
                _ => false,
            },
            None => false,
        };

        let use_ndvi = match config_map.first(USE_NDVI_KEY) {
            Some(value) => match value.as_str() {
                "true" | "True" | "TRUE" | "1" => true,
                _ => false,
            },
            None => false,
        };

        let output_types_defs = config_map
            .all(OUTPUTS_KEY)
            .ok_or(format!("KEY {OUTPUTS_KEY} not found"))?;
        let variables_defs = config_map
            .all(VARIABLES_KEY)
            .ok_or(format!("KEY {VARIABLES_KEY} not found"))?;

        let output_types = Self::parse_output_types(output_types_defs, variables_defs)?;

        let mut cells = read_cells_properties(&cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let vegetations = read_vegetation(&vegetation_file)
            .map_err(|error| format!("error reading {}, {error}", vegetation_file))?;

        let warm_state =
            read_warm_state(&warm_state_path, date)
                .unwrap_or(vec![WarmState::default(); cells.len()]);

        let ppf = match ppf_file {
            Some(ppf_file) => read_ppf(&ppf_file)
                .map_err(|error| format!("error reading {}, {}", &ppf_file, error))?,
            None => vec![(1.0, 1.0); cells.len()],
        };

        let config = Config {
            model_name: model_name,
            warm_state_path: warm_state_path,
            warm_state,
            cells_properties: cells,
            ppf,
            vegetations,
            outputs: output_types,
            use_temperature_effect: use_temperature_effect,
            use_ndvi: use_ndvi,
        };

        Ok(config)
    }

    
    pub fn new_state(&self, date: DateTime<Utc>) -> State {
        let cells = izip!(&self.cells_properties, &self.warm_state, &self.ppf)
            .map(|(props, warm_state, ppf)| {
                let veg = self.vegetations.get(&props.vegetation).unwrap();
                Cell::new(&props, &warm_state, &veg)
            })
            .collect();
        
        State {
            cells: cells,
            time: date,
        }
    }
    
    pub fn write_output(&self, state: &State) -> Result<(), ConfigError> {
        for output in &self.outputs {
            match output.write_variables(state) {
                Ok(_) => (),
                Err(e) => println!("Error writing output: {}", e)
            }
        }
        Ok(())
    }

    pub fn write_warm_state(&self, state: &State) -> Result<(), ConfigError> {
        let date_string = state.time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}_{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        for cell in &state.cells {
            let dffm = cell.state.dffm;
            let NDSI = NODATAVAL; //cell.state.NDSI;
            let NDSI_TTL = NODATAVAL;  //cell.state.NDSI_TTL;
            let MSI = NODATAVAL;  //cell.state.MSI;
            let MSI_TTL = NODATAVAL;  //cell.state.MSI_TTL;
            let NDVI = NODATAVAL;  //cell.state.NDVI;
            let NDVI_TIME = NODATAVAL;  //cell.state.NDVI_TIME;
            let NDWI = NODATAVAL;  //cell.state.NDWI;
            let NDWI_TTL = NODATAVAL;  //cell.state.NDWI_TTL;

            let line = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                dffm, NDSI, NDSI_TTL, MSI, MSI_TTL, NDVI, NDVI_TIME, NDWI, NDWI_TTL
            );
            writeln!(warm_state_file, "{}", line).map_err(|error| {
                format!(
                    "error writing to {}, {}",
                    &warm_state_name, error
                )
            })?;            
        }         
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Config {
    pub model_name: String,
    pub warm_state_path: String,

    pub warm_state: Vec<WarmState>,
    pub cells_properties: Vec<models::Properties>,
    pub ppf: Vec<(f32, f32)>,
    pub vegetations: HashMap<String, models::Vegetation>,

    pub outputs: Vec<OutputType>,
    pub use_temperature_effect: bool,
    pub use_ndvi: bool,
}


#[derive(Debug, Clone)]
pub struct InputFileParseError {
    message: String,
}

impl From<std::io::Error> for InputFileParseError {
    fn from(err: std::io::Error) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}

impl From<&str> for InputFileParseError {
    fn from(err: &str) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}

impl From<String> for InputFileParseError {
    fn from(err: String) -> Self {
        Self { message: err }
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
    pub data: Option<Vec<f32>>,
}

impl LazyInputFile {
    pub fn new(grid_name: String, path: String) -> LazyInputFile {
        LazyInputFile {
            grid_name,
            path,
            data: None,
        }
    }

    pub fn load(
        &mut self,
        grid_registry: &mut HashMap<String, Box<dyn Grid>>,
    ) -> Result<(), InputFileParseError> {
        if !self.data.is_none() {
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
    pub grid_registry: HashMap<String, Box<dyn Grid>>,
    pub data_map: HashMap<DateTime<Utc>, HashMap<String, LazyInputFile>>,
}


impl InputDataHandler {
    pub fn new(file_path: &str) -> InputDataHandler {
        let mut handler = InputDataHandler {
            grid_registry: HashMap::new(),
            data_map: HashMap::new(),
        };

        let data_map = &mut handler.data_map;

        let file = File::open(file_path).expect(&format!("Can't open input file {}", file_path));

        // file is a text file in which each line is a file with the following structure:
        // directory/<YYYYmmDDHHMM>_<grid_name>_<variable>.<extension>
        // read the file and parse the lines
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line.unwrap();

            if !line.ends_with(".zbin") {
                continue;
            }

            let maybe_parsed = parse_line(&line);
            if maybe_parsed.is_err() {
                let err = maybe_parsed.err();
                print!("Error parsing filename {line}: {err:?}");
                continue;
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

    pub fn load_data(&mut self, date: &DateTime<Utc>, lats: &[f32], lons: &[f32]) {
        let data_map = self.data_map.get_mut(date).unwrap();
        for (_, lazy_file) in data_map.iter_mut() {
            if lazy_file.data.is_none() {
                lazy_file
                    .load(&mut self.grid_registry)
                    .expect(&format!("Error loading file {}", lazy_file.path));
            }
            // build cache
            let _ = lazy_file.data.as_ref().unwrap();
            let grid = self.grid_registry.get_mut(&lazy_file.grid_name).unwrap();
            grid.build_cache(lats, lons);
        }
    }

    /// Returns the data for the given date and variable on the selected coordinates
    pub fn get_value(&self, var: &str, date: &DateTime<Utc>, lat: f32, lon: f32) -> f32 {
        let data_map = self
            .data_map
            .get(date)
            .expect(&format!("No data for date {date}"));
        let lazy_file = data_map
            .get(var)
            .expect(&format!("No data for variable {var}"));

        let data = &lazy_file.data;
        let data = data.as_ref().unwrap();

        let grid = self.grid_registry.get(&lazy_file.grid_name).unwrap();
        let index = grid.index(&lat, &lon);

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
    pub NDSI: f32,
    pub NDSI_TTL: f32,
    pub MSI: f32,
    pub MSI_TTL: f32,
    pub NDVI: f32,
    pub NDVI_TIME: f32,
    pub NDWI: f32,
    pub NDWI_TIME: f32,
}

/// Reads the warm state from the file
/// The warm state is stored in a file with the following structure:
/// <base_warm_file>_<YYYYmmDDHHMM>
/// where <base_warm_file> is the base name of the file and <YYYYmmDDHHMM> is the date of the warm state
/// The warm state is stored in a text file with the following structure:
/// dffm
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
            println!(
                "Could not find warm state file for {}",
                current_date.format("%Y%m%d%H%M")
            );
            continue;
        }
        file = Some(file_handle.expect("Should unwrap"));
        break;
    }
    if file.is_none() {
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

        let NDSI = components[1].parse::<f32>().unwrap();
        let NDSI_TTL = components[2].parse::<f32>().unwrap();

        let MSI = components[3].parse::<f32>().unwrap();
        let MSI_TTL = components[4].parse::<f32>().unwrap();

        let NDVI = components[5].parse::<f32>().unwrap();
        let NDVI_TIME = components[6].parse::<f32>().unwrap();

        let NDWI = components[7].parse::<f32>().unwrap();
        let NDWI_TIME = components[8].parse::<f32>().unwrap();

        warm_state.push(WarmState {
            dffm,
            NDSI,
            NDSI_TTL,
            MSI,
            MSI_TTL,
            NDVI,
            NDVI_TIME,
            NDWI,
            NDWI_TIME,
        });
    }
    Some(warm_state)
}

/// Reads the PPF file and returns a vector of with (ppf_summer, ppf_winter) tuples
/// The PPF file is a text file with the following structure:
/// ppf_summer ppf_winter
/// where ppf_summer and ppf_winter are floats
pub fn read_ppf(ppf_file: &str) -> Result<Vec<(f32, f32)>, ConfigError> {
    let file = File::open(ppf_file)
        .map_err(|error| format!("Could not open file {}: {}", ppf_file, error))?;

    let reader = io::BufReader::new(file);
    let mut ppf: Vec<(f32, f32)> = Vec::new();
    for line in reader.lines() {
        let line = line.unwrap();
        let components: Vec<&str> = line.split_whitespace().collect();
        let lat = components[0].parse::<f32>().unwrap();
        let lon = components[1].parse::<f32>().unwrap();
        ppf.push((lat, lon));
    }
    Ok(ppf)
}
