use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::{self, BufRead, BufWriter, Write},
    path::Path,
};

use chrono::*;
use chrono::{DateTime, Utc};
use log::{info, warn};
use ndarray::Array1;
use rayon::prelude::*;

use crate::library::{io::models::grid::ClusterMode, state::models::State};
use crate::library::{
    io::{
        models::{
            output::{OutputType, OutputVariable},
            palette::Palette,
        },
        readers::{read_grid_from_file, read_values_from_file},
    },
    state::{
        config::ModelConfig,
        constants::NODATAVAL,
        models::{Output, Properties},
    },
};

use super::data::{read_cells_properties, read_vegetation};

pub type PaletteMap = HashMap<String, Box<Palette>>;
pub type ConfigMap = HashMap<String, Vec<String>>;

const MODEL_NAME_KEY: &str = "MODELNAME";
const WARM_STATE_PATH_KEY: &str = "STATO0";
const CELLS_FILE_KEY: &str = "CELLE";
const VEGETATION_FILE_KEY: &str = "VEG";
const PPF_FILE_KEY: &str = "PPF";
const CACHE_PATH_KEY: &str = "BUFFERS";

const MODEL_VERSION_KEY: &str = "MODEL_VERSION";

const USE_TEMPERATURE_EFFECT_KEY: &str = "USETCONTR";
const USE_NDVI_KEY: &str = "USENDVI";
const OUTPUTS_KEY: &str = "MODEL";
const VARIABLES_KEY: &str = "VARIABLE";
const PALETTE_KEY: &str = "PALETTE";
const KEY_HOURSRESOLUTION: &str = "OUTPUTHRES";

pub trait ConfigMapExt {
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
pub struct RISICOError {
    msg: String,
}

impl From<String> for RISICOError {
    fn from(msg: String) -> Self {
        RISICOError { msg }
    }
}

impl From<&str> for RISICOError {
    fn from(msg: &str) -> Self {
        RISICOError { msg: msg.into() }
    }
}

impl Display for RISICOError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

/// Read the config file and return a map of key-value pairs
pub fn read_config(file_name: impl Into<String>) -> Result<ConfigMap, RISICOError> {
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

        if line.starts_with("%") || line.starts_with("#") || line.is_empty() {
            // skip comments and empty lines
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

impl Config {
    fn parse_output_types(
        run_date: &DateTime<Utc>,
        output_types_defs: &Vec<String>,
        variables_defs: &Vec<String>,
        palettes: &PaletteMap,
    ) -> Result<Vec<OutputType>, RISICOError> {
        let mut output_types_vec: Vec<OutputType> = Vec::new();

        for out_type_def in output_types_defs {
            let parts = out_type_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid output definition".into());
            }
            let (internal_name, name, path, grid_path, format) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);

            let output_type = OutputType::new(
                internal_name,
                name,
                path,
                grid_path,
                format,
                run_date,
                palettes.clone(),
            )
            .map_err(|_err| {
                format!(
                    "Invalid output type definition: {out_type_def}",
                    out_type_def = out_type_def
                )
            })?;
            output_types_vec.push(output_type);
        }

        for variable_def in variables_defs {
            let parts = variable_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid variable definition".into());
            }
            let (output_type, internal_name, name, cluster_mode, precision) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);

            let precision = precision.parse::<i32>().map_err(|_| "Invalid precision")?;

            output_types_vec
                .iter_mut()
                .filter(|_type| _type.internal_name == output_type)
                .for_each(|_type| {
                    _type.add_variable(OutputVariable::new(
                        internal_name,
                        name,
                        ClusterMode::from(cluster_mode),
                        precision,
                    ))
                });
        }

