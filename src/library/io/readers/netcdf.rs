use std::error::Error;

use chrono::{DateTime, Utc};
use log::warn;
use ndarray::Array1;
use netcdf::extent::Extents;
use std::fs;

use crate::library::io::models::grid::{Grid, RegularGrid};

use super::prelude::InputHandler;

pub struct NetCdfFileInputRecord {
    file: String,
    timeline: Array1<DateTime<Utc>>,
    variables: Vec<String>,
    indexes: Array1<Option<usize>>,
}

/// inspect a single netcdf file and builds a record
fn register_nc_file(
    file: &str,
    lat_name: &str,
    lon_name: &str,
    time_name: &str,
    lats: &[f32],
    lons: &[f32],
) -> Result<NetCdfFileInputRecord, Box<dyn Error>> {
    let nc_file = netcdf::open(file)?;

    let lats_var = &nc_file
        .variable(lat_name)
        .expect("Could not find variable 'latitude'");
    let lons_var = &nc_file
        .variable(lon_name)
        .expect("Could not find variable 'longitude'");

    let time_var = &nc_file
        .variable(time_name)
        .expect("Could not find variable 'longitude'");

    let variables = nc_file
        .variables()
        .map(|var| var.name().to_owned())
        .collect::<Vec<String>>();

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

struct NetCdfInputHandler {
    records: Vec<NetCdfFileInputRecord>,
}

impl NetCdfInputHandler {
    pub fn new(path: &str, lats: &[f32], lons: &[f32]) -> Result<Self, Box<dyn Error>> {
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
            match register_nc_file(&file_path_str, "latitude", "longitude", "time", lats, lons) {
                Ok(record) => records.push(record),
                Err(e) => warn!("Error inspecting file {}: {}", file_path_str, e),
            }
        }

        Ok(NetCdfInputHandler { records })
    }
}

impl InputHandler for NetCdfInputHandler {
    fn get_values(&self, var: &str, date: &DateTime<Utc>) -> Option<Array1<f32>> {
        for record in &self.records {
            let time_index = record.timeline.iter().position(|t| t == date);

            if time_index.is_some() && record.variables.contains(&var.to_string()) {
                unimplemented!("need to implement projection to the grid");
            }
        }
        None
    }

    fn get_timeline(&self) -> Vec<DateTime<Utc>> {
        self.records
            .iter()
            .flat_map(|record| record.timeline.iter())
            .cloned()
            .collect()
    }

    fn get_variables(&self, time: &DateTime<Utc>) -> Vec<String> {
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
