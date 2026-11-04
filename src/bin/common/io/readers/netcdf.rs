use std::{
    collections::HashMap,
    error::Error,
    str::FromStr,
    sync::{Arc, Mutex},
};

use cftime_rs::{calendars::Calendar, parser::Unit, utils::get_datetime_and_unit_from_units};
use chrono::{DateTime, TimeZone, Utc};
use itertools::Itertools;
use log::{debug, warn};
use ndarray::Array1;
use netcdf_reader::{NcAttrValue, NcFile, NcSliceInfo, NcSliceInfoElem, NcType, NcVariable};

use risico::models::input::InputVariableName;
use serde;
use serde_derive::{Deserialize, Serialize};
use std::fs;
use strum::IntoEnumIterator;

use crate::common::io::models::grid::{CellIndexes, Grid, IrregularGrid};
use rayon::prelude::*;

use super::prelude::InputHandler;

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

pub struct NetCdfFileInputRecord {
    file: String,
    timeline: Array1<DateTime<Utc>>,
    variables: Vec<InputVariableName>,
    grid: IrregularGrid,
    indexes: Option<CellIndexes>,
}

/// Read a whole variable as `i64`, converting from whatever numeric type it
/// is stored as (netcdf-c truncated doubles the same way).
fn read_i64_values(file: &NcFile, variable: &NcVariable) -> Result<Vec<i64>, Box<dyn Error>> {
    let name = variable.name();
    if *variable.dtype() == NcType::Int64 {
        return Ok(file.read_variable::<i64>(name)?.into_raw_vec_and_offset().0);
    }
    Ok(file
        .read_variable_as_f64(name)?
        .iter()
        .map(|value| *value as i64)
        .collect())
}

/// Read a selection of a variable as `f32`, converting other numeric types.
fn read_f32_values(
    file: &NcFile,
    variable: &NcVariable,
    selection: Option<&NcSliceInfo>,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let name = variable.name();
    if *variable.dtype() == NcType::Float {
        let values = match selection {
            Some(selection) => file.read_variable_slice::<f32>(name, selection)?,
            None => file.read_variable::<f32>(name)?,
        };
        return Ok(values.into_raw_vec_and_offset().0);
    }
    let values = match selection {
        Some(selection) => file.read_variable_slice_as_f64(name, selection)?,
        None => file.read_variable_as_f64(name)?,
    };
    Ok(values.iter().map(|value| *value as f32).collect())
}

fn text_attribute(variable: &NcVariable, name: &str) -> Option<String> {
    match &variable.attribute(name)?.value {
        NcAttrValue::Chars(text) => Some(text.clone()),
        NcAttrValue::Strings(values) => Some(values.concat()),
        _ => None,
    }
}

