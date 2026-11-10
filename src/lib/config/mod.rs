//! The `configuration.yml` schema read by the `risico-2023` runner.
//!
//! These are the serde types the runner deserializes, with the checks that
//! need no input data, so that tools (for example a configuration editor) can
//! load, write and validate a configuration exactly as the runner does,
//! without pulling in the runner's I/O stack. Enabled by the `config` feature.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use log::warn;
use serde_derive::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumString};

use crate::models::input::InputVariableName;
use crate::models::output::OutputVariableName;

pub type PaletteMap = HashMap<String, String>;

/// A configuration that cannot be read or is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl ConfigError {
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

impl From<String> for ConfigError {
    fn from(msg: String) -> Self {
        ConfigError(msg)
    }
}

impl From<&str> for ConfigError {
    fn from(msg: &str) -> Self {
        ConfigError(msg.into())
    }
}

/// How native cells falling in one output pixel are combined.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumString, Display, Serialize, Deserialize)]
#[strum(ascii_case_insensitive)]
pub enum ClusterMode {
    Mean,
    Median,
    Min,
    Max,
}

/// One variable of an output type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputVariable {
    /// The model variable to write.
    pub internal_name: OutputVariableName,
    /// Name of the variable in the output files.
    pub name: String,
    /// How the native cells in one output pixel are combined.
    pub cluster_mode: ClusterMode,
    /// Decimal digits kept: values are rounded to a multiple of 10^-precision.
    pub precision: i32,
}

impl OutputVariable {
    pub fn new(
        internal_name: OutputVariableName,
        name: &str,
        cluster_mode: ClusterMode,
        precision: i32,
    ) -> Self {
        Self {
            internal_name,
            name: name.to_string(),
            cluster_mode,
            precision,
        }
    }

    pub fn internal_name(&self) -> OutputVariableName {
        self.internal_name
    }

    pub fn output_name(&self) -> &str {
        &self.name
    }

    pub fn cluster_mode(&self) -> ClusterMode {
        self.cluster_mode
    }

