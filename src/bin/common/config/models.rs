use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
};

use std::f32::consts::PI;
use std::fs;
use std::sync::Arc;

use chrono::*;
use chrono::{DateTime, Utc};
use log::{info, warn};
use rayon::prelude::*;
use risico::{
    constants::NODATAVAL,
    models::output::Output,
    modules::angstrom::models::{
        AngstromCellPropertiesContainer, AngstromProperties, AngstromState,
    },
    modules::fosberg::models::{FosbergCellPropertiesContainer, FosbergProperties, FosbergState},
    modules::fwi::{
        config::FWIModelConfig,
        models::{FWICellPropertiesContainer, FWIProperties, FWIState, FWIWarmState},
    },
    //    modules::portuguese::models::{PortugueseCellPropertiesContainer, PortugueseProperties, PortugueseState, PortugueseWarmState},
    modules::hdw::models::{HdwCellPropertiesContainer, HdwProperties, HdwState},
    modules::kbdi::{
        config::KBDIModelConfig,
        models::{KBDICellPropertiesContainer, KBDIProperties, KBDIState, KBDIWarmState},
    },
    modules::mark5::{
        config::Mark5ModelConfig,
        models::{Mark5CellPropertiesContainer, Mark5Properties, Mark5State, Mark5WarmState},
    },
    modules::nesterov::models::{
        NesterovCellPropertiesContainer, NesterovProperties, NesterovState, NesterovWarmState,
    },
    modules::orieux::models::{
        OrieuxCellPropertiesContainer, OrieuxProperties, OrieuxState, OrieuxWarmState,
    },
    modules::risico::{
        config::RISICOModelConfig,
        models::{
            RISICOCellPropertiesContainer, RISICOProperties, RISICOState, RISICOVegetation,
            RISICOWarmState,
        },
    },
    modules::sharples::models::{
        SharplesCellPropertiesContainer, SharplesProperties, SharplesState,
    },
};

use super::builder::{
    AngstromConfigBuilder,
    FWIConfigBuilder,
    FosbergConfigBuilder,
    //    PortugueseConfigBuilder,
    HdwConfigBuilder,
    KbdiConfigBuilder,
    Mark5ConfigBuilder,
    NesterovConfigBuilder,
    OrieuxConfigBuilder,
    OutputTypeConfig,
    RISICOConfigBuilder,
    SharplesConfigBuilder,
};

use crate::common::helpers::RISICOError;
use crate::common::io::models::{output::OutputType, palette::Palette};

pub type PaletteMap = HashMap<String, Box<Palette>>;
// pub type ConfigMap = HashMap<String, Vec<String>>;

pub fn check_write_warm_state(time: &DateTime<Utc>, warm_state_hour: i64) -> bool {
    time.hour() as i64 == warm_state_hour
}

pub fn find_warm_state(
    base_warm_file: &str,
    run_date: DateTime<Utc>,
    hour: i64,
) -> (Option<File>, DateTime<Utc>) {
    // for the last n days before date, try to read the warm state
    // compose the filename as base_warm_file_YYYYmmDDHHMM
    let mut current_date = run_date;
    let mut file: Option<File> = None;
    for days_before in 1..4 {
        current_date = run_date - Duration::try_days(days_before).expect("Should be valid");
        // add the time to the warm state time
        current_date += Duration::try_hours(hour).expect("Should be valid");
        let filename = format!("{}{}", base_warm_file, current_date.format("%Y%m%d%H%M"));
        let file_handle = File::open(filename);
        if file_handle.is_err() {
            continue;
        }
        file = Some(file_handle.expect("Should unwrap"));
        break;
    }
    (file, current_date)
}

