use chrono::{DateTime, Utc};
use risico::models::output::OutputVariableName;
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::fs::File;
use std::io::BufRead;
use std::io::Read;
use std::str::FromStr;
use std::{collections::HashMap, io};

use crate::common::helpers::RISICOError;
use crate::common::io::models::grid::ClusterMode;
use crate::common::io::models::output::OutputVariable;
use crate::common::io::readers::netcdf::NetCdfInputConfiguration;

use super::models::{
    RISICOConfig,
    FWIConfig,
    Mark5Config,
    KbdiConfig,
    AngstromConfig,
    FosbergConfig,
    NesterovConfig,
    SharplesConfig,
    OrieuxConfig,
//    PortugueseConfig,
    HdwConfig,
};

pub type PaletteMap = HashMap<String, String>;
pub type ConfigMap = HashMap<String, Vec<String>>;

const MODEL_NAME_KEY: &str = "MODELNAME";
const WARM_STATE_PATH_KEY: &str = "STATO0";
const WARM_STATE_OFFSET_KEY: &str = "STATO0_OFFSET";
const CELLS_FILE_KEY: &str = "CELLE";
const VEGETATION_FILE_KEY: &str = "VEG";
const PPF_FILE_KEY: &str = "PPF";
const MODEL_VERSION_KEY: &str = "MODEL_VERSION";
const USE_TEMPERATURE_EFFECT_KEY: &str = "USETCONTR";
const USE_NDVI_KEY: &str = "USENDVI";
const OUTPUTS_KEY: &str = "MODEL";
const NETCDF_INPUT_CONFIG: &str = "NETCDFINPUTCONFIG";
const VARIABLES_KEY: &str = "VARIABLE";
const PALETTE_KEY: &str = "PALETTE";
const KEY_HOURSRESOLUTION: &str = "OUTPUTHRES";

trait ConfigMapExt {
    /// Get the first value of a key in the config map
    fn first(&self, key: &str) -> Option<String>;
    fn all(&self, key: &str) -> Option<Vec<String>>;
}

impl ConfigMapExt for ConfigMap {
    fn first(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|values| values.first().cloned())
    }

    fn all(&self, key: &str) -> Option<Vec<String>> {
        self.get(key).cloned()
    }
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct RISICOConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub vegetation_file: String,
    pub warm_state_path: String,
    pub warm_state_offset: i64,
    pub ppf_file: Option<String>,
    pub output_types: Vec<OutputTypeConfig>,
    pub use_temperature_effect: bool,
    pub use_ndvi: bool,
    pub output_time_resolution: u32,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FWIConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_offset: i64,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: u32,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mark5ConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_offset: i64,
    pub output_types: Vec<OutputTypeConfig>,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KbdiConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_offset: i64,
    pub output_types: Vec<OutputTypeConfig>,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AngstromConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FosbergConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NesterovConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_offset: i64,
    pub output_types: Vec<OutputTypeConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SharplesConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrieuxConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_offset: i64,
    pub output_types: Vec<OutputTypeConfig>,
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct PortugueseConfigBuilder {
//     pub model_name: String,
//     pub cells_file_path: String,
//     pub warm_state_path: String,
//     pub warm_state_offset: i64,
//     pub output_types: Vec<OutputTypeConfig>,
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct HdwConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: u32,
}


#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConfigBuilderType {
    RISICO(RISICOConfigBuilder),
    FWI(FWIConfigBuilder),
    Mark5(Mark5ConfigBuilder),
    KBDI(KbdiConfigBuilder),
    Angstrom(AngstromConfigBuilder),
    Fosberg(FosbergConfigBuilder),
    Nesterov(NesterovConfigBuilder),
    Sharples(SharplesConfigBuilder),
    Orieux(OrieuxConfigBuilder),
//     Portuguese(PortugueseConfigBuilder),
    Hdw(HdwConfigBuilder),
}


