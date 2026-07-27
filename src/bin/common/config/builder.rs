use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::Read;
use std::io;

use crate::common::helpers::RISICOError;
use crate::common::io::models::output::OutputVariable;
use crate::common::io::readers::netcdf::NetCdfInputConfiguration;

use super::models::{
    AngstromConfig,
    FWIConfig,
    FosbergConfig,
    //    PortugueseConfig,
    HdwConfig,
    KbdiConfig,
    Mark5Config,
    NesterovConfig,
    OrieuxConfig,
    RISICOConfig,
    SharplesConfig,
};

pub type PaletteMap = HashMap<String, String>;
pub type ConfigMap = HashMap<String, Vec<String>>;

/// Parses the legacy `KEY=VALUE` grid-definition and cell-order text files
/// still used by models without a GeoTIFF/NetCDF static-data equivalent, and
/// by the offline `static-converter` migration tool.
pub fn read_config(file_name: impl Into<String>) -> Result<ConfigMap, RISICOError> {
    let file_name = file_name.into();
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
#[serde(tag = "type")]
pub enum StaticDataConfig {
    #[serde(rename = "geotiff")]
    GeoTiff {
        domain_mask: String,
        slope: Option<String>,
        aspect: Option<String>,
        vegetation_id: Option<String>,
        vegetation_catalog: Option<String>,
        ppf_summer: Option<String>,
        ppf_winter: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissingWarmStatePolicy {
    #[default]
    Error,
    Defaults,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WarmStateConfig {
    #[serde(rename = "netcdf")]
    NetCdf {
        directory: String,
        max_age_hours: Option<i64>,
        #[serde(default)]
        on_missing: MissingWarmStatePolicy,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RISICOConfigBuilder {
    pub model_name: String,
    pub static_data: StaticDataConfig,
    pub warm_state: WarmStateConfig,
    pub warm_state_hour: Option<i64>,
    pub warm_state_lag_days: Option<i64>,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: u32,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FWIConfigBuilder {
    pub model_name: String,
    pub static_data: StaticDataConfig,
    pub warm_state: WarmStateConfig,
    pub warm_state_hour: Option<i64>,
    pub warm_state_lag_days: Option<i64>,
    pub output_types: Vec<OutputTypeConfig>,
    pub output_time_resolution: Option<u32>,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mark5ConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_hour: Option<i64>,
    pub warm_state_lag_days: Option<i64>,
    pub output_types: Vec<OutputTypeConfig>,
    pub model_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KbdiConfigBuilder {
    pub model_name: String,
    pub cells_file_path: String,
    pub warm_state_path: String,
    pub warm_state_hour: Option<i64>,
    pub warm_state_lag_days: Option<i64>,
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
    pub warm_state_hour: Option<i64>,
    pub warm_state_lag_days: Option<i64>,
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
    pub warm_state_hour: Option<i64>,
    pub warm_state_lag_days: Option<i64>,
    pub output_types: Vec<OutputTypeConfig>,
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct PortugueseConfigBuilder {
//     pub model_name: String,
//     pub cells_file_path: String,
//     pub warm_state_path: String,
//     pub warm_state_hour: i64,
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
    RISICO(Box<RISICOConfigBuilder>),
    FWI(Box<FWIConfigBuilder>),
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

fn default_tile_height() -> usize {
    512
}

fn default_tile_width() -> usize {
    512
}

fn default_cells_per_tile() -> usize {
    1_048_576
}

/// Model-independent controls for bounded-memory execution.
///
/// Raster-backed models use two-dimensional windows. Legacy point models use
/// batches in their configured cell order but enter the same tile runner.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamingExecutionConfig {
    #[serde(default = "default_tile_height")]
    pub tile_height: usize,
    #[serde(default = "default_tile_width")]
    pub tile_width: usize,
    #[serde(default = "default_cells_per_tile")]
    pub cells_per_tile: usize,
    /// How many tiles may be processed at once.
    ///
    /// Memory in flight scales with this times the tile size, so it is the
    /// dial to turn when a run has to fit a smaller budget. Defaults to one
    /// tile per available core.
    pub tile_concurrency: Option<usize>,
    /// Target peak resident size, as a human size such as `6GB` or `768 MiB`.
    ///
    /// Tile concurrency is derived from what is left of this budget once the
    /// whole-domain arrays are accounted for, so the run adapts to the model
    /// and the domain instead of needing a hand-tuned thread count. This is a
    /// target rather than a limit: nothing enforces it against the allocator,
    /// and a budget below the domain's fixed cost still runs, one tile at a
    /// time. An explicit `tile_concurrency` wins over this.
    pub max_memory: Option<String>,
    pub scratch_directory: Option<String>,
}

impl Default for StreamingExecutionConfig {
    fn default() -> Self {
        Self {
            tile_height: default_tile_height(),
            tile_width: default_tile_width(),
            cells_per_tile: default_cells_per_tile(),
            tile_concurrency: None,
            max_memory: None,
            scratch_directory: None,
        }
    }
}

impl StreamingExecutionConfig {
    pub fn validate(&self) -> Result<(), RISICOError> {
        if self.tile_height == 0 || self.tile_width == 0 {
            return Err("streaming tile dimensions must be greater than zero".into());
        }
        if self.cells_per_tile == 0 {
            return Err("streaming cells_per_tile must be greater than zero".into());
        }
        if self.tile_concurrency == Some(0) {
            return Err("streaming tile_concurrency must be greater than zero".into());
        }
        self.max_memory_bytes()?;
        Ok(())
    }

    /// The configured budget in bytes, if one was given.
    pub fn max_memory_bytes(&self) -> Result<Option<u64>, RISICOError> {
        match &self.max_memory {
            None => Ok(None),
            Some(budget) => parse_byte_size(budget).map(Some),
        }
    }
}

/// Parse a human byte size such as `512MB`, `6 GiB` or a bare byte count.
///
/// Both the decimal (`kB`, `MB`, `GB`) and binary (`KiB`, `MiB`, `GiB`) scales
/// are accepted because configuration is written by people who mean either.
fn parse_byte_size(text: &str) -> Result<u64, RISICOError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("streaming max_memory must not be empty".into());
    }

    let split = trimmed
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let amount: f64 = number
        .parse()
        .map_err(|_| format!("streaming max_memory has no leading number: {trimmed}"))?;
    if !amount.is_finite() || amount <= 0.0 {
        return Err(format!("streaming max_memory must be positive: {trimmed}").into());
    }

    let multiplier: u64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1_000,
        "m" | "mb" => 1_000_000,
        "g" | "gb" => 1_000_000_000,
        "t" | "tb" => 1_000_000_000_000,
        "ki" | "kib" => 1 << 10,
        "mi" | "mib" => 1 << 20,
        "gi" | "gib" => 1 << 30,
        "ti" | "tib" => 1u64 << 40,
        other => {
            return Err(format!("streaming max_memory has an unknown unit: {other}").into());
        }
    };

    Ok((amount * multiplier as f64) as u64)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigContainer {
    pub models: Vec<ConfigBuilderType>,
    pub palettes: PaletteMap,
    pub netcdf_input_configuration: Option<NetCdfInputConfiguration>,
    #[serde(default)]
    pub streaming: StreamingExecutionConfig,
}

impl ConfigContainer {
    pub fn from_file(config_file: &str) -> Result<ConfigContainer, RISICOError> {
        if config_file.ends_with(".yaml") || config_file.ends_with(".yml") {
            Self::from_yaml(config_file)
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

        let conf: Self = serde_yaml::from_str(&contents)
            .map_err(|err| format!("Cannot parse config file {}: {}", config_file, err))?;
        conf.streaming.validate()?;
        Ok(conf)
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

impl Mark5ConfigBuilder {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geotiff_and_netcdf_configuration_is_deserializable() {
        let yaml = r#"
model_name: RISICO2023
model_version: v2025
static_data:
  type: geotiff
  domain_mask: /data/domain.tif
  slope: /data/slope.tif
  aspect: /data/aspect.tif
  vegetation_id: /data/vegetation.tif
  vegetation_catalog: /data/vegetation.yml
  ppf_summer: null
  ppf_winter: null
warm_state:
  type: netcdf
  directory: /state
  max_age_hours: 120
  on_missing: error
output_types: []
output_time_resolution: 1
"#;
        let config: RISICOConfigBuilder =
            serde_yaml::from_str(yaml).expect("configuration should parse");
        assert!(matches!(config.static_data, StaticDataConfig::GeoTiff { .. }));
        assert!(matches!(config.warm_state, WarmStateConfig::NetCdf { .. }));
    }

    #[test]
    fn fwi_geotiff_and_netcdf_configuration_is_deserializable() {
        let yaml = r#"
model_name: FWIWORLD
model_version: legacy
static_data:
  type: geotiff
  domain_mask: /data/domain_mask.tif
warm_state:
  type: netcdf
  directory: /state/netcdf
  max_age_hours: 120
  on_missing: error
output_types: []
"#;
        let config: FWIConfigBuilder =
            serde_yaml::from_str(yaml).expect("new FWI configuration should parse");
        assert!(matches!(config.static_data, StaticDataConfig::GeoTiff { .. }));
        assert!(matches!(config.warm_state, WarmStateConfig::NetCdf { .. }));
    }

    #[test]
    fn streaming_execution_defaults_and_overrides_are_validated() {
        let defaults: StreamingExecutionConfig =
            serde_yaml::from_str("{}").expect("empty streaming configuration should use defaults");
        assert_eq!(defaults.tile_height, 512);
        assert_eq!(defaults.tile_width, 512);
        assert_eq!(defaults.cells_per_tile, 1_048_576);
        assert_eq!(defaults.tile_concurrency, None);
        assert_eq!(defaults.max_memory, None);
        assert_eq!(defaults.max_memory_bytes().unwrap(), None);
        defaults.validate().unwrap();

        let configured: StreamingExecutionConfig = serde_yaml::from_str(
            r#"
tile_height: 128
tile_width: 256
cells_per_tile: 10000
tile_concurrency: 4
scratch_directory: /scratch/risico
"#,
        )
        .unwrap();
        assert_eq!(configured.tile_height, 128);
        assert_eq!(configured.tile_width, 256);
        assert_eq!(configured.tile_concurrency, Some(4));
        assert_eq!(
            configured.scratch_directory.as_deref(),
            Some("/scratch/risico")
        );

        let invalid: StreamingExecutionConfig = serde_yaml::from_str("tile_height: 0").unwrap();
        assert!(invalid.validate().is_err());

        let invalid: StreamingExecutionConfig =
            serde_yaml::from_str("tile_concurrency: 0").unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn memory_budgets_accept_both_the_decimal_and_binary_scales() {
        let cases = [
            ("512", 512_u64),
            ("512B", 512),
            ("6GB", 6_000_000_000),
            ("6 GB", 6_000_000_000),
            ("6gb", 6_000_000_000),
            ("768 MiB", 768 << 20),
            ("1.5GB", 1_500_000_000),
            ("8GiB", 8 << 30),
        ];
        for (text, expected) in cases {
            let configured: StreamingExecutionConfig =
                serde_yaml::from_str(&format!("max_memory: \"{text}\"")).unwrap();
            configured.validate().unwrap();
            assert_eq!(
                configured.max_memory_bytes().unwrap(),
                Some(expected),
                "parsing {text}"
            );
        }
    }

    #[test]
    fn malformed_memory_budgets_are_rejected_by_validation() {
        for text in ["", "  ", "lots", "GB", "-4GB", "0GB", "12 parsecs"] {
            let configured: StreamingExecutionConfig =
                serde_yaml::from_str(&format!("max_memory: \"{text}\"")).unwrap();
            assert!(configured.validate().is_err(), "should reject {text:?}");
        }
    }
}