pub struct RISICOConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<RISICOWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: RISICOProperties,
    palettes: PaletteMap,
    // use_temperature_effect: bool,
    // use_ndvi: bool,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct FWIConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<FWIWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: FWIProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct Mark5Config {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<Mark5WarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: Mark5Properties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct KbdiConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<KBDIWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: KBDIProperties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct AngstromConfig {
    run_date: DateTime<Utc>,
    properties: AngstromProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct FosbergConfig {
    run_date: DateTime<Utc>,
    properties: FosbergProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct NesterovConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<NesterovWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: NesterovProperties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct SharplesConfig {
    run_date: DateTime<Utc>,
    properties: SharplesProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct OrieuxConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<OrieuxWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: OrieuxProperties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
}

// pub struct PortugueseConfig {
//     run_date: DateTime<Utc>,
//     warm_state_path: String,
//     warm_state: Vec<PortugueseWarmState>,
//     warm_state_time: DateTime<Utc>,
//     warm_state_hour: i64,
//     properties: PortugueseProperties,
//     palettes: PaletteMap,
//     output_types_defs: Vec<OutputTypeConfig>,
// }

pub struct HdwConfig {
    run_date: DateTime<Utc>,
    properties: HdwProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
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

pub fn load_palettes(palettes_defs: &HashMap<String, String>) -> HashMap<String, Box<Palette>> {
    let mut palettes: HashMap<String, Box<Palette>> = HashMap::new();

    for (name, path) in palettes_defs.iter() {
        if let Ok(palette) = Palette::load_palette(path) {
            palettes.insert(name.to_string(), Box::new(palette));
        }
    }
    palettes
}

impl RISICOConfig {
    pub fn new(
        config_defs: &RISICOConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<RISICOConfig, RISICOError> {
        let palettes = load_palettes(palettes);

        let cells_file = &config_defs.cells_file_path;

        let props_container = RISICOConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len()
            || n_cells != props_container.slopes.len()
            || n_cells != props_container.aspects.len()
            || n_cells != props_container.vegetations.len()
        {
            panic!("All properties must have the same length");
        }

        let vegetations_dict = RISICOConfig::read_vegetation(&config_defs.vegetation_file)
            .map_err(|error| format!("error reading {}, {error}", &config_defs.vegetation_file))?;

        let warm_state_hour = config_defs.warm_state_hour;

        let (warm_state, warm_state_time) =
            RISICOConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
                .unwrap_or((
                    vec![RISICOWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ));

        let ppf_file = &config_defs.ppf_file;
        let ppf = match ppf_file {
            Some(ppf_file) => RISICOConfig::read_ppf(ppf_file)
                .map_err(|error| format!("error reading {}, {}", &ppf_file, error))?,
            None => vec![(1.0, 1.0); n_cells],
        };
        let ppf_summer = ppf.iter().map(|(s, _)| *s).collect();
        let ppf_winter = ppf.iter().map(|(_, w)| *w).collect();

        let props =
            RISICOProperties::new(props_container, vegetations_dict, ppf_summer, ppf_winter);

        let config = RISICOConfig {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            // use_temperature_effect: config_defs.use_temperature_effect,
            // use_ndvi: config_defs.use_ndvi,
            output_time_resolution: config_defs.output_time_resolution,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    /// Read the cells from a file.
    /// :param file_path: The path to the file.
    /// :return: A list of cells.
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<RISICOCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;

        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut slopes: Vec<f32> = Vec::new();
        let mut aspects: Vec<f32> = Vec::new();
        let mut vegetations: Vec<String> = Vec::new();

        let reader = BufReader::new(file);

        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }

            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();

            if line_parts.len() < 5 {
                let error_message = format!(
                    "Invalid line in file {file_path}: 
                expected 5 elements, found {} in line #{index}:
                {line}",
                    line_parts.len()
                );
                return Err(error_message.into());
            }

            //  [TODO] refactor this for using error handling
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let slope = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `slope` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let aspect = line_parts[3].parse::<f32>().map_err(|_| {
                format!("Invalid `aspect` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let vegetation = line_parts[4].to_string();

            let slope = slope * PI / 180.0;
            let aspect = aspect * PI / 180.0;

            lons.push(lon);
            lats.push(lat);
            slopes.push(slope);
            aspects.push(aspect);
            vegetations.push(vegetation);
        }

        let props = RISICOCellPropertiesContainer {
            lats,
            lons,
            slopes,
            aspects,
            vegetations,
        };
        Ok(props)
    }

    /// Read the cells from a file.
    /// :param file_path: The path to the file.
    /// :return: A list of cells.
    pub fn read_vegetation(
        file_path: &str,
    ) -> Result<HashMap<String, Arc<RISICOVegetation>>, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("Can't open file: {err}."))?;
        let mut vegetations: HashMap<String, Arc<RISICOVegetation>> = HashMap::new();

        let reader = BufReader::new(file);

        for (index, line) in reader.lines().enumerate() {
            let line =
                line.map_err(|err| format!("Error reading {file_path} at line #{index}: {err}"))?;
            if index == 0 && line.starts_with("#") || line.is_empty() {
                // skip header and empty lines
                continue;
            }
            let line_elements: Vec<&str> = line.split_whitespace().collect::<Vec<&str>>();

            let n_elements = line_elements.len();
            if n_elements < 9 {
                return Err(format!("Invalid line in file {file_path}: {line}").into());
            }

            //  [TODO] refactor this for using error handling
            let id = line_elements[0].to_string();
            let d0 = line_elements[1].parse::<f32>().map_err(|_| {
                format!("Invalid `d0` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let d1 = line_elements[2].parse::<f32>().map_err(|_| {
                format!("Invalid `d1` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let hhv = line_elements[3].parse::<f32>().map_err(|_| {
                format!("Invalid `hhv` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let umid = line_elements[4].parse::<f32>().map_err(|_| {
                format!("Invalid `umid` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let v0 = line_elements[5].parse::<f32>().map_err(|_| {
                format!("Invalid `v0` value in file {file_path} at line #{index}: '{line}'")
            })?;
            #[allow(non_snake_case)]
            let T0 = line_elements[6].parse::<f32>().map_err(|_| {
                format!("Invalid `T0` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let sat = line_elements[7].parse::<f32>().map_err(|_| {
                format!("Invalid `sat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let use_ndvi = match n_elements {
                10.. => line_elements[8].parse::<bool>().map_err(|_| {
                    format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
                })?,
                _ => false,
            };
            let name = line_elements[n_elements - 1].to_string();

            let veg_id = id.clone();

            let veg = Arc::new(RISICOVegetation {
                id,
                d0,
                d1,
                hhv,
                umid,
                v0,
                T0,
                sat,
                name,
                use_ndvi,
            });

            vegetations.insert(veg_id, veg);
        }

        Result::Ok(vegetations)
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
            let ppf_summer = components[0].parse::<f32>().map_err(|err| {
                format!("Could not parse value from PPF file {}: {}", ppf_file, err)
            })?;

            let ppf_winter = components[1].parse::<f32>().map_err(|err| {
                format!("Could not parse value from PPF file {}: {}", ppf_file, err)
            })?;
            ppf.push((ppf_summer, ppf_winter));
        }
        Ok(ppf)
    }

    pub fn get_properties(&self) -> &RISICOProperties {
        &self.properties
    }

    pub fn new_state(&self) -> RISICOState {
        log::info!("Model version: {}", &self.model_version);
        let config = RISICOModelConfig::new(&self.model_version);
        RISICOState::new(&self.warm_state, &self.warm_state_time, config)
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

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    /// The warm state is stored in a text file with the following structure:
    /// dffm
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
    ) -> Option<(Vec<RISICOWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );

        let mut warm_state: Vec<RISICOWarmState> = Vec::new();

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

            warm_state.push(RISICOWarmState {
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

        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &RISICOState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
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

impl FWIConfig {
    pub fn new(
        config_defs: &FWIConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<FWIConfig, RISICOError> {
        let palettes = load_palettes(palettes);

        let cells_file = &config_defs.cells_file_path;

        let props_container = FWIConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }

        let warm_state_hour = config_defs.warm_state_hour;

        let (warm_state, warm_state_time) =
            FWIConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
                .unwrap_or((
                    vec![FWIWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ));

        let props = FWIProperties::new(props_container);

        let config = FWIConfig {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<FWICellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;

        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();

        let reader = BufReader::new(file);

        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }

            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();

            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }

            //  [TODO] refactor this for using error handling
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }

        let props = FWICellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_properties(&self) -> &FWIProperties {
        &self.properties
    }

    pub fn new_state(&self) -> FWIState {
        log::info!("Model version: {}", &self.model_version);
        let config = FWIModelConfig::new(&self.model_version);
        FWIState::new(&self.warm_state, &self.warm_state_time, config)
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

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    /// The warm state is stored in a text file with the following structure:
    /// dffm
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
    ) -> Option<(Vec<FWIWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<FWIWarmState> = Vec::new();

        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");

            let components: Vec<&str> = line.split_whitespace().collect();
            let dates = components[0]
                .split(",")
                .map(|date| {
                    NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
                        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
                        .unwrap_or_else(|_| panic!("Could not parse date from {}", date))
                })
                .collect();
            let ffmc = components[1]
                .split(",")
                .map(|ffmc| {
                    ffmc.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse FFMC value from {}", ffmc))
                })
                .collect();
            let dmc = components[2]
                .split(",")
                .map(|dmc| {
                    dmc.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse DMC value from {}", dmc))
                })
                .collect();
            let dc = components[3]
                .split(",")
                .map(|dc| {
                    dc.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse DC value from {}", dc))
                })
                .collect();
            let rain = components[4]
                .split(",")
                .map(|rain| {
                    rain.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse rain value from {}", rain))
                })
                .collect();

            warm_state.push(FWIWarmState {
                dates,
                ffmc,
                dmc,
                dc,
                rain,
            });
        }

        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &FWIState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;

        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);

        for state in &state.data {
            let dates = state.dates.clone();
            let ffmc = state.ffmc.clone();
            let dmc = state.dmc.clone();
            let dc = state.dc.clone();
            let rain = state.rain.clone();

            let line = format!(
                "{}\t{}\t{}\t{}\t{}",
                dates
                    .iter()
                    .map(|value| format!("{}", value.format("%Y%m%d%H%M")))
                    .collect::<Vec<String>>()
                    .join(","),
                ffmc.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                dmc.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                dc.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                rain.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(",")
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl Mark5Config {
    // New Mark5 configuration
    pub fn new(
        config_defs: &Mark5ConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<Mark5Config, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = Mark5Config::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            return Err(format!("All properties must have the same length").into());
        }
        let warm_state_hour = config_defs.warm_state_hour;
        let (warm_state, warm_state_time) =
            Mark5Config::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
                .unwrap_or((
                    vec![Mark5WarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ));
        let props = Mark5Properties::new(props_container);
        let config = Mark5Config {
            run_date: date,
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read the cells from a file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<Mark5CellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("Can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut mean_rains: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("Can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 3 {
                let error_message = format!(
                    "
                    Invalid line in file {file_path} at #{index}:
                    Expected 3 elements, found {} in line: 
                {line}",
                    line_parts.len()
                );
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let mean_rain = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `mean_rain` value in file {file_path} at line #{index}: '{line}'")
            })?;
            lons.push(lon);
            lats.push(lat);
            mean_rains.push(mean_rain);
        }
        let props = Mark5CellPropertiesContainer {
            lats,
            lons,
            mean_rains,
        };
        Ok(props)
    }

    pub fn get_properties(&self) -> &Mark5Properties {
        &self.properties
    }

    pub fn new_state(&self) -> Mark5State {
        log::info!("Model version: {}", &self.model_version);
        let config = Mark5ModelConfig::new(&self.model_version);
        Mark5State::new(&self.warm_state, &self.warm_state_time, config)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
    ) -> Option<(Vec<Mark5WarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<Mark5WarmState> = Vec::new();
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let dates = components[0]
                .split(",")
                .map(|date| {
                    NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
                        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
                        .unwrap_or_else(|_| panic!("Could not parse date from {}", date))
                })
                .collect();
            let daily_rain = components[1]
                .split(",")
                .map(|rain| {
                    rain.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse FFMC value from {}", rain))
                })
                .collect();
            let smd = components[2]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(Mark5WarmState {
                dates,
                daily_rain,
                smd,
            });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &Mark5State,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let dates = state.dates.clone();
            let daily_rain = state.daily_rain.clone();
            let smd = state.smd;
            let line = format!(
                "{}\t{}\t{}",
                dates
                    .iter()
                    .map(|value| format!("{}", value.format("%Y%m%d%H%M")))
                    .collect::<Vec<String>>()
                    .join(","),
                daily_rain
                    .iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                smd
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl KbdiConfig {
    // Keetch-Byram Drought Index configuration
    pub fn new(
        config_defs: &KbdiConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<KbdiConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = KbdiConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            return Err(format!("All properties must have the same length").into());
        }
        let warm_state_hour = config_defs.warm_state_hour;
        let (warm_state, warm_state_time) =
            KbdiConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
                .unwrap_or((
                    vec![KBDIWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ));
        let props = KBDIProperties::new(props_container);
        let config = KbdiConfig {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Reads the properties from a file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<KBDICellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut mean_rains: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 3 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let mean_rain = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `mean_rain` value in file {file_path} at line #{index}: '{line}'")
            })?;
            lons.push(lon);
            lats.push(lat);
            mean_rains.push(mean_rain);
        }

        let props = KBDICellPropertiesContainer {
            lats,
            lons,
            mean_rains,
        };
        Ok(props)
    }

    pub fn get_properties(&self) -> &KBDIProperties {
        &self.properties
    }

    pub fn new_state(&self) -> KBDIState {
        log::info!("Model version: {}", &self.model_version);
        let config = KBDIModelConfig::new(&self.model_version);
        KBDIState::new(&self.warm_state, &self.warm_state_time, config)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
    ) -> Option<(Vec<KBDIWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<KBDIWarmState> = Vec::new();
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let dates = components[0]
                .split(",")
                .map(|date| {
                    NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
                        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
                        .unwrap_or_else(|_| panic!("Could not parse date from {}", date))
                })
                .collect();
            let daily_rain = components[1]
                .split(",")
                .map(|rain| {
                    rain.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse FFMC value from {}", rain))
                })
                .collect();
            let kbdi = components[2]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(KBDIWarmState {
                dates,
                daily_rain,
                kbdi,
            });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &KBDIState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let dates = state.dates.clone();
            let daily_rain = state.daily_rain.clone();
            let kbdi = state.kbdi;
            let line = format!(
                "{}\t{}\t{}",
                dates
                    .iter()
                    .map(|value| format!("{}", value.format("%Y%m%d%H%M")))
                    .collect::<Vec<String>>()
                    .join(","),
                daily_rain
                    .iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                kbdi
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl AngstromConfig {
    // New Angstrom index configuration
    pub fn new(
        config_defs: &AngstromConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<AngstromConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = AngstromConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = AngstromProperties::new(props_container);
        let config = AngstromConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<AngstromCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;
            lons.push(lon);
            lats.push(lat);
        }
        let props = AngstromCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_properties(&self) -> &AngstromProperties {
        &self.properties
    }

    pub fn new_state(&self) -> AngstromState {
        AngstromState::new(&self.run_date, self.properties.len)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        // the Angstrom index is computed every 24 hours (once a day)
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}

impl FosbergConfig {
    // New Fosberg index configuration
    pub fn new(
        config_defs: &FosbergConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<FosbergConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = FosbergConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = FosbergProperties::new(props_container);
        let config = FosbergConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<FosbergCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }
        let props = FosbergCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_properties(&self) -> &FosbergProperties {
        &self.properties
    }

    pub fn new_state(&self) -> FosbergState {
        FosbergState::new(&self.run_date, self.properties.len)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}

impl NesterovConfig {
    // New Nesterov index configuration
    pub fn new(
        config_defs: &NesterovConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<NesterovConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = NesterovConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let warm_state_hour = config_defs.warm_state_hour;
        let (warm_state, warm_state_time) =
            NesterovConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
                .unwrap_or((
                    vec![NesterovWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ));
        let props = NesterovProperties::new(props_container);
        let config = NesterovConfig {
            run_date: date,
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<NesterovCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }

        let props = NesterovCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_properties(&self) -> &NesterovProperties {
        &self.properties
    }

    pub fn new_state(&self) -> NesterovState {
        NesterovState::new(&self.warm_state, &self.warm_state_time)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
    ) -> Option<(Vec<NesterovWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<NesterovWarmState> = Vec::new();

        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let nesterov = components[0]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(NesterovWarmState { nesterov });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &NesterovState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let nesterov = state.nesterov;
            let line = format!("{}", nesterov);
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl SharplesConfig {
    // New Sharples index configuration
    pub fn new(
        config_defs: &SharplesConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<SharplesConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = SharplesConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = SharplesProperties::new(props_container);
        let config = SharplesConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<SharplesCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }
        let props = SharplesCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_properties(&self) -> &SharplesProperties {
        &self.properties
    }

    pub fn new_state(&self) -> SharplesState {
        SharplesState::new(&self.run_date, self.properties.len)
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
}

impl OrieuxConfig {
    // New Orieux index configuration
    pub fn new(
        config_defs: &OrieuxConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<OrieuxConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = OrieuxConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let warm_state_hour = config_defs.warm_state_hour;
        let (warm_state, warm_state_time) =
            OrieuxConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
                .unwrap_or((
                    vec![OrieuxWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ));
        let props = OrieuxProperties::new(props_container);
        let config = OrieuxConfig {
            run_date: date,
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<OrieuxCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut heat_indices: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 3 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let hindex = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `hindex` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
            heat_indices.push(hindex);
        }
        let props = OrieuxCellPropertiesContainer {
            lats,
            lons,
            heat_indices,
        };
        Ok(props)
    }

    pub fn get_properties(&self) -> &OrieuxProperties {
        &self.properties
    }

    pub fn new_state(&self) -> OrieuxState {
        OrieuxState::new(&self.warm_state, &self.warm_state_time)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
    ) -> Option<(Vec<OrieuxWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<OrieuxWarmState> = Vec::new();
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let orieux_wr = components[0]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(OrieuxWarmState { orieux_wr });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &OrieuxState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let orieux_wr = state.orieux_wr;
            let line = format!("{}", orieux_wr);
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

// UNDER CONSTRUCTION
//impl PortugueseConfig {
//
//    // New Portuguese index configuration
//    pub fn new(
//        config_defs: &PortugueseConfigBuilder,
//        date: DateTime<Utc>,
//        palettes: &HashMap<String, String>,
//    ) -> Result<PortugueseConfig, RISICOError> {
//        let palettes = load_palettes(palettes);
//        let cells_file = &config_defs.cells_file_path;
//        let props_container = PortugueseConfig::properties_from_file(cells_file)
//            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
//        let n_cells = props_container.lons.len();
//        if n_cells != props_container.lats.len()
//        {
//            panic!("All properties must have the same length");
//        }
//        let warm_state_hour = if config_defs.warm_state_hour > 0 {
//            config_defs.warm_state_hour.clone()
//        } else {
//            24
//        };
//        let (warm_state, warm_state_time) = PortugueseConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
//            .unwrap_or((
//                vec![PortugueseWarmState::default(); n_cells],
//                date - Duration::try_days(1).expect("Should be a valid duration"),
//            ));
//        let props = PortugueseProperties::new(props_container);
//        let config = PortugueseConfig {
//            run_date: date,
//            warm_state_path: config_defs.warm_state_path.clone(),
//            warm_state,
//            warm_state_time,
//            warm_state_hour: warm_state_hour,
//            properties: props,
//            palettes,
//            output_types_defs: config_defs.output_types.clone(),
//        };
//        Ok(config)
//    }
//
//    // Read properties from file
//    pub fn properties_from_file(file_path: &str) -> Result<PortugueseCellPropertiesContainer, RISICOError> {
//        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
//        let mut lons: Vec<f32> = Vec::new();
//        let mut lats: Vec<f32> = Vec::new();
//        let reader = BufReader::new(file);
//        for line in reader.lines() {
//            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
//            if line.starts_with("#") {
//                // skip header
//                continue;
//            }
//            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
//            if line_parts.len() < 2 {
//                let error_message = format!("Invalid line in file: {}", line);
//                return Err(error_message.into());
//            }
//            let lon = line_parts[0]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
//            let lat = line_parts[1]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
//            lons.push(lon);
//            lats.push(lat);
//        }
//        let props = PortugueseCellPropertiesContainer {
//            lats,
//            lons,
//        };
//        Ok(props)
//    }
//
//    pub fn get_properties(&self) -> &PortugueseProperties {
//        &self.properties
//    }
//
//    pub fn new_state(&self) -> PortugueseState {
//        PortugueseState::new(&self.warm_state, &self.warm_state_time)
//    }
//
//    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
//        Ok(OutputWriter::new(
//            self.output_types_defs.as_slice(),
//            &self.run_date,
//            &self.palettes,
//        ))
//    }
//
//    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> (bool, DateTime<Utc>) {
//        let time_diff = time.signed_duration_since(self.run_date);
//        let minutes = time_diff.num_minutes();
//        // Approximation to the closest hour
//        let approximate_hours = if minutes % 60 >= 30 {
//            (minutes / 60) + 1
//        } else {
//            minutes / 60
//        };
//        let warm_state_time = self.run_date + Duration::try_hours(approximate_hours).expect("Should be valid");
//        let should_write= (approximate_hours % self.warm_state_hour == 0) && (approximate_hours > 0);
//        (should_write, warm_state_time)
//    }
//
//    #[allow(non_snake_case)]
//    /// Reads the warm state from the file
//    /// The warm state is stored in a file with the following structure:
//    /// base_warm_file_YYYYmmDDHHMM
//    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
//    pub fn read_warm_state(
//        base_warm_file: &str,
//        run_date: DateTime<Utc>,
//        offset: &i64,
//    ) -> Option<(Vec<PortugueseWarmState>, DateTime<Utc>)> {
//        // for the last n days before date, try to read the warm state
//        // compose the filename as base_warm_file_YYYYmmDDHHMM
//        let mut file: Option<File> = None;
//        let mut current_date = run_date;
//        for days_before in 1..4 {
//            current_date = run_date - Duration::try_days(days_before).expect("Should be valid");
//            // add the offset to the current date
//            current_date = current_date + Duration::try_hours(*offset).expect("Should be valid");
//            let filename = format!("{}{}", base_warm_file, current_date.format("%Y%m%d%H%M"));
//            let file_handle = File::open(filename);
//            if file_handle.is_err() {
//                continue;
//            }
//            file = Some(file_handle.expect("Should unwrap"));
//            break;
//        }
//        let file = match file {
//            Some(file) => file,
//            None => {
//                warn!(
//                    "WARNING: Could not find a valid warm state file for run date {}",
//                    run_date.format("%Y-%m-%d")
//                );
//                return None;
//            }
//        };
//
//        info!(
//            "Loading warm state from {}",
//            current_date.format("%Y-%m-%d %H:%M")
//        );
//        let mut warm_state: Vec<PortugueseWarmState> = Vec::new();
//        let reader = io::BufReader::new(file);
//        for line in reader.lines() {
//            if let Err(line) = line {
//                warn!("Error reading warm state file: {}", line);
//                return None;
//            }
//            let line = line.expect("Should unwrap line");
//            let components: Vec<&str> = line.split_whitespace().collect();
//            let sum_ign = components[0]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
//            let cum_index = components[1]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
//            warm_state.push(PortugueseWarmState {
//                sum_ign,
//                cum_index
//            });
//        }
//        Some((warm_state, current_date))
//    }
//
//    #[allow(non_snake_case)]
//    pub fn write_warm_state(&self, state: &PortugueseState, warm_state_time: DateTime<Utc>) -> Result<(), RISICOError> {
//        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
//        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
//        let mut warm_state_file = File::create(&warm_state_name)
//            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
//        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
//        for state in &state.data {
//            let sum_ign = state.sum_ign.clone();
//            let cum_index = state.cum_index.clone();
//            let line = format!(
//                "{}\t{}",
//                sum_ign,
//                cum_index
//            );
//            writeln!(warm_state_writer, "{}", line)
//                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
//        }
//        Ok(())
//    }
//}

impl HdwConfig {
    // New Hot-dry-wind index configuration
    pub fn new(
        config_defs: &HdwConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<HdwConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = HdwConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = HdwProperties::new(props_container);
        let config = HdwConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<HdwCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }
        let props = HdwCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_properties(&self) -> &HdwProperties {
        &self.properties
    }

    pub fn new_state(&self) -> HdwState {
        HdwState::new(&self.run_date, self.properties.len)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}
