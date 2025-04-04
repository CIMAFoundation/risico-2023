use chrono::Utc;
#[cfg(feature = "gdal")]
#[cfg(feature = "gdal")]
use gdal::raster::{Buffer, RasterCreationOption};

use libflate::gzip::{self, Encoder};
use log::warn;
use netcdf::extent::Extents;
use risico::constants::NODATAVAL;
use risico::version::FULL_VERSION;

use std::io::BufWriter;
use std::path::Path;

use std::{
    fs::File,
    io::{self, Write},
};

use crate::common::helpers::RISICOError;

use strum::EnumProperty;

use super::models::{grid::RegularGrid, palette::Palette};

pub fn write_and_check(
    encoder: &mut Encoder<BufWriter<File>>,
    buf: &[u8],
    ok: &mut bool,
) -> Result<(), io::Error> {
    let written = encoder.write(buf)?;
    if written < buf.len() {
        *ok = false;
    }
    Ok(())
}

pub fn write_to_zbin_file(file: &str, grid: &RegularGrid, values: &[f32]) -> Result<(), io::Error> {
    let output = File::create(file)?;

    let output = io::BufWriter::new(output);
    let mut encoder = gzip::Encoder::new(output)?;
    let mut ok = true;

    let buf = [1u8, 0u8, 0u8, 0u8];
    write_and_check(&mut encoder, &buf, &mut ok)?;
    let nrows = grid.nrows as u32;
    let ncols = grid.ncols as u32;
    let buf = nrows.to_le_bytes();
    write_and_check(&mut encoder, &buf, &mut ok)?;
    let buf = ncols.to_le_bytes();
    write_and_check(&mut encoder, &buf, &mut ok)?;

    let buf = grid.min_lat.to_le_bytes();
    write_and_check(&mut encoder, &buf, &mut ok)?;
    let buf = grid.max_lat.to_le_bytes();
    write_and_check(&mut encoder, &buf, &mut ok)?;

    let buf = grid.min_lon.to_le_bytes();
    write_and_check(&mut encoder, &buf, &mut ok)?;

    let buf = grid.max_lon.to_le_bytes();
    write_and_check(&mut encoder, &buf, &mut ok)?;

    let mut buf = Vec::<u8>::new();

    for index in 0..nrows * ncols {
        let index = index as usize;
        let val = values[index];
        buf.extend(val.to_le_bytes());
    }
    write_and_check(&mut encoder, &buf, &mut ok)?;
    encoder.finish();

    if !ok {
        warn!("Problems writing file {}", file);
    }

    Ok(())
}

pub fn write_to_pngwjson(
    file: &str,
    grid: &RegularGrid,
    values: &[f32],
    palette: &Palette,
) -> Result<(), io::Error> {
    let output = File::create(file)?;

    let output = io::BufWriter::new(output);

    let w = &mut BufWriter::new(output);

    let nrows = grid.nrows as u32;
    let ncols = grid.ncols as u32;

    let mut encoder = png::Encoder::new(w, ncols, nrows); // Width is 2 pixels and height is 1.
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::NoFilter);

    let mut writer = encoder.write_header()?;

    let mut data: Vec<u8> = Vec::new();
    for row in (0..nrows).rev() {
        for col in 0..ncols {
            let index = (row * ncols + col) as usize;
            let val = values[index];
            let color = palette.get_color(val);
            let pixel_data = [color.r, color.g, color.b, color.a];
            for value in pixel_data {
                data.push(value);
            }
        }
    }
    writer.write_image_data(&data)?;

    // create a new file name replacing the extension .png with .json
    let path = Path::new(file);
    let base_path = path.parent().expect("should have a parent");

    let file_name = path.file_stem().expect("should have a file name");
    let json_file = format!(
        "{}/{}.json",
        base_path.to_str().expect("should have a path"),
        file_name.to_str().expect("should have a file name")
    );

    //convert last comment to valid rust
    let json = format!(
        "{{\n  
            \"west\":{},\n  
            \"east\":{},\n  
            \"south\":{},\n  
            \"north\":{} 
    \n}}",
        grid.min_lon, grid.max_lon, grid.min_lat, grid.max_lat
    );
    let mut json_file = File::create(json_file)?;
    json_file.write_all(json.as_bytes())?;

    Ok(())
}

