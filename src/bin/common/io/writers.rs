use chrono::{DateTime, Utc};
#[cfg(feature = "gdal")]
use gdal::raster::{Buffer, RasterCreationOptions};
#[cfg(feature = "gdal")]
use gdal::spatial_ref::SpatialRef;

use libflate::gzip::{self, Encoder};
use log::warn;
use nc4_stream::{AttrValue, DataType, EncodedRecord, Nc4Writer, VarId, VariableOptions};
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

    let options =
        RasterCreationOptions::from_iter(["COMPRESS=LZW", "PROFILE=GDALGeoTIFF", "BIGTIFF=YES"]);
    let mut dataset = driver.create_with_band_type_with_options::<f32, &str>(
        file, grid.ncols, grid.nrows, 1, &options,
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
    let mut buffer = Buffer::new(size, data);

    // Write the data to the band
    band.write((0, 0), size, &mut buffer)?;

    Ok(())
}

/// Write an already north-up regular grid through GDAL.
///
/// This is used by offline conversion tools. Runtime static-data loading does
/// not depend on GDAL.
#[cfg(feature = "gdal")]
#[allow(dead_code)]
pub fn write_north_up_geotiff(
    file: &str,
    width: usize,
    height: usize,
    transform: &[f64; 6],
    epsg: u32,
    values: &[f32],
    nodata: f32,
) -> Result<(), RISICOError> {
    if values.len() != width * height {
        return Err(format!(
            "cannot write {file}: {} samples for a {width}x{height} grid",
            values.len()
        )
        .into());
    }

    let driver = gdal::DriverManager::get_driver_by_name("GTiff")
        .map_err(|error| format!("cannot load GDAL GTiff driver: {error}"))?;
    let options = RasterCreationOptions::from_iter([
        "COMPRESS=DEFLATE",
        "PREDICTOR=3",
        "TILED=YES",
        "BIGTIFF=IF_SAFER",
    ]);
    let mut dataset = driver
        .create_with_band_type_with_options::<f32, _>(file, width, height, 1, &options)
        .map_err(|error| format!("cannot create GeoTIFF {file}: {error}"))?;
    dataset
        .set_geo_transform(transform)
        .map_err(|error| format!("cannot georeference {file}: {error}"))?;
    let spatial_reference = SpatialRef::from_epsg(epsg)
        .map_err(|error| format!("cannot create EPSG:{epsg} reference: {error}"))?;
    dataset
        .set_spatial_ref(&spatial_reference)
        .map_err(|error| format!("cannot set CRS on {file}: {error}"))?;
    let mut band = dataset
        .rasterband(1)
        .map_err(|error| format!("cannot access band in {file}: {error}"))?;
    band.set_no_data_value(Some(nodata as f64))
        .map_err(|error| format!("cannot set nodata on {file}: {error}"))?;
    let mut buffer = Buffer::new((width, height), values.to_vec());
    band.write((0, 0), (width, height), &mut buffer)
        .map_err(|error| format!("cannot write pixels to {file}: {error}"))?;
    dataset
        .flush_cache()
        .map_err(|error| format!("cannot flush {file}: {error}"))?;
    Ok(())
}

/// One NetCDF-4 output file of the model: the streaming writer plus the two
/// variables a timestep touches, the data grid and its timestamp.
pub struct Nc4Output {
    writer: Option<Nc4Writer>,
    data: VarId,
    time: VarId,
}

impl Nc4Output {
    /// Compress one grid. Needs only the file's definition, so callers run
    /// it without exclusive access to the file.
    pub fn encode(&self, values: &[f32]) -> Result<EncodedRecord, RISICOError> {
        self.writer()?
            .encode_record(self.data, values)
            .map_err(nc4_error)
    }

    /// Append a compressed grid and its timestamp as the next record.
    pub fn append(&mut self, record: EncodedRecord, time: DateTime<Utc>) -> Result<(), RISICOError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| RISICOError::from("NetCDF output is already finished".to_string()))?;
        writer
            .append_encoded_record(self.data, record)
            .map_err(nc4_error)?;
        writer
            .append_record(self.time, &[time.timestamp()])
            .map_err(nc4_error)
    }

    /// Write the metadata and close the file. A second call does nothing.
    pub fn finish(&mut self) -> Result<(), RISICOError> {
        match self.writer.take() {
            Some(writer) => writer.finish().map_err(nc4_error),
            None => Ok(()),
        }
    }

    fn writer(&self) -> Result<&Nc4Writer, RISICOError> {
        self.writer
            .as_ref()
            .ok_or_else(|| RISICOError::from("NetCDF output is already finished".to_string()))
    }
}