    /// Decimal digits kept: values are rounded to a multiple of 10^-precision.
    pub fn precision(&self) -> i32 {
        self.precision
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariableEntry {
    pub name: String,        // variable name in netcdf file
    pub offset: Option<i64>, // offset in seconds to be aplied in the variable time line
}

impl VariableEntry {
    pub fn new(name: String, offset: Option<i64>) -> Self {
        VariableEntry { name, offset }
    }
}

// Define a helper struct for deserializing the `variable_map` in the desired YAML format.
#[derive(Debug, Deserialize)]
struct VariableMapEntry {
    internal_name: String,
    name: String,
    offset: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NetCdfInputConfiguration {
    pub variable_map: HashMap<InputVariableName, VariableEntry>,
    pub lat_name: String,
    pub lon_name: String,
    pub time_name: String,
    pub coords_dims: Option<(String, String)>,
    pub time_units: Option<String>,
}

impl Default for NetCdfInputConfiguration {
    fn default() -> Self {
        let mut variable_map: HashMap<InputVariableName, VariableEntry> = HashMap::new();
        InputVariableName::iter().for_each(|var| {
            if variable_map.contains_key(&var) {
                return;
            }
            warn!("Variable {} not found in configuration", &var);
            variable_map.insert(var, VariableEntry::new(var.to_string(), Some(0)));
        });

        NetCdfInputConfiguration {
            variable_map,
            lat_name: "latitude".into(),
            lon_name: "longitude".into(),
            time_name: "time".into(),
            coords_dims: None,
            time_units: None,
        }
    }
}

// Custom implementation for deserializing `NetCdfInputConfiguration`.
impl<'de> serde::Deserialize<'de> for NetCdfInputConfiguration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into an intermediate structure to capture the YAML format.
        #[derive(Deserialize)]
        struct IntermediateConfig {
            lat_name: String,
            lon_name: String,
            time_name: String,
            coords_dims: Option<(String, String)>,
            time_units: Option<String>,
            variable_map: Vec<VariableMapEntry>,
        }

        // Deserialize as an `IntermediateConfig` and convert it to `NetCdfInputConfiguration`.
        let intermediate = IntermediateConfig::deserialize(deserializer)?;

        // Convert the vector of `VariableMapEntry` into a `HashMap`.
        let variable_map: HashMap<InputVariableName, VariableEntry> = intermediate
            .variable_map
            .into_iter()
            .filter_map(|entry| {
                if let Ok(internal_name) = InputVariableName::from_str(&entry.internal_name) {
                    Some((internal_name, VariableEntry::new(entry.name, entry.offset)))
                } else {
                    None
                }
            })
            .collect();

        Ok(NetCdfInputConfiguration {
            variable_map,
            lat_name: intermediate.lat_name,
            lon_name: intermediate.lon_name,
            time_name: intermediate.time_name,
            coords_dims: intermediate.coords_dims,
            time_units: intermediate.time_units,
        })
    }
}

impl<T> From<T> for NetCdfInputConfiguration
where
    T: Into<String>,
{
    /// extracts the configruation from a string of key:value pairs
    /// example:
    ///
    fn from(s: T) -> Self {
        let string: String = s.into();
        let parts: Vec<&str> = string.split(',').collect();
        let raw_variable_map: HashMap<String, String> = parts
            .iter()
            .map(|part| {
                let kv: Vec<&str> = part.split(':').collect();
                let key = kv[0].trim().to_owned();
                let value = kv[1].trim().to_owned();
                (key, value)
            })
            .collect();

        let lat_name = raw_variable_map
            .get("latitude")
            .cloned()
            .unwrap_or_else(|| "latitude".to_owned());

        let lon_name = raw_variable_map
            .get("longitude")
            .cloned()
            .unwrap_or_else(|| "longitude".to_owned());

        let time_name = raw_variable_map
            .get("time")
            .cloned()
            .unwrap_or_else(|| "time".to_owned());

        let coords_dims = raw_variable_map.get("coords_dims").map(|s| {
            let parts: Vec<&str> = s.split(',').collect();
            (parts[0].to_owned(), parts[1].to_owned())
        });

        let mut variable_map: HashMap<InputVariableName, VariableEntry> = raw_variable_map
            .iter()
            .filter(|(k, _)| *k == "variable_map")
            .filter_map(|(k, v)| {
                if let Ok(var) = InputVariableName::from_str(k) {
                    let parts: Vec<&str> = v.split(';').collect();
                    let name = parts[0].to_owned();
                    let offset = if parts.len() > 1 {
                        parts[1].parse::<i64>().ok()
                    } else {
                        None
                    };
                    let entry = VariableEntry::new(name, offset);
                    Some((var, entry))
                } else {
                    warn!("Variable {} not recognized", k);
                    None
                }
            })
            .collect();

        // add defaults if missing
        InputVariableName::iter().for_each(|var| {
            if variable_map.contains_key(&var) {
                return;
            }
            warn!("Variable {} not found in configuration", &var);
            variable_map.insert(var, VariableEntry::new(var.to_string(), Some(0)));
        });

        NetCdfInputConfiguration {
            variable_map,
            lat_name,
            lon_name,
            time_name,
            coords_dims,
            time_units: None,
        }
    }
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
    /// Hours added to the run date before applying `output_time_resolution`
    /// (e.g. 12 with a resolution of 24 writes the daily maps at 12:00 UTC).
    #[serde(default)]
    pub output_time_offset: Option<i64>,
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

/// Where each tile's live model state lives between timesteps.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TileStateResidency {
    /// In memory when the `max_memory` budget has room for every tile's state
    /// at the chosen concurrency, or when no budget is set; on disk otherwise.
    #[default]
    Auto,
    /// Always in memory. The fastest option: no checkpoint is written or read.
    Memory,
    /// Always checkpointed to the scratch directory between timesteps, so the
    /// run holds the state of only the tiles in flight.
    Disk,
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
    /// Whether tile state stays in memory across timesteps or is checkpointed
    /// to disk. Reading and writing every tile's checkpoint each timestep is
    /// the single largest I/O cost of a run, so `auto` keeps the state in
    /// memory whenever the budget allows.
    #[serde(default)]
    pub tile_state: TileStateResidency,
    /// Whether a timestamp's output is resampled and written while the next
    /// timestamp is being stepped. Unset, it is decided from the budget: the
    /// overlap costs one timestamp's output grids and warm state on top of
    /// the step, and is only taken when that still fits `max_memory`.
    pub overlap_postprocess: Option<bool>,
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
            tile_state: TileStateResidency::default(),
            overlap_postprocess: None,
            scratch_directory: None,
        }
    }
}