#[cfg(feature = "gdal")]
pub fn write_to_geotiff(
    file: &str,
    grid: &RegularGrid,
    values: &[f32],
) -> Result<(), gdal::errors::GdalError> {
    // Open a GDAL driver for GeoTIFF files
    let driver = gdal::DriverManager::get_driver_by_name(&"GTiff")?;

    let options = vec![
        RasterCreationOption {
            key: "COMPRESS",
            value: "LZW",
        },
        RasterCreationOption {
            key: "PROFILE",
            value: "GDALGeoTIFF",
        },
        RasterCreationOption {
            key: "BIGTIFF",
            value: "YES",
        },
    ];
    let mut dataset = driver.create_with_band_type_with_options::<f32, &str>(
        file,
        grid.ncols as isize,
        grid.nrows as isize,
        1,
        &options,
    )?;

    // Set the geo-transform for the dataset
    let geo_transform = [
        grid.min_lon as f64,
        grid.step_lon as f64,
        0.0,
        grid.max_lat as f64,
        0.0,
        -grid.step_lat as f64,
    ];
    dataset.set_geo_transform(&geo_transform)?;

    // Set the Coordinate Reference System for the dataset
    let proj_wkt = "+proj=longlat +datum=WGS84 +no_defs";
    dataset.set_projection(proj_wkt)?;

    // Get a reference to the first band
    let mut band = dataset.rasterband(1)?;
    band.set_no_data_value(Some(NODATAVAL.into()))?;

    // Create a buffer with some data to write to the band
    let mut data = vec![];
    for row in (0..grid.nrows).rev() {
        for col in 0..grid.ncols {
            let index = (row * grid.ncols + col) as usize;
            let val = values[index] as f32;
            data.push(val);
        }
    }
    let size = (grid.ncols, grid.nrows);
    let buffer = Buffer::new(size, data);

    // Write the data to the band
    band.write((0, 0), size, &buffer)?;

    Ok(())
}

const COMPRESSION_RATE: i32 = 7;

pub fn create_nc_file<T>(
    file_name: &str,
    grid: &RegularGrid,
    output_name: &str,
    variable_name: T,
) -> Result<netcdf::MutableFile, RISICOError>
where
    T: EnumProperty + ToString,
{
    let n_lats = grid.nrows;
    let n_lons = grid.ncols;

    let options = netcdf::Options::NETCDF4;

    let mut file = netcdf::create_with(file_name, options)
        .map_err(|err| format!("can't create file {file_name}: {err}"))?;

    file.add_attribute("risico_version", FULL_VERSION)
        .expect("Should add attribute 'risico_version'");

    file.add_attribute("creation_date", Utc::now().to_rfc3339())
        .expect("Should add attribute 'creation_date'");

    file.add_attribute("missing_value", NODATAVAL)
        .expect("Should add attribute");

    // We must create a dimension which corresponds to our data
    file.add_dimension("latitude", n_lats)
        .map_err(|err| format!("Add latitude dimension failed {err}"))?;
    file.add_dimension("longitude", n_lons)
        .map_err(|err| format!("Add longitude dimension failed {err}"))?;

    file.add_unlimited_dimension("time")
        .map_err(|err| format!("Add time dimension failed {err}"))?;
    let lats: Vec<f32> = (0..n_lats)
        .map(|i| grid.min_lat + (grid.max_lat - grid.min_lat) * (i as f32) / (grid.nrows as f32))
        .collect();
    let lons: Vec<f32> = (0..n_lons)
        .map(|i| grid.min_lon + (grid.max_lon - grid.min_lon) * (i as f32) / (grid.ncols as f32))
        .collect();

    let mut var = file
        .add_variable::<f32>("latitude", &["latitude"])
        .expect("Add latitude failed");

    var.put_values(&lats, Extents::All)
        .expect("Add longitude failed");

    let mut var = file
        .add_variable::<f32>("longitude", &["longitude"])
        .expect("Add longitude failed");

    var.put_values(&lons, Extents::All)
        .expect("Add longitude failed");

    let mut time_var = file
        .add_variable::<i64>("time", &["time"])
        .expect("Add time failed");

    time_var
        .add_attribute("units", "seconds since 1970-01-01 00:00:00.0")
        .unwrap_or_else(|_| panic!("Add time units failed"));
    time_var
        .add_attribute("long_name", "time")
        .unwrap_or_else(|_| panic!("Add time units failed"));
    time_var
        .add_attribute("calendar", "proleptic_gregorian")
        .unwrap_or_else(|_| panic!("Add time units failed"));

    let mut variable_var = file
        .add_variable::<f32>(output_name, &["time", "latitude", "longitude"])
        .unwrap_or_else(|_| panic!("Add {} failed", output_name));

    variable_var
        .compression(COMPRESSION_RATE, true)
        .expect("Set compression failed");

    variable_var
        .add_attribute("missing_value", NODATAVAL)
        .expect("Should add attribute");

    variable_var
        .add_attribute("units", variable_name.get_str("units").unwrap_or("unknown"))
        .expect("Should add attribute");

    variable_var
        .add_attribute(
            "long_name",
            variable_name
                .get_str("long_name")
                .unwrap_or(&variable_name.to_string()),
        )
        .expect("Should add attribute");

    Ok(file)
}