fn nc4_error(error: nc4_stream::Error) -> RISICOError {
    RISICOError::from(error.to_string())
}

/// Create the NetCDF-4 file of one output variable: latitude and longitude
/// axes, an unlimited time axis, and the variable chunked one timestep at a
/// time with shuffle and deflate at `compression_level` (0 disables both).
pub fn create_nc4_file<T>(
    file_name: &Path,
    grid: &RegularGrid,
    output_name: &str,
    variable_name: T,
    compression_level: i32,
) -> Result<Nc4Output, RISICOError>
where
    T: EnumProperty + ToString,
{
    if !(0..=9).contains(&compression_level) {
        return Err(format!(
            "NetCDF compression level must be between 0 and 9, got {compression_level}"
        )
        .into());
    }
    let describe = |error: nc4_stream::Error| format!("{}: {error}", file_name.display());

    let mut file = Nc4Writer::create(file_name).map_err(describe)?;
    file.put_attr("risico_version", FULL_VERSION)
        .map_err(describe)?;
    file.put_attr("creation_date", Utc::now().to_rfc3339())
        .map_err(describe)?;
    file.put_attr("missing_value", AttrValue::F32(NODATAVAL))
        .map_err(describe)?;

    let latitude = file
        .add_dimension("latitude", grid.nrows)
        .map_err(describe)?;
    let longitude = file
        .add_dimension("longitude", grid.ncols)
        .map_err(describe)?;
    let time = file.add_unlimited_dimension("time").map_err(describe)?;

    let lats: Vec<f32> = (0..grid.nrows)
        .map(|i| grid.min_lat + grid.step_lat * (i as f32))
        .collect();
    let lons: Vec<f32> = (0..grid.ncols)
        .map(|i| grid.min_lon + grid.step_lon * (i as f32))
        .collect();
    let latitude_var = file
        .add_variable(
            "latitude",
            DataType::F32,
            &[latitude],
            VariableOptions::contiguous(),
        )
        .map_err(describe)?;
    file.put_values(latitude_var, &lats).map_err(describe)?;
    let longitude_var = file
        .add_variable(
            "longitude",
            DataType::F32,
            &[longitude],
            VariableOptions::contiguous(),
        )
        .map_err(describe)?;
    file.put_values(longitude_var, &lons).map_err(describe)?;

    let time_var = file
        .add_variable(
            "time",
            DataType::I64,
            &[time],
            VariableOptions::chunked(vec![512]),
        )
        .map_err(describe)?;
    file.put_var_attr(time_var, "units", "seconds since 1970-01-01 00:00:00.0")
        .map_err(describe)?;
    file.put_var_attr(time_var, "long_name", "time")
        .map_err(describe)?;
    file.put_var_attr(time_var, "calendar", "proleptic_gregorian")
        .map_err(describe)?;

    // One timestep per chunk, which is how the file is written and read.
    let mut options = VariableOptions::chunked(vec![1, grid.nrows, grid.ncols]);
    if compression_level > 0 {
        options = options.shuffle(true).deflate(compression_level as u32);
    }
    let data = file
        .add_variable(
            output_name,
            DataType::F32,
            &[time, latitude, longitude],
            options,
        )
        .map_err(describe)?;
    file.put_var_attr(data, "missing_value", AttrValue::F32(NODATAVAL))
        .map_err(describe)?;
    file.put_var_attr(
        data,
        "units",
        variable_name.get_str("units").unwrap_or("unknown"),
    )
    .map_err(describe)?;
    file.put_var_attr(
        data,
        "long_name",
        variable_name
            .get_str("long_name")
            .unwrap_or(&variable_name.to_string()),
    )
    .map_err(describe)?;

    Ok(Nc4Output {
        writer: Some(file),
        data,
        time: time_var,
    })
}