impl StreamingExecutionConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
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
    pub fn max_memory_bytes(&self) -> Result<Option<u64>, ConfigError> {
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
fn parse_byte_size(text: &str) -> Result<u64, ConfigError> {
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

/// The whole `configuration.yml`.
///
/// Paths in it may be relative: [`ConfigContainer::load`] resolves them
/// against the directory of the configuration file, so a chain can write
/// `STATIC/slope.tif` or `../PALETTE/palettes.yml` instead of repeating its
/// absolute location. Absolute paths are used as they are.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigContainer {
    pub models: Vec<ConfigBuilderType>,
    /// Shared palette mapping (variable name -> palette file), a YAML file
    /// whose relative paths are resolved against its own directory. Entries
    /// of `palettes` win over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_file: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub palettes: PaletteMap,
    pub netcdf_input_configuration: Option<NetCdfInputConfiguration>,
    #[serde(default)]
    pub streaming: StreamingExecutionConfig,
}

/// Makes `path` absolute against `base` when it is relative and not empty.
fn resolve_path(base: &Path, path: &mut String) {
    if !path.is_empty() && Path::new(path.as_str()).is_relative() {
        *path = base.join(path.as_str()).to_string_lossy().into_owned();
    }
}

fn resolve_optional(base: &Path, path: &mut Option<String>) {
    if let Some(path) = path {
        resolve_path(base, path);
    }
}

fn resolve_outputs(base: &Path, outputs: &mut [OutputTypeConfig]) {
    for output in outputs {
        resolve_path(base, &mut output.path);
        resolve_path(base, &mut output.grid_path);
    }
}

fn resolve_static(base: &Path, static_data: &mut StaticDataConfig) {
    let StaticDataConfig::GeoTiff {
        domain_mask,
        slope,
        aspect,
        vegetation_id,
        vegetation_catalog,
        ppf_summer,
        ppf_winter,
    } = static_data;
    resolve_path(base, domain_mask);
    for path in [
        slope,
        aspect,
        vegetation_id,
        vegetation_catalog,
        ppf_summer,
        ppf_winter,
    ] {
        resolve_optional(base, path);
    }
}

fn resolve_warm_state(base: &Path, warm_state: &mut WarmStateConfig) {
    let WarmStateConfig::NetCdf { directory, .. } = warm_state;
    resolve_path(base, directory);
}

/// The absolute directory holding `file`.
fn absolute_directory(file: &Path) -> Result<PathBuf, ConfigError> {
    let directory = file.parent().unwrap_or(Path::new(""));
    if directory.is_absolute() {
        return Ok(directory.to_path_buf());
    }
    let cwd = std::env::current_dir()
        .map_err(|err| format!("Cannot read the current directory: {err}"))?;
    Ok(cwd.join(directory))
}

impl ConfigContainer {
    /// Reads, validates and resolves a configuration file: relative paths
    /// become absolute against the file's directory and the shared palette
    /// file is merged into `palettes`. This is what the runners use; tools
    /// that write the file back use [`ConfigContainer::from_file`].
    pub fn load(config_file: &str) -> Result<Self, ConfigError> {
        let mut conf = Self::from_file(config_file)?;
        let base = absolute_directory(Path::new(config_file))?;
        conf.resolve_paths(&base)?;
        Ok(conf)
    }

    /// Makes every relative path absolute against `base` and merges the
    /// shared palette file into `palettes`, inline entries winning.
    pub fn resolve_paths(&mut self, base: &Path) -> Result<(), ConfigError> {
        for model in &mut self.models {
            match model {
                ConfigBuilderType::RISICO(m) => {
                    resolve_static(base, &mut m.static_data);
                    resolve_warm_state(base, &mut m.warm_state);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::FWI(m) => {
                    resolve_static(base, &mut m.static_data);
                    resolve_warm_state(base, &mut m.warm_state);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Mark5(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_path(base, &mut m.warm_state_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::KBDI(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_path(base, &mut m.warm_state_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Nesterov(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_path(base, &mut m.warm_state_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Orieux(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_path(base, &mut m.warm_state_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Angstrom(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Fosberg(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Sharples(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_outputs(base, &mut m.output_types);
                }
                ConfigBuilderType::Hdw(m) => {
                    resolve_path(base, &mut m.cells_file_path);
                    resolve_outputs(base, &mut m.output_types);
                }
            }
        }
        resolve_optional(base, &mut self.streaming.scratch_directory);

        for path in self.palettes.values_mut() {
            resolve_path(base, path);
        }
        if let Some(file) = &mut self.palette_file {
            resolve_path(base, file);
            let contents = std::fs::read_to_string(file.as_str())
                .map_err(|err| format!("Cannot open palette file {file}: {err}"))?;
            let shared: PaletteMap = serde_yaml::from_str(&contents)
                .map_err(|err| format!("Cannot parse palette file {file}: {err}"))?;
            let palette_base = absolute_directory(Path::new(file.as_str()))?;
            for (name, mut path) in shared {
                resolve_path(&palette_base, &mut path);
                self.palettes.entry(name).or_insert(path);
            }
        }
        Ok(())
    }

    pub fn from_file(config_file: &str) -> Result<ConfigContainer, ConfigError> {
        if config_file.ends_with(".yaml") || config_file.ends_with(".yml") {
            Self::from_yaml(config_file)
        } else {
            Err(ConfigError::from(format!(
                "Unsupported config file format: {}",
                config_file
            )))
        }
    }

    pub fn from_yaml(config_file: &str) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(config_file)
            .map_err(|err| format!("Cannot open config file {}: {}", config_file, err))?;
        Self::parse(&contents)
            .map_err(|err| format!("Cannot parse config file {}: {}", config_file, err).into())
    }

    /// Parses and validates a configuration held in memory, as `from_yaml`
    /// does for a file.
    pub fn parse(contents: &str) -> Result<Self, ConfigError> {
        let conf: Self = serde_yaml::from_str(contents).map_err(|err| err.to_string())?;
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
    /// Compression level: deflate 0 to 9 for NETCDF, zstd 0 to 22 for ZARR,
    /// 0 for none. The NetCDF library takes one call at a time, so every
    /// variable of a timestep is deflated on one thread; on output-bound runs
    /// this level is most of the wall time. Zarr stores compress in parallel.
    #[serde(default = "default_compression_level")]
    pub compression_level: i32,
    pub variables: Vec<OutputVariable>,
}

fn default_compression_level() -> i32 {
    7
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
        assert!(matches!(
            config.static_data,
            StaticDataConfig::GeoTiff { .. }
        ));
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
        assert!(matches!(
            config.static_data,
            StaticDataConfig::GeoTiff { .. }
        ));
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
        assert_eq!(defaults.tile_state, TileStateResidency::Auto);
        assert_eq!(defaults.overlap_postprocess, None);
        defaults.validate().unwrap();

        let on_disk: StreamingExecutionConfig = serde_yaml::from_str("tile_state: disk").unwrap();
        assert_eq!(on_disk.tile_state, TileStateResidency::Disk);
        let in_memory: StreamingExecutionConfig =
            serde_yaml::from_str("tile_state: memory").unwrap();
        assert_eq!(in_memory.tile_state, TileStateResidency::Memory);
        assert!(serde_yaml::from_str::<StreamingExecutionConfig>("tile_state: tape").is_err());

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
    fn relative_paths_resolve_against_the_configuration_directory() {
        let dir = std::env::temp_dir().join(format!("risico-relpaths-{}", std::process::id()));
        let chain = dir.join("CHAIN");
        let palettes = dir.join("PALETTE");
        std::fs::create_dir_all(&chain).unwrap();
        std::fs::create_dir_all(&palettes).unwrap();
        std::fs::write(
            palettes.join("palettes.yml"),
            "V: RISICO_V.pal\nI: /abs/RISICO_I.pal\n",
        )
        .unwrap();
        let config = chain.join("configuration.yml");
        std::fs::write(
            &config,
            r#"
models:
  - type: RISICO
    model_name: CHAIN
    model_version: v2025
    static_data:
      type: geotiff
      domain_mask: STATIC/domain_mask.tif
      slope: /elsewhere/slope.tif
    warm_state:
      type: netcdf
      directory: STATE0-NC
    output_types:
      - internal_name: CHAIN
        name: CHAIN
        path: OUTPUT-NC
        grid_path: GRID/GRID.txt
        format: NETCDF
        variables: []
    output_time_resolution: 3
palette_file: ../PALETTE/palettes.yml
palettes:
  I: own_I.pal
"#,
        )
        .unwrap();

        let raw = ConfigContainer::from_file(config.to_str().unwrap()).unwrap();
        assert_eq!(raw.palettes.len(), 1, "from_file keeps the file as written");

        let conf = ConfigContainer::load(config.to_str().unwrap()).unwrap();
        let ConfigBuilderType::RISICO(model) = &conf.models[0] else {
            panic!("RISICO model expected");
        };
        let StaticDataConfig::GeoTiff {
            domain_mask, slope, ..
        } = &model.static_data;
        assert_eq!(
            domain_mask,
            &chain.join("STATIC/domain_mask.tif").to_string_lossy()
        );
        assert_eq!(slope.as_deref(), Some("/elsewhere/slope.tif"));
        let WarmStateConfig::NetCdf { directory, .. } = &model.warm_state;
        assert_eq!(directory, &chain.join("STATE0-NC").to_string_lossy());
        assert_eq!(
            model.output_types[0].path,
            chain.join("OUTPUT-NC").to_string_lossy()
        );
        assert_eq!(
            model.output_types[0].grid_path,
            chain.join("GRID/GRID.txt").to_string_lossy()
        );
        let shared_v = chain.join("../PALETTE").join("RISICO_V.pal");
        assert_eq!(conf.palettes["V"], shared_v.to_string_lossy());
        assert_eq!(
            conf.palettes["I"],
            chain.join("own_I.pal").to_string_lossy()
        );

        std::fs::remove_dir_all(&dir).unwrap();
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
