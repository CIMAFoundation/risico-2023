use std::{collections::HashMap, error::Error, str::FromStr};

use cftime_rs::{calendars::Calendar, parser::Unit, utils::get_datetime_and_unit_from_units};
use chrono::{DateTime, TimeZone, Utc};
use itertools::Itertools;
use log::{debug, warn};
use ndarray::Array1;
use netcdf::{extent::Extents, AttrValue, Variable};
use rayon::prelude::*;

use risico::{constants::NODATAVAL, models::input::InputVariableName};
use serde;
use serde_derive::{Deserialize, Serialize};
use std::fs;
use strum::IntoEnumIterator;

use crate::common::io::models::grid::{Grid, IrregularGrid};

use super::prelude::InputHandler;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariableEntry {
    pub name: String, // variable name in netcdf file
    pub offset: i64,  // offset in seconds to be aplied in the variable time line
}

impl VariableEntry {
    pub fn new(name: String, offset: i64) -> Self {
        VariableEntry { name, offset }
    }
}

// Define a helper struct for deserializing the `variable_map` in the desired YAML format.
#[derive(Debug, Deserialize)]
struct VariableMapEntry {
    internal_name: String,
    name: String,
    offset: i64,
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
            variable_map.insert(var, VariableEntry::new(var.to_string(), 0));
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
                    let offset = parts[1].parse::<i64>().unwrap_or_else(|e| {
                        warn!("Error parsing offset: {}", e);
                        0
                    });
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
            variable_map.insert(var, VariableEntry::new(var.to_string(), 0));
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

pub struct NetCdfFileInputRecord {
    file: String,
    timeline: Array1<DateTime<Utc>>,
    variables: Vec<InputVariableName>,
    grid: IrregularGrid,
    indexes: Option<Array1<Option<usize>>>,
}

/// extract the time from a netcdf file using the given attribute
fn extract_time(
    time_var: &Variable,
    time_units: &Option<String>,
    offset_seconds: &i64,
) -> Result<Array1<DateTime<Utc>>, Box<dyn Error>> {
    let default_units_name: String = String::from("units");
    let time_units_attr_name = time_units.as_ref().unwrap_or(&default_units_name);

    let units_attr = time_var.attribute(time_units_attr_name);
    let timeline = if units_attr.is_none() && time_units.is_none() {
        // if the units attribute is not found, try to use the default units which are "seconds since 1970-01-01 00:00:00"
        time_var
            .values::<i64, _>(Extents::All)?
            .into_iter()
            .filter_map(|t| {
                let adjusted_time = t + offset_seconds; // apply the offset
                DateTime::from_timestamp_millis(adjusted_time * 1000)
            })
            .collect::<Array1<DateTime<Utc>>>()
    } else {
        let units_attr_values = units_attr
            .expect("should have units attribute")
            .value()
            .expect("should have a value");

        let units = if let AttrValue::Str(units) = units_attr_values {
            units.to_owned()
        } else {
            return Err("Could not find units".into());
        };

        let calendar = Calendar::Standard;
        let (cf_datetime, unit) = get_datetime_and_unit_from_units(&units, calendar)?;
        let duration = unit.to_duration(calendar);

        // Convert offset_seconds to the specified unit
        let offset_in_specified_unit = match unit {
            Unit::Day => *offset_seconds / 86400,
            Unit::Hour => *offset_seconds / 3600,
            Unit::Minute => *offset_seconds / 60,
            Unit::Second => *offset_seconds,
            Unit::Millisecond => *offset_seconds * 1000,
            Unit::Microsecond => *offset_seconds * 1000000,
            Unit::Nanosecond => *offset_seconds * 1000000000,
            _ => return Err("Problem with converstion of the offset".into()),
        };

        time_var
            .values::<i64, _>(Extents::All)?
            .into_iter()
            .filter_map(|t| {
                let adjusted_time = t + offset_in_specified_unit; // apply the offset
                (&cf_datetime + (&duration * adjusted_time)).ok()
            })
            .map(|d| {
                let (year, month, day, hour, minute, seconds) =
                    d.ymd_hms().expect("should be a valid date");
                let year: i32 = year.try_into().unwrap();
                // create a UTC datetime
                Utc.with_ymd_and_hms(
                    year,
                    month as u32,
                    day as u32,
                    hour as u32,
                    minute as u32,
                    seconds as u32,
                )
                .single()
                .expect("should be a valid date")
            })
            .collect::<Array1<DateTime<Utc>>>()
    };
    Ok(timeline)
}

/// inspect a single netcdf file and builds a record
fn register_nc_file(
    file: &str,
    config: &NetCdfInputConfiguration,
) -> Result<Option<NetCdfFileInputRecord>, Box<dyn Error>> {
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

    let (variables, offsets): (Vec<InputVariableName>, Vec<i64>) = nc_file
        .variables()
        .filter_map(|var| {
            let nc_var = var.name().to_owned();
            // Find the variable in the config and handle both enum variants
            config
                .variable_map
                .iter()
                .find(|(_, entry)| entry.name == nc_var)
                .map(|(k, entry)| (*k, entry.offset))
        })
        .unzip();

    // If no variables are found, return None
    if variables.is_empty() {
        return Ok(None);
    }

    // check it all offests are the same, and extract the unique value, otherwise return an error
    let offset: &i64 = if offsets.is_empty() {
        &0 // default offset
    } else {
        offsets
            .iter()
            .unique()
            .next()
            .ok_or("All variables must have the same offset")?
    };

    let timeline = extract_time(time_var, &config.time_units, offset)?;

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

    Ok(Some(record))
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
                Ok(Some(record)) => {
                    records.push(record);
                }
                Ok(None) => {
                    debug!("No specified variables found in file {}", file_path_str);
                }
                Err(e) => {
                    warn!("Error inspecting file {}: {}", file_path_str, e);
                }
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
            let variable_info = self.config.variable_map.get(&var).unwrap_or_else(|| {
                panic!("Could not find variable mapping for variable '{}'", var)
            });

            let variable = &variable_info.name;

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

    fn info_input(&self) -> String {
        // print the file and variables for each record
        let mut info = String::new();
        for record in &self.records {
            info.push_str(&format!("File: {}\n", record.file));
            info.push_str(&format!("Variables: {:?}\n", record.variables));
        }
        info
    }
}
