use std::{collections::HashMap, error::Error, str::FromStr};

use cftime_rs::{
    calendars::Calendar, datetime::CFDatetime, utils::get_datetime_and_unit_from_units,
};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use log::warn;
use ndarray::Array1;
use netcdf::extent::Extents;
use rayon::prelude::*;

use risico::{constants::NODATAVAL, models::input::InputVariableName};
use serde_derive::{Deserialize, Serialize};
use std::fs;
use strum::IntoEnumIterator;

use crate::common::io::models::grid::{Grid, IrregularGrid};

use super::prelude::InputHandler;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetCdfInputConfiguration {
    pub variable_map: HashMap<InputVariableName, String>,
    pub lat_name: String,
    pub lon_name: String,
    pub time_name: String,
    pub coords_dims: Option<(String, String)>,
}

impl Default for NetCdfInputConfiguration {
    fn default() -> Self {
        let mut variable_map: HashMap<InputVariableName, String> = HashMap::new();
        InputVariableName::iter().for_each(|var| {
            if variable_map.contains_key(&var) {
                return;
            }
            warn!("Variable {} not found in configuration", &var);
            variable_map.insert(var, var.to_string());
        });

        NetCdfInputConfiguration {
            variable_map,
            lat_name: "latitude".into(),
            lon_name: "longitude".into(),
            time_name: "time".into(),
            coords_dims: None,
        }
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
        let variable_map: HashMap<String, String> = parts
            .iter()
            .map(|part| {
                let kv: Vec<&str> = part.split(':').collect();
                let key = kv[0].trim().to_owned();
                let value = kv[1].trim().to_owned();
                (key, value)
            })
            .collect();

        let lat_name = variable_map
            .get("latitude")
            .cloned()
            .unwrap_or_else(|| "latitude".to_owned());

        let lon_name = variable_map
            .get("longitude")
            .cloned()
            .unwrap_or_else(|| "longitude".to_owned());

        let time_name = variable_map
            .get("time")
            .cloned()
            .unwrap_or_else(|| "time".to_owned());

        let mut variable_map: HashMap<InputVariableName, String> = variable_map
            .iter()
            .filter(|(k, _)| *k != "latitude" && *k != "longitude" && *k != "time")
            .filter_map(|(k, v)| {
                if let Ok(var) = InputVariableName::from_str(k) {
                    Some((var, v.clone()))
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
            variable_map.insert(var, var.to_string());
        });

        NetCdfInputConfiguration {
            variable_map,
            lat_name,
            lon_name,
            time_name,
            coords_dims: None,
        }
    }
}

pub struct NetCdfFileInputRecord {
    file: String,
    timeline: Array1<DateTime<Utc>>,
    variables: Vec<InputVariableName>,
    grid: IrregularGrid,
    indexes: Option<Array1<Option<usize>>>,
}

/// inspect a single netcdf file and builds a record
fn register_nc_file(
    file: &str,
    config: &NetCdfInputConfiguration,
) -> Result<NetCdfFileInputRecord, Box<dyn Error>> {
    let nc_file = netcdf::open(file)?;

    let lats_var = &nc_file
        .variable(&config.lat_name)
        .ok_or_else(|| format!("Could not find variable {}", &config.lat_name))?;

    let lons_var = &nc_file
        .variable(&config.lon_name)
        .ok_or_else(|| format!("Could not find variable {}", &config.lon_name))?;

    let time_var = &nc_file
        .variable(&config.time_name)
        .ok_or_else(|| format!("Could not find variable {}", &config.time_name))?;

    let variables = nc_file
        .variables()
        .filter_map(|var| {
            let nc_var = var.name().to_owned();
            let var_name = config
                .variable_map
                .iter()
                .find(|(_, v)| *v == &nc_var)
                .map(|(k, _)| *k);
            var_name
        })
        .collect::<Vec<InputVariableName>>();

    let units = time_var.attribute("description").unwrap();
    let calendar = Calendar::Standard;
    let (cf_datetime, unit) = get_datetime_and_unit_from_units(units, calendar)?;

    let timeline = time_var
        .values::<i64, _>(Extents::All)?
        .into_iter()
        .filter_map(|t| cf_datetime.decode(t).ok())
        .map(|d| d.to_datetime())
        .collect::<Array1<DateTime<Utc>>>();

    let dimensions = lats_var.dimensions();

    let (extents, nrows, ncols) = if let Some((lat_dim, lon_dim)) = &config.coords_dims {
        let lat_index = dimensions
            .iter()
            .position(|dim| &dim.name() == lat_dim)
            .unwrap_or_else(|| panic!("Could not find dimension '{}'", lat_dim));
        let lon_index = dimensions
            .iter()
            .position(|dim| &dim.name() == lon_dim)
            .unwrap_or_else(|| panic!("Could not find dimension '{}'", lon_dim));

        let mut extents = (0..dimensions.len()).map(|_| 0..1).collect::<Vec<_>>();
        extents[lat_index] = 0..dimensions[lat_index].len();
        extents[lon_index] = 0..dimensions[lon_index].len();

        let nrows = dimensions[lat_index].len();
        let ncols = dimensions[lon_index].len();
        (extents.as_slice().into(), nrows, ncols)
    } else {
        let ndims = dimensions.len();
        if ndims != 2 {
            return Err("Latitude & Longitude variables must have 2 dimensions".into());
        }
        let nrows = dimensions[0].len();
        let ncols = dimensions[1].len();
        (Extents::All, nrows, ncols)
    };

    let nc_lats = lats_var
        .values::<f32, _>(&extents)?
        .into_iter()
        .collect::<Array1<f32>>();

    let nc_lons = lons_var
        .values::<f32, _>(&extents)?
        .into_iter()
        .collect::<Array1<f32>>();

    let grid = IrregularGrid::new(nrows, ncols, nc_lats, nc_lons);

    let record = NetCdfFileInputRecord {
        file: file.to_owned(),
        timeline,
        variables,
        grid,
        indexes: None,
    };

    Ok(record)
}

/// read a slice of a variable from a netcdf file
fn read_variable_from_file(
    file: &str,
    variable: &str,
    time_index: usize,
) -> Result<Array1<f32>, Box<dyn Error>> {
    let nc_file = netcdf::open(file)?;

    let var = nc_file
        .variable(variable)
        .unwrap_or_else(|| panic!("Could not find variable '{}'", variable));

    let extent: Extents = (time_index, .., ..)
        .try_into()
        .unwrap_or_else(|_| panic!("Could not create extent '{}'", &time_index));
    let values = var
        .values::<f32, _>(extent)?
        .into_iter()
        .collect::<Array1<f32>>();

    Ok(values)
}

pub struct NetCdfInputHandler {
    records: Vec<NetCdfFileInputRecord>,
    config: NetCdfInputConfiguration,
}

impl NetCdfInputHandler {
    pub fn new(path: &str, config: &NetCdfInputConfiguration) -> Result<Self, Box<dyn Error>> {
        let mut records = Vec::new();

        // Iterate over the files in the specified directory
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            // Check if the entry is a file and has the .nc extension
            if !file_path.is_file() || file_path.extension().unwrap_or_default() != "nc" {
                continue;
            }
            // Convert the file path to a string
            let file_path_str = file_path.to_string_lossy().into_owned();

            // Call the inspect_nc_file function to build the record
            match register_nc_file(&file_path_str, config) {
                Ok(record) => records.push(record),
                Err(e) => warn!("Error inspecting file {}: {}", file_path_str, e),
            }
        }