impl ConfigBuilderType {
    pub fn get_model_name(&self) -> &str {
        match self {
            ConfigBuilderType::RISICO(_) => "RISICO",
            ConfigBuilderType::FWI(_) => "FWI",
            ConfigBuilderType::Mark5(_) => "Mark5",
            ConfigBuilderType::KBDI(_) => "KBDI",
            ConfigBuilderType::Angstrom(_) => "Angstrom",
            ConfigBuilderType::Fosberg(_) => "Fosberg",
            ConfigBuilderType::Nesterov(_) => "Nesterov",
            ConfigBuilderType::Sharples(_) => "Sharples",
            ConfigBuilderType::Orieux(_) => "Orieux",
//             ConfigBuilderType::Portuguese(_) => "Portuguese",
            ConfigBuilderType::Hdw(_) => "Hdw",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigContainer {
    pub models: Vec<ConfigBuilderType>,
    pub palettes: PaletteMap,
    pub netcdf_input_configuration: Option<NetCdfInputConfiguration>,
}

impl ConfigContainer {
    pub fn from_file(config_file: &str) -> Result<ConfigContainer, RISICOError> {
        // Check the file extension to determine which method to use
        if config_file.ends_with(".yaml") || config_file.ends_with(".yml") {
            Self::from_yaml(config_file)
        } else if config_file.ends_with(".txt") {
            Self::from_txt_file(config_file)
        } else {
            Err(RISICOError::from(format!(
                "Unsupported config file format: {}",
                config_file
            )))
        }
    }

    pub fn from_yaml(config_file: &str) -> Result<Self, RISICOError> {
        let mut file = File::open(config_file)
            .map_err(|err| format!("Cannot open config file {}: {}", config_file, err))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|err| format!("Cannot read config file {}: {}", config_file, err))?;

        let conf = serde_yaml::from_str(&contents)
            .map_err(|err| format!("Cannot parse config file {}: {}", config_file, err))?;
        Ok(conf)
    }

    fn parse_output_types(
        output_types_defs: &Vec<String>,
        variables_defs: &Vec<String>,
    ) -> Result<Vec<OutputTypeConfig>, RISICOError> {
        let mut output_types_vec: Vec<OutputTypeConfig> = Vec::new();

        for out_type_def in output_types_defs {
            let parts = out_type_def.split(":").collect::<Vec<&str>>();
            if parts.len() != 5 {
                return Err("Invalid output definition".into());
            }
            let (internal_name, name, path, grid_path, format) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);

            let output_type = OutputTypeConfig {
                internal_name: internal_name.into(),
                name: name.into(),
                path: path.into(),
                grid_path: grid_path.into(),
                format: format.into(),
                variables: Vec::new(),
            };

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
                    let internal_name = OutputVariableName::from_str(internal_name)
                        .unwrap_or_else(|_| panic!("Invalid Variable Name {}", &internal_name));
                    let cluster_mode = ClusterMode::from_str(cluster_mode)
                        .unwrap_or_else(|_| panic!("Invalid ClusterMode {}", &cluster_mode));
                    _type.variables.push(OutputVariable::new(
                        internal_name,
                        name,
                        cluster_mode,
                        precision,
                    ))
                });
        }