        Ok(output_types_vec)
    }

    fn load_palettes(config_map: &ConfigMap) -> HashMap<String, Box<Palette>> {
        let mut palettes: HashMap<String, Box<Palette>> = HashMap::new();
        let palettes_defs = config_map.all(PALETTE_KEY);
        if palettes_defs.is_none() {
            return palettes;
        }
        let palettes_defs = palettes_defs.expect("should be there");

        for palette_def in palettes_defs {
            let parts = palette_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 2 {
                continue;
            }
            let (name, path) = (parts[0], parts[1]);

            if let Ok(palette) = Palette::load_palette(path) {
                palettes.insert(name.to_string(), Box::new(palette));
            }
        }
        palettes
    }

    pub fn new(config_file: &str, date: DateTime<Utc>) -> Result<Config, RISICOError> {
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

        let palettes = Config::load_palettes(&config_map);

        // let cache_path = config_map
        //     .first(CACHE_PATH_KEY)
        //     .ok_or(format!("Error: {CACHE_PATH_KEY} not found in config"))?;

        let model_version = match config_map.first(MODEL_VERSION_KEY) {
            Some(value) => value,
            None => "legacy".to_owned(),
        };

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
        let output_time_resolution = match config_map.first(KEY_HOURSRESOLUTION) {
            Some(value) => value.parse::<u32>().unwrap_or(3),
            None => 3,
        };

        let output_types_defs = config_map
            .all(OUTPUTS_KEY)
            .ok_or(format!("KEY {OUTPUTS_KEY} not found"))?;
        let variables_defs = config_map
            .all(VARIABLES_KEY)
            .ok_or(format!("KEY {VARIABLES_KEY} not found"))?;

        let (lats, lons, slopes, aspects, vegetations) = read_cells_properties(&cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;

        let n_cells = lons.len();
        if n_cells != lats.len()
            || n_cells != slopes.len()
            || n_cells != aspects.len()
            || n_cells != vegetations.len()
        {
            panic!("All properties must have the same length");
        }

        let vegetations_dict = read_vegetation(&vegetation_file)
            .map_err(|error| format!("error reading {}, {error}", vegetation_file))?;

        let (warm_state, warm_state_time) = read_warm_state(&warm_state_path, date).unwrap_or((
            vec![WarmState::default(); n_cells],
            date.clone() - Duration::days(1),
        ));

        let ppf = match ppf_file {
            Some(ppf_file) => read_ppf(&ppf_file)
                .map_err(|error| format!("error reading {}, {}", &ppf_file, error))?,
            None => vec![(1.0, 1.0); n_cells],
        };
        let ppf_summer = ppf.iter().map(|(s, _)| *s).collect();
        let ppf_winter = ppf.iter().map(|(_, w)| *w).collect();

        let props = Properties::new(
            lats,
            lons,
            slopes,
            aspects,
            vegetations,
            vegetations_dict,
            ppf_summer,
            ppf_winter,
        );

        let config = Config {
            run_date: date,
            model_name,
            warm_state_path,
            warm_state,
            warm_state_time,
            properties: props,
            output_types_defs,
            variables_defs,
            palettes,
            use_temperature_effect,
            use_ndvi,
            output_time_resolution,
            model_version,
        };

        Ok(config)
    }

    pub fn get_properties(&self) -> &Properties {
        &self.properties
    }

    pub fn new_state(&self) -> State {
        let config = ModelConfig::new(&self.model_version);
        State::new(&self.warm_state, &self.warm_state_time, config)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        let outputs = Self::parse_output_types(
            &self.run_date,
            &self.output_types_defs,
            &self.variables_defs,
            &self.palettes,
        )?;
        Ok(OutputWriter::new(outputs))
    }

    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(&self, state: &State) -> Result<(), RISICOError> {
        let warm_state_time = state.time.clone() + Duration::days(1);
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;

        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);

        for state in &state.data {
            let dffm = state.dffm;

            let MSI = state.MSI; //cell.state.MSI;
            let MSI_TTL = state.MSI_TTL; //cell.state.MSI_TTL;
            let NDVI = state.NDVI; //cell.state.NDVI;
            let NDVI_TIME = state.NDVI_TIME; //cell.state.NDVI_TIME;
            let NDWI = state.NDWI; //cell.state.NDWI;
            let NDWI_TIME = state.NDWI_TIME; //cell.state.NDWI_TTL;
            let snow_cover = state.snow_cover; //cell.state.snow_cover;
            let snow_cover_time = state.snow_cover_time; //cell.state.snow_cover_time;

            let line = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                dffm, snow_cover, snow_cover_time, MSI, MSI_TTL, NDVI, NDVI_TIME, NDWI, NDWI_TIME
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

pub struct Config {
    run_date: DateTime<Utc>,

    model_name: String,
    warm_state_path: String,
    warm_state: Vec<WarmState>,
    warm_state_time: DateTime<Utc>,
    pub properties: Properties,

    output_types_defs: Vec<String>,
    variables_defs: Vec<String>,
    palettes: PaletteMap,
    use_temperature_effect: bool,
    use_ndvi: bool,
    output_time_resolution: u32,
    model_version: String,
}

pub struct OutputWriter {
    outputs: Vec<OutputType>,
}

impl OutputWriter {
    pub fn new(outputs: Vec<OutputType>) -> Self {
        Self { outputs }
    }

    pub fn write_output(
        &mut self,
        lats: &[f32],
        lons: &[f32],
        output: &Output,
    ) -> Result<(), RISICOError> {
        self.outputs.par_iter_mut().for_each(|output_type| {
            match output_type.write_variables(lats, lons, output) {
                Ok(_) => (),
                Err(e) => warn!("Error writing output: {}", e),
            }
        });
        Ok(())
    }
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

    let date = NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
        .map_err(|error| format!("Error parsing date: {error}"))?;

    let date = DateTime::from_naive_utc_and_offset(date, Utc);

    Ok((grid_name, variable, date))
}

#[derive(Debug)]
pub struct InputFile {
    pub grid_name: String,
    pub path: String,
}

#[derive(Debug)]
pub struct InputDataHandler {
    pub grid_registry: HashMap<String, Array1<Option<usize>>>,
    pub data_map: HashMap<DateTime<Utc>, HashMap<String, InputFile>>,
}

impl InputDataHandler {
    pub fn new(file_path: &str, lats: &[f32], lons: &[f32]) -> InputDataHandler {
        let mut grid_registry = HashMap::new();
        let mut data_map = HashMap::new();

        let file = File::open(file_path).expect(&format!("Can't open input file {}", file_path));

        // file is a text file in which each line is a file with the following structure:
        // directory/<YYYYmmDDHHMM>_<grid_name>_<variable>.<extension>
        // read the file and parse the lines
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(e) => {
                    warn!("Error reading line: {}", e);
                    continue;
                }
            };

            if !line.ends_with(".zbin") {
                continue;
            }

            let (grid_name, variable, date) = match parse_line(&line) {
                Ok(parsed) => parsed,
                Err(err) => {
                    warn!("Error parsing filename {line}: {err:?}");
                    continue;
                }
            };

            let date = date.with_timezone(&Utc);
            let input_file = InputFile {
                grid_name,
                path: line,
            };

            if !grid_registry.contains_key(&input_file.grid_name) {
                let mut grid = match read_grid_from_file(input_file.path.as_str()) {
                    Ok(grid) => grid,
                    Err(e) => {
                        warn!("Error reading grid: {}", e);
                        continue;
                    }
                };

                let indexes = grid.indexes(lats, lons);
                grid_registry.insert(input_file.grid_name.clone(), indexes);
            }

            // add the data to the data map
            if !data_map.contains_key(&date) {
                data_map.insert(date, HashMap::new());
            }

            if let Some(data_map_for_date) = data_map.get_mut(&date) {
                data_map_for_date.insert(variable.to_string(), input_file);
            }
        }

        InputDataHandler {
            grid_registry,
            data_map,
        }
    }

    /// Returns the data for the given date and variable on the selected coordinates
    pub fn get_values(&self, var: &str, date: &DateTime<Utc>) -> Option<Array1<f32>> {
        let data_map = match self.data_map.get(date) {
            Some(data_map) => data_map,
            None => return None,
        };

        let file = match data_map.get(var) {
            Some(file) => file,
            None => return None,
        };

        let data = read_values_from_file(file.path.as_str())
            .expect(&format!("Error reading file {}", file.path));

        let indexes = self
            .grid_registry
            .get(&file.grid_name)
            .expect(&format!("there should be a grid named {}", file.grid_name));

        let data = indexes
            .iter()
            .map(|index| index.and_then(|idx| Some(data[idx])).unwrap_or(NODATAVAL))
            .collect();
        Some(data)
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
    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<String> {
        let mut variables: Vec<String> = Vec::new();

        let data_map = self
            .data_map
            .get(time)
            .expect("there should be data for this time");

        for var in data_map.keys() {
            variables.push(var.to_string());
        }
        variables
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct WarmState {
    pub dffm: f32,
    pub snow_cover: f32,
    pub snow_cover_time: f32,
    pub MSI: f32,
    pub MSI_TTL: f32,
    pub NDVI: f32,
    pub NDVI_TIME: f32,
    pub NDWI: f32,
    pub NDWI_TIME: f32,
}

impl Default for WarmState {
    fn default() -> Self {
        WarmState {
            dffm: 40.0,
            snow_cover: 0.0,
            snow_cover_time: 0.0,
            MSI: 0.0,
            MSI_TTL: 0.0,
            NDVI: 0.0,
            NDVI_TIME: 0.0,
            NDWI: 0.0,
            NDWI_TIME: 0.0,
        }
    }
}

#[allow(non_snake_case)]
/// Reads the warm state from the file
/// The warm state is stored in a file with the following structure:
/// <base_warm_file>_<YYYYmmDDHHMM>
/// where <base_warm_file> is the base name of the file and <YYYYmmDDHHMM> is the date of the warm state
/// The warm state is stored in a text file with the following structure:
/// dffm
fn read_warm_state(
    base_warm_file: &str,
    date: DateTime<Utc>,
) -> Option<(Vec<WarmState>, DateTime<Utc>)> {
    // for the last n days before date, try to read the warm state
    // compose the filename as <base_warm_file>_<YYYYmmDDHHMM>
    let mut file: Option<File> = None;

    let mut current_date = date.clone();

    for days_before in 0..4 {
        current_date = date.clone() - Duration::days(days_before);

        let filename = format!("{}{}", base_warm_file, current_date.format("%Y%m%d%H%M"));

        let file_handle = File::open(filename);
        if file_handle.is_err() {
            continue;
        }
        file = Some(file_handle.expect("Should unwrap"));
        break;
    }
    let file = match file {
        Some(file) => file,
        None => {
            warn!(
                "WARNING: Could not find a valid warm state file for run date {}",
                date.format("%Y-%m-%d")
            );
            return None;
        }
    };

    info!(
        "Loading warm state from {}",
        current_date.format("%Y-%m-%d")
    );
    let mut warm_state: Vec<WarmState> = Vec::new();

    let reader = io::BufReader::new(file);

    for line in reader.lines() {
        if let Err(line) = line {
            warn!("Error reading warm state file: {}", line);
            return None;
        }
        let line = line.expect("Should unwrap line");

        let components: Vec<&str> = line.split_whitespace().collect();
        let dffm = components[0]
            .parse::<f32>()
            .expect(&format!("Could not parse dffm from {}", line));
        let snow_cover = components[1]
            .parse::<f32>()
            .expect(&format!("Could not parse snow_cover from {}", line));
        let snow_cover_time = components[2]
            .parse::<f32>()
            .expect(&format!("Could not parse snow_cover_time from {}", line));
        let MSI = components[3]
            .parse::<f32>()
            .expect(&format!("Could not parse MSI from {}", line));
        let MSI_TTL = components[4]
            .parse::<f32>()
            .expect(&format!("Could not parse MSI_TTL from {}", line));
        let NDVI = components[5]
            .parse::<f32>()
            .expect(&format!("Could not parse NDVI from {}", line));
        let NDVI_TIME = components[6]
            .parse::<f32>()
            .expect(&format!("Could not parse NDVI_TIME from {}", line));

        let mut NDWI = NODATAVAL;
        let mut NDWI_TIME = 0.0;

        if components.len() > 7 {
            NDWI = components[7]
                .parse::<f32>()
                .expect(&format!("Could not parse NDWI from {}", line));
            NDWI_TIME = components[8]
                .parse::<f32>()
                .expect(&format!("Could not parse NDWI_TIME from {}", line));
        }

        warm_state.push(WarmState {
            dffm,
            snow_cover,
            snow_cover_time,
            MSI,
            MSI_TTL,
            NDVI,
            NDVI_TIME,
            NDWI,
            NDWI_TIME,
        });
    }

    let current_date = current_date - Duration::days(1);
    Some((warm_state, current_date))
}

/// Reads the PPF file and returns a vector of with (ppf_summer, ppf_winter) tuples
/// The PPF file is a text file with the following structure:
/// ppf_summer ppf_winter
/// where ppf_summer and ppf_winter are floats
pub fn read_ppf(ppf_file: &str) -> Result<Vec<(f32, f32)>, RISICOError> {
    let file = File::open(ppf_file)
        .map_err(|error| format!("Could not open file {}: {}", ppf_file, error))?;

    let reader = io::BufReader::new(file);
    let mut ppf: Vec<(f32, f32)> = Vec::new();
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                return Err(format!("Error reading PPF file {}: {}", ppf_file, error).into());
            }
        };
        let components: Vec<&str> = line.split_whitespace().collect();
        let ppf_summer = components[0]
            .parse::<f32>()
            .map_err(|err| format!("Could not parse value from PPF file {}: {}", ppf_file, err))?;

        let ppf_winter = components[1]
            .parse::<f32>()
            .map_err(|err| format!("Could not parse value from PPF file {}: {}", ppf_file, err))?;
        ppf.push((ppf_summer, ppf_winter));
    }
    Ok(ppf)
}