        Ok(NetCdfInputHandler {
            records,
            config: config.clone(),
        })
    }
}

impl InputHandler for NetCdfInputHandler {
    fn get_values(&self, var: InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>> {
        for record in &self.records {
            let time_index = record.timeline.iter().position(|t| t == date);

            if time_index.is_none() || !record.variables.contains(&var) {
                continue;
            }
            let time_index = time_index.expect("Could not find time index");
            let variable = self.config.variable_map.get(&var).unwrap_or_else(|| {
                panic!("Could not find variable mapping for variable '{}'", var)
            });

            let values = read_variable_from_file(&record.file, variable, time_index);

            match values {
                Err(err) => {
                    let file = &record.file;
                    warn!("Error reading variable {variable} from file {file}: {err}");
                    continue;
                }
                Ok(values) => {
                    let data: Vec<f32> = record
                        .indexes
                        .as_ref()
                        .expect("indexes should be set")
                        .par_iter()
                        .map(|index| index.and_then(|idx| Some(values[idx])).unwrap_or(NODATAVAL))
                        .collect();

                    let data = Array1::from(data);
                    return Some(data);
                }
            }
        }
        None
    }

    fn get_timeline(&self) -> Vec<DateTime<Utc>> {
        self.records
            .iter()
            .flat_map(|record| record.timeline.iter())
            .unique()
            .cloned()
            .sorted()
            .collect()
    }

    fn set_coordinates(&mut self, lats: &[f32], lons: &[f32]) -> Result<(), Box<dyn Error>> {
        for record in &mut self.records {
            let grid = &mut record.grid;
            let indexes = grid.indexes(lats, lons);
            record.indexes = Some(Array1::from(indexes));
        }
        Ok(())
    }
}
