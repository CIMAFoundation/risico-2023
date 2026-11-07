use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::BufRead;

use crate::common::helpers::RISICOError;

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

pub use risico::config::*;

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

/// Builds the runtime configuration of a model from its `configuration.yml`
/// entry (the schema lives in `risico::config`).
pub trait BuildConfig {
    type Output;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<Self::Output, RISICOError>;
}

impl BuildConfig for RISICOConfigBuilder {
    type Output = RISICOConfig;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<RISICOConfig, RISICOError> {
        RISICOConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for FWIConfigBuilder {
    type Output = FWIConfig;

    fn build(&self, date: &DateTime<Utc>, palettes: &PaletteMap) -> Result<FWIConfig, RISICOError> {
        FWIConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for Mark5ConfigBuilder {
    type Output = Mark5Config;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<Mark5Config, RISICOError> {
        Mark5Config::new(self, *date, palettes)
    }
}

impl BuildConfig for KbdiConfigBuilder {
    type Output = KbdiConfig;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<KbdiConfig, RISICOError> {
        KbdiConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for AngstromConfigBuilder {
    type Output = AngstromConfig;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<AngstromConfig, RISICOError> {
        AngstromConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for FosbergConfigBuilder {
    type Output = FosbergConfig;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<FosbergConfig, RISICOError> {
        FosbergConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for NesterovConfigBuilder {
    type Output = NesterovConfig;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<NesterovConfig, RISICOError> {
        NesterovConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for SharplesConfigBuilder {
    type Output = SharplesConfig;

    fn build(
        &self,
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<SharplesConfig, RISICOError> {
        SharplesConfig::new(self, *date, palettes)
    }
}

impl BuildConfig for OrieuxConfigBuilder {
    type Output = OrieuxConfig;

    fn build(
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

impl BuildConfig for HdwConfigBuilder {
    type Output = HdwConfig;

    fn build(&self, date: &DateTime<Utc>, palettes: &PaletteMap) -> Result<HdwConfig, RISICOError> {
        HdwConfig::new(self, *date, palettes)
    }
}