/// extract the time from a netcdf file using the given attribute
fn extract_time(
    file: &NcFile,
    time_var: &NcVariable,
    time_units: &Option<String>,
    offset_seconds: &i64,
) -> Result<Array1<DateTime<Utc>>, Box<dyn Error>> {
    let default_units_name: String = String::from("units");
    let time_units_attr_name = time_units.as_ref().unwrap_or(&default_units_name);

    let units_attr = time_var.attribute(time_units_attr_name);
    let timeline = if units_attr.is_none() && time_units.is_none() {
        // if the units attribute is not found, try to use the default units which are "seconds since 1970-01-01 00:00:00"
        read_i64_values(file, time_var)?
            .into_iter()
            .filter_map(|t| {
                let adjusted_time = t + offset_seconds; // apply the offset
                DateTime::from_timestamp_millis(adjusted_time * 1000)
            })
            .collect::<Array1<DateTime<Utc>>>()
    } else {
        let units = text_attribute(time_var, time_units_attr_name)
            .ok_or("Could not find units")?;

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

        read_i64_values(file, time_var)?
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
    let nc_file = NcFile::open(file)?;

    let lats_var = nc_file
        .variable(&config.lat_name)
        .map_err(|_| format!("Could not find variable {}", &config.lat_name))?;

    let lons_var = nc_file
        .variable(&config.lon_name)
        .map_err(|_| format!("Could not find variable {}", &config.lon_name))?;

    let time_var = nc_file
        .variable(&config.time_name)
        .map_err(|_| format!("Could not find variable {}", &config.time_name))?;

    let (variables, offsets): (Vec<InputVariableName>, Vec<Option<i64>>) = nc_file
        .variables()?
        .iter()
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

    let offsets: Vec<i64> = offsets.into_iter().map(|opt| opt.unwrap_or(0)).collect();

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

    let timeline = extract_time(&nc_file, time_var, &config.time_units, offset)?;

    let dimensions = lats_var.dimensions();

    let (selection, nrows, ncols) = if let Some((lat_dim, lon_dim)) = &config.coords_dims {
        let lat_index = dimensions
            .iter()
            .position(|dim| &dim.name == lat_dim)
            .ok_or_else(|| format!("Could not find dimension '{}'", lat_dim))?;
        let lon_index = dimensions
            .iter()
            .position(|dim| &dim.name == lon_dim)
            .ok_or_else(|| format!("Could not find dimension '{}'", lon_dim))?;

        // The first entry along every other dimension, as before.
        let selections = dimensions
            .iter()
            .enumerate()
            .map(|(index, dim)| {
                if index == lat_index || index == lon_index {
                    NcSliceInfoElem::Slice {
                        start: 0,
                        end: dim.size,
                        step: 1,
                    }
                } else {
                    NcSliceInfoElem::Index(0)
                }
            })
            .collect();

        let nrows = dimensions[lat_index].size as usize;
        let ncols = dimensions[lon_index].size as usize;
        (Some(NcSliceInfo { selections }), nrows, ncols)
    } else {
        let ndims = dimensions.len();
        if ndims != 2 {
            return Err("Latitude & Longitude variables must have 2 dimensions".into());
        }
        let nrows = dimensions[0].size as usize;
        let ncols = dimensions[1].size as usize;
        (None, nrows, ncols)
    };

    let nc_lats = Array1::from_vec(read_f32_values(&nc_file, lats_var, selection.as_ref())?);
    let nc_lons = Array1::from_vec(read_f32_values(&nc_file, lons_var, selection.as_ref())?);

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
    let nc_file = NcFile::open(file)?;

    let var = nc_file
        .variable(variable)
        .map_err(|_| format!("Could not find variable '{}'", variable))?;
    if var.ndim() == 0 {
        return Err(format!("variable '{variable}' has no time dimension").into());
    }

    // One timestep, whole grid: the first dimension is time.
    let mut selections = vec![NcSliceInfoElem::Index(time_index as u64)];
    selections.extend(var.dimensions()[1..].iter().map(|dim| NcSliceInfoElem::Slice {
        start: 0,
        end: dim.size,
        step: 1,
    }));
    let selection = NcSliceInfo { selections };
    let values = Array1::from_vec(read_f32_values(&nc_file, var, Some(&selection))?);

    Ok(values)
}

pub struct NetCdfInputHandler {
    records: Vec<NetCdfFileInputRecord>,
    config: NetCdfInputConfiguration,
    source_values: Mutex<HashMap<(String, String, usize), Arc<Array1<f32>>>>,
    coordinate_registry: Vec<Vec<CellIndexes>>,
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
            source_values: Mutex::new(HashMap::new()),
            coordinate_registry: Vec::new(),
        })
    }
}

impl InputHandler for NetCdfInputHandler {
    /// Decode every variable this timestamp refers to, in parallel.
    fn preload(&mut self, date: &DateTime<Utc>) {
        let mut pending: Vec<(String, String, usize)> = Vec::new();
        for record in &self.records {
            let Some(time_index) = record.timeline.iter().position(|t| t == date) else {
                continue;
            };
            for var in &record.variables {
                let Some(variable_info) = self.config.variable_map.get(var) else {
                    continue;
                };
                pending.push((record.file.clone(), variable_info.name.clone(), time_index));
            }
        }

        {
            let cache = self
                .source_values
                .lock()
                .expect("NetCDF source cache lock is poisoned");
            pending.retain(|key| !cache.contains_key(key));
        }
        pending.sort();
        pending.dedup();

        let decoded: Vec<((String, String, usize), Arc<Array1<f32>>)> = pending
            .into_par_iter()
            .filter_map(|key| {
                let (file, variable, time_index) = &key;
                match read_variable_from_file(file, variable, *time_index) {
                    Ok(values) => Some((key.clone(), Arc::new(values))),
                    Err(error) => {
                        warn!("Error reading variable {variable} from file {file}: {error}");
                        None
                    }
                }
            })
            .collect();

        let mut cache = self
            .source_values
            .lock()
            .expect("NetCDF source cache lock is poisoned");
        cache.extend(decoded);
    }

