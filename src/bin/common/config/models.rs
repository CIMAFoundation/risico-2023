use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufWriter, Write},
};

use chrono::*;
use chrono::{DateTime, Utc};
use log::{info, warn};
use rayon::prelude::*;
use risico::modules::risico::{
    config::ModelConfig,
    constants::NODATAVAL,
    models::{Output, Properties, State, WarmState},
};

use crate::common::io::models::{output::OutputType, palette::Palette};
use crate::common::{helpers::RISICOError, io::readers::netcdf::NetCdfInputConfiguration};

use super::{
    builder::{ConfigBuilder, OutputTypeConfig},
    data::{from_file, read_vegetation},
};

pub type PaletteMap = HashMap<String, Box<Palette>>;
// pub type ConfigMap = HashMap<String, Vec<String>>;

pub struct Config {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<WarmState>,
    warm_state_time: DateTime<Utc>,
    properties: Properties,
    palettes: PaletteMap,
    // use_temperature_effect: bool,
    // use_ndvi: bool,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
    netcdf_input_configuration: Option<NetCdfInputConfiguration>,
}

pub struct OutputWriter {
    outputs: Vec<OutputType>,
}

impl OutputWriter {
    pub fn new(
        outputs_defs: &[OutputTypeConfig],
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Self {
        Self {
            outputs: outputs_defs
                .iter()
                .filter_map(|t| OutputType::new(t, date, palettes).ok())
                .collect(),
        }
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

impl Config {
    fn load_palettes(palettes_defs: &HashMap<String, String>) -> HashMap<String, Box<Palette>> {
        let mut palettes: HashMap<String, Box<Palette>> = HashMap::new();

        for (name, path) in palettes_defs.iter() {
            if let Ok(palette) = Palette::load_palette(path) {
                palettes.insert(name.to_string(), Box::new(palette));
            }
        }
        palettes
    }

    pub fn new(config_defs: &ConfigBuilder, date: DateTime<Utc>) -> Result<Config, RISICOError> {
        let palettes = Config::load_palettes(&config_defs.palettes);

        let cells_file = &config_defs.cells_file_path;

        let props_container = from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len()
            || n_cells != props_container.slopes.len()
            || n_cells != props_container.aspects.len()
            || n_cells != props_container.vegetations.len()
        {
            panic!("All properties must have the same length");
        }

        let vegetations_dict = read_vegetation(&config_defs.vegetation_file)
            .map_err(|error| format!("error reading {}, {error}", &config_defs.vegetation_file))?;

        let (warm_state, warm_state_time) = read_warm_state(&config_defs.warm_state_path, date)
            .unwrap_or((
                vec![WarmState::default(); n_cells],
                date - Duration::try_days(1).expect("Should be a valid duration"),
            ));

        let ppf_file = &config_defs.ppf_file;
        let ppf = match ppf_file {
            Some(ppf_file) => read_ppf(ppf_file)
                .map_err(|error| format!("error reading {}, {}", &ppf_file, error))?,
            None => vec![(1.0, 1.0); n_cells],
        };
        let ppf_summer = ppf.iter().map(|(s, _)| *s).collect();
        let ppf_winter = ppf.iter().map(|(_, w)| *w).collect();

        let props = Properties::new(props_container, vegetations_dict, ppf_summer, ppf_winter);

        let config = Config {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            properties: props,
            palettes,
            // use_temperature_effect: config_defs.use_temperature_effect,
            // use_ndvi: config_defs.use_ndvi,
            output_time_resolution: config_defs.output_time_resolution,
            model_version: config_defs.model_version.clone(),
            netcdf_input_configuration: config_defs.netcdf_input_configuration.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    pub fn get_properties(&self) -> &Properties {
        &self.properties
    }

    pub fn new_state(&self) -> State {
        log::info!("Model version: {}", &self.model_version);
        let config = ModelConfig::new(&self.model_version);
        State::new(&self.warm_state, &self.warm_state_time, config)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(&self, state: &State) -> Result<(), RISICOError> {
        let warm_state_time = state.time + Duration::try_days(1).expect("Should be valid");
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

    pub fn get_netcdf_input_config(&self) -> &Option<NetCdfInputConfiguration> {
        &self.netcdf_input_configuration
    }
}

#[allow(non_snake_case)]
/// Reads the warm state from the file
/// The warm state is stored in a file with the following structure:
/// base_warm_file_YYYYmmDDHHMM
/// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
/// The warm state is stored in a text file with the following structure:
/// dffm
fn read_warm_state(
    base_warm_file: &str,
    date: DateTime<Utc>,
) -> Option<(Vec<WarmState>, DateTime<Utc>)> {
    // for the last n days before date, try to read the warm state
    // compose the filename as base_warm_file_YYYYmmDDHHMM
    let mut file: Option<File> = None;

    let mut current_date = date;

    for days_before in 0..4 {
        current_date = date - Duration::try_days(days_before).expect("Should be valid");

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
            .unwrap_or_else(|_| panic!("Could not parse dffm from {}", line));
        let snow_cover = components[1]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
        let snow_cover_time = components[2]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Could not parse snow_cover_time from {}", line));
        let MSI = components[3]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Could not parse MSI from {}", line));
        let MSI_TTL = components[4]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Could not parse MSI_TTL from {}", line));
        let NDVI = components[5]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Could not parse NDVI from {}", line));
        let NDVI_TIME = components[6]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Could not parse NDVI_TIME from {}", line));

        let mut NDWI = NODATAVAL;
        let mut NDWI_TIME = 0.0;

        if components.len() > 7 {
            NDWI = components[7]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse NDWI from {}", line));
            NDWI_TIME = components[8]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse NDWI_TIME from {}", line));
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

    let current_date = current_date - Duration::try_days(1).expect("Should be valid");
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
