use std::{collections::HashMap, error::Error, str::FromStr};

use chrono::{DateTime, Utc};
use itertools::Itertools;
use log::warn;
use ndarray::Array1;
use netcdf::extent::Extents;
use rayon::prelude::*;
use std::fs;

use crate::library::{
    io::models::grid::{Grid, RegularGrid},
    modules::risico::constants::NODATAVAL,
};

use super::prelude::{InputHandler, InputVariableName};

#[derive(Clone)]
pub struct NetCdfInputConfiguration {
    pub variable_map: HashMap<InputVariableName, String>,
    pub lat_name: String,
    pub lon_name: String,
    pub time_name: String,
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
            .get("lat_name")
            .cloned()
            .unwrap_or_else(|| "latitude".to_owned());

        let lon_name = variable_map
            .get("lon_name")
            .cloned()
            .unwrap_or_else(|| "longitude".to_owned());

        let time_name = variable_map
            .get("time_name")
            .cloned()
            .unwrap_or_else(|| "time".to_owned());

        let variable_map = variable_map
            .iter()
            .filter(|(k, _)| *k != "lat_name" && *k != "lon_name" && *k != "time_name")
            .map(|(k, v)| (InputVariableName::from_str(k).unwrap(), v.clone()))
            .collect();

        NetCdfInputConfiguration {
            variable_map,
            lat_name,
            lon_name,
            time_name,
        }
    }
}

pub struct NetCdfFileInputRecord {
    file: String,
    timeline: Array1<DateTime<Utc>>,
    variables: Vec<InputVariableName>,
    indexes: Array1<Option<usize>>,
}

/// inspect a single netcdf file and builds a record
fn register_nc_file(
    file: &str,
    config: &NetCdfInputConfiguration,
    lats: &[f32],
    lons: &[f32],
) -> Result<NetCdfFileInputRecord, Box<dyn Error>> {
    let nc_file = netcdf::open(file)?;

    let lats_var = &nc_file
        .variable(&config.lat_name)
        .expect("Could not find variable 'latitude'");
    let lons_var = &nc_file
        .variable(&config.lon_name)
        .expect("Could not find variable 'longitude'");

    let time_var = &nc_file
        .variable(&config.time_name)
        .expect("Could not find variable 'longitude'");

    let variables = nc_file
        .variables()
        .filter_map(|var| {
            let nc_var = var.name().to_owned();
            let var_name = config
                .variable_map
                .iter()
                .find(|(_, v)| *v == &nc_var)
                .map(|(k, _)| k.clone());
            var_name
        })
        .collect::<Vec<InputVariableName>>();

    let timeline = time_var
        .values::<i64, _>(Extents::All)?
        .into_iter()
        .map(|t| DateTime::from_timestamp_nanos(t).to_utc())
        .collect::<Array1<DateTime<Utc>>>();

    let nc_lats = lats_var
        .values::<f32, _>(Extents::All)?
        .into_iter()
        .collect::<Array1<f32>>();

    let nc_lons = lons_var
        .values::<f32, _>(Extents::All)?
        .into_iter()
        .collect::<Array1<f32>>();

    let nrows = nc_lats.len();
    let ncols = nc_lons.len();

    let min_lat = nc_lats[0] as f32;
    let max_lat = nc_lats[nc_lats.len() - 1] as f32;
    let min_lon = nc_lons[0] as f32;
    let max_lon = nc_lons[nc_lons.len() - 1] as f32;

    let mut grid = RegularGrid::new(nrows, ncols, min_lat, max_lat, min_lon, max_lon);

    let indexes = grid.indexes(lats, lons);

    let record = NetCdfFileInputRecord {
        file: file.to_owned(),
        timeline,
        variables,
        indexes,
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
        .expect(&format!("Could not find variable '{}'", variable));

    let extent: Extents = (&[time_index], &[1]).try_into().unwrap();
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
    pub fn new(
        path: &str,
        lats: &[f32],
        lons: &[f32],
        config: &NetCdfInputConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
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
            match register_nc_file(&file_path_str, config, lats, lons) {
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
            let time_index = time_index.unwrap();
            let variable = self.config.variable_map.get(&var).unwrap();

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

    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<InputVariableName> {
        let mut variables = Vec::new();

        for record in &self.records {
            let time_index = record.timeline.iter().position(|t| t == time);
            if time_index.is_some() {
                variables.extend_from_slice(&record.variables);
            }
        }
        variables
    }
}