    fn get_values(
        &self,
        selection: usize,
        var: InputVariableName,
        date: &DateTime<Utc>,
    ) -> Option<Array1<f32>> {
        for (record_index, record) in self.records.iter().enumerate() {
            let time_index = record.timeline.iter().position(|t| t == date);

            if time_index.is_none() || !record.variables.contains(&var) {
                continue;
            }
            let time_index = time_index.expect("Could not find time index");
            let variable_info = self.config.variable_map.get(&var).unwrap_or_else(|| {
                panic!("Could not find variable mapping for variable '{}'", var)
            });

            let variable = &variable_info.name;

            let cache_key = (record.file.clone(), variable.clone(), time_index);
            let values = self
                .source_values
                .lock()
                .expect("NetCDF source cache lock is poisoned")
                .get(&cache_key)
                .cloned();
            let values = match values {
                Some(values) => Ok(values),
                None => read_variable_from_file(&record.file, variable, time_index).map(Arc::new),
            };

            match values {
                Err(err) => {
                    let file = &record.file;
                    warn!("Error reading variable {variable} from file {file}: {err}");
                    continue;
                }
                Ok(values) => {
                    self.source_values
                        .lock()
                        .expect("NetCDF source cache lock is poisoned")
                        .entry(cache_key)
                        .or_insert_with(|| values.clone());
                    let indexes = self
                        .coordinate_registry
                        .get(selection)
                        .and_then(|selections| selections.get(record_index))
                        .or(record.indexes.as_ref())
                        .expect("indexes should be set");
                    let data =
                        indexes.gather(values.as_slice().expect("source values are contiguous"));
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

    fn register_coordinates(
        &mut self,
        lats: &[f32],
        lons: &[f32],
    ) -> Result<usize, Box<dyn Error>> {
        let indexes = self
            .records
            .iter()
            .map(|record| record.grid.indexes(lats, lons))
            .collect();
        let selection = self.coordinate_registry.len();
        self.coordinate_registry.push(indexes);
        Ok(selection)
    }

    fn clear_registered_coordinates(&mut self) {
        self.coordinate_registry.clear();
    }

    fn clear_cached_values(&mut self) {
        self.source_values
            .lock()
            .expect("NetCDF source cache lock is poisoned")
            .clear();
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

#[cfg(test)]
mod tests {
    use super::*;
    use nc4_stream::{DataType, Nc4Writer, VariableOptions};
    use std::path::{Path, PathBuf};

    fn temporary_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "risico-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// A curvilinear 2 x 3 input: double coordinates, a double time axis in
    /// hours, one deflated float variable and one double variable, so both
    /// the typed and the promoted read paths are exercised.
    fn write_input_file(path: &Path) {
        let mut file = Nc4Writer::create(path).unwrap();
        let time = file.add_dimension("time", 2).unwrap();
        let y = file.add_dimension("y", 2).unwrap();
        let x = file.add_dimension("x", 3).unwrap();
        let lat = file
            .add_variable("latitude", DataType::F64, &[y, x], VariableOptions::contiguous())
            .unwrap();
        file.put_values(lat, &[44.0f64, 44.0, 44.0, 45.0, 45.0, 45.0])
            .unwrap();
        let lon = file
            .add_variable("longitude", DataType::F64, &[y, x], VariableOptions::contiguous())
            .unwrap();
        file.put_values(lon, &[8.0f64, 9.0, 10.0, 8.0, 9.0, 10.0])
            .unwrap();
        let t = file
            .add_variable("time", DataType::F64, &[time], VariableOptions::contiguous())
            .unwrap();
        file.put_var_attr(t, "units", "hours since 2025-01-01 00:00:00")
            .unwrap();
        file.put_values(t, &[0.0f64, 1.0]).unwrap();
        let t2 = file
            .add_variable(
                "T2",
                DataType::F32,
                &[time, y, x],
                VariableOptions::chunked(vec![1, 2, 3]).deflate(4).shuffle(true),
            )
            .unwrap();
        file.put_values(
            t2,
            &[10.0f32, 11.0, 12.0, 13.0, 14.0, 15.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0],
        )
        .unwrap();
        let rh = file
            .add_variable("RH", DataType::F64, &[time, y, x], VariableOptions::contiguous())
            .unwrap();
        file.put_values(
            rh,
            &[50.0f64, 51.0, 52.0, 53.0, 54.0, 55.0, 60.0, 61.0, 62.0, 63.0, 64.0, 65.0],
        )
        .unwrap();
        file.finish().unwrap();
    }

    #[test]
    fn netcdf4_inputs_are_registered_and_read_through_the_pure_rust_reader() {
        let directory = temporary_directory("netcdf-input");
        write_input_file(&directory.join("input.nc"));

        let mut config = NetCdfInputConfiguration::default();
        config.variable_map.insert(
            InputVariableName::T,
            VariableEntry::new("T2".into(), Some(0)),
        );
        config.variable_map.insert(
            InputVariableName::H,
            VariableEntry::new("RH".into(), Some(0)),
        );
        let mut handler = NetCdfInputHandler::new(directory.to_str().unwrap(), &config).unwrap();

        let timeline = handler.get_timeline();
        assert_eq!(
            timeline,
            vec![
                Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 1, 1, 0, 0).unwrap(),
            ]
        );

        // Two model cells: the north-east and the south-west corner.
        let selection = handler
            .register_coordinates(&[45.0, 44.0], &[10.0, 8.0])
            .unwrap();
        let temperature = handler
            .get_values(selection, InputVariableName::T, &timeline[1])
            .unwrap();
        assert_eq!(temperature.to_vec(), vec![25.0, 20.0]);
        let humidity = handler
            .get_values(selection, InputVariableName::H, &timeline[0])
            .unwrap();
        assert_eq!(humidity.to_vec(), vec![55.0, 50.0]);

        // The preload path decodes through the same reader into the cache.
        handler.clear_cached_values();
        handler.preload(&timeline[0]);
        let cached = handler
            .get_values(selection, InputVariableName::T, &timeline[0])
            .unwrap();
        assert_eq!(cached.to_vec(), vec![15.0, 10.0]);

        fs::remove_dir_all(directory).unwrap();
    }
}
