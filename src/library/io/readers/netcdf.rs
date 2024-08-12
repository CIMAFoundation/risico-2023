use std::error::Error;

use chrono::{DateTime, Utc};
use log::warn;
use ndarray::Array1;
use netcdf::extent::Extents;
use std::fs;

pub struct NetCdfFileInputRecord {
    file: String,
    timeline: Array1<DateTime<Utc>>,
    variables: Array1<String>,
    lats: Array1<f32>,
    lons: Array1<f32>,
}

/// inspect a single netcdf file and builds a record
fn inspect_nc_file(
    file: &str,
    lat_name: &str,
    lon_name: &str,
    time_name: &str,
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
        .collect::<Array1<String>>();

    let timeline = time_var
        .values::<i64, _>(Extents::All)?
        .into_iter()
        .map(|t| DateTime::from_timestamp_nanos(t).to_utc())
        .collect::<Array1<DateTime<Utc>>>();

    let lats = lats_var
        .values::<f32, _>(Extents::All)?
        .into_iter()
        .collect::<Array1<f32>>();

    let lons = lons_var
        .values::<f32, _>(Extents::All)?
        .into_iter()
        .collect::<Array1<f32>>();

    let record = NetCdfFileInputRecord {
        file: file.to_owned(),
        timeline,
        variables,
        lats,
        lons,
    };

    Ok(record)
}

pub fn inspect_directory(path: &str) -> Result<Vec<NetCdfFileInputRecord>, Box<dyn Error>> {
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
        match inspect_nc_file(&file_path_str, "latitude", "longitude", "time") {
            Ok(record) => records.push(record),
            Err(e) => warn!("Error inspecting file {}: {}", file_path_str, e),
        }
    }

    Ok(records)
}