        Ok(output_types_vec)
    }

    fn from_txt_file(config_file: &str) -> Result<ConfigContainer, RISICOError> {
        let config_map = read_config(config_file)?;

        // try to get the model name, expect it to be there
        let model_name = config_map
            .first(MODEL_NAME_KEY)
            .ok_or(format!("Error: {MODEL_NAME_KEY} not found in config"))?;

        let warm_state_path = config_map
            .first(WARM_STATE_PATH_KEY)
            .ok_or(format!("Error: {WARM_STATE_PATH_KEY} not found in config"))?;
        
        // try to get the warm state offset, otherwise default to 0
        let warm_state_offset = match config_map.first(WARM_STATE_OFFSET_KEY) {
            Some(value) => value.parse::<i64>().unwrap_or(0),
            None => 0,
        };

        let cells_file_path = config_map
            .first(CELLS_FILE_KEY)
            .ok_or(format!("Error: {CELLS_FILE_KEY} not found in config"))?;

        let vegetation_file = config_map
            .first(VEGETATION_FILE_KEY)
            .ok_or(format!("Error: {VEGETATION_FILE_KEY} not found in config"))?;

        let model_version = match config_map.first(MODEL_VERSION_KEY) {
            Some(value) => value,
            None => "legacy".to_owned(),
        };

        let ppf_file = config_map.first(PPF_FILE_KEY);

        let use_temperature_effect =
            if let Some(value) = config_map.first(USE_TEMPERATURE_EFFECT_KEY) {
                matches!(value.as_str(), "true" | "True" | "TRUE" | "1")
            } else {
                false
            };

        let use_ndvi = if let Some(value) = config_map.first(USE_NDVI_KEY) {
            matches!(value.as_str(), "true" | "True" | "TRUE" | "1")
        } else {
            false
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

        let palettes = load_palettes(&config_map);
        let output_types = Self::parse_output_types(&output_types_defs, &variables_defs)?;

        let netcdf_input_configuration = config_map
            .first(NETCDF_INPUT_CONFIG)
            .map(|line| NetCdfInputConfiguration::from(&line))
            .or(None);

        let config = RISICOConfigBuilder {
            model_name,
            warm_state_path,
            warm_state_offset,
            cells_file_path,
            vegetation_file,
            ppf_file,
            output_types,

            use_temperature_effect,
            use_ndvi,
            output_time_resolution,
            model_version,
        };

        let config_container = ConfigContainer {
            models: vec![ConfigBuilderType::RISICO(config)],
            palettes,
            netcdf_input_configuration,
        };

        Ok(config_container)
    }

    pub fn get_netcdf_input_config(&self) -> &Option<NetCdfInputConfiguration> {
        &self.netcdf_input_configuration
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutputTypeConfig {
    pub internal_name: String,
    pub name: String,
    pub path: String,
    pub grid_path: String,
    pub format: String,
    pub variables: Vec<OutputVariable>,
}

impl RISICOConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<RISICOConfig, RISICOError> {
        RISICOConfig::new(self, *date, palettes)
    }
}


impl FWIConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<FWIConfig, RISICOError> {
        FWIConfig::new(self, *date, palettes)
    }
}


impl  Mark5ConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<Mark5Config, RISICOError> {
        Mark5Config::new(self, *date, palettes)
    }
    
}

impl KbdiConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<KbdiConfig, RISICOError> {
        KbdiConfig::new(self, *date, palettes)
    }
}

impl AngstromConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<AngstromConfig, RISICOError> {
        AngstromConfig::new(self, *date, palettes)
    }
}

impl FosbergConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<FosbergConfig, RISICOError> {
        FosbergConfig::new(self, *date, palettes)
    }
}

impl NesterovConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<NesterovConfig, RISICOError> {
        NesterovConfig::new(self, *date, palettes)
    }
}

impl SharplesConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<SharplesConfig, RISICOError> {
        SharplesConfig::new(self, *date, palettes)
    }
}

impl OrieuxConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<OrieuxConfig, RISICOError> {
        OrieuxConfig::new(self, *date, palettes)
    }
}

// impl PortugueseConfigBuilder {
//     pub fn build(
//         &self,
//         date: &DateTime<Utc>,
//         palettes: &PaletteMap,
//     ) -> Result<PortugueseConfig, RISICOError> {
//         PortugueseConfig::new(self, *date, palettes)
//     }
// }

impl HdwConfigBuilder {
    pub fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<HdwConfig, RISICOError> {
        HdwConfig::new(self, *date, palettes)
    }
}


pub fn load_palettes(config_map: &ConfigMap) -> HashMap<String, String> {
    let mut palettes: HashMap<String, String> = HashMap::new();
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

        // if let Ok(palette) = Palette::load_palette(path) {
        //     palettes.insert(name.to_string(), Box::new(palette));
        // }

        palettes.insert(name.into(), path.into());
    }
    palettes
}