use chrono::{DateTime, Utc};
use geotiff_ndarray::ArrayView2;
use geotiff_writer::{Compression, GeoTiffBuilder, GeoTransform, Predictor, TiffVariant};

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

/// Write a model output grid as a single-band EPSG:4326 GeoTIFF.
///
/// `values` are stored south row first, as the model grids are; the file is
/// written north-up.
pub fn write_to_geotiff(file: &str, grid: &RegularGrid, values: &[f32]) -> Result<(), RISICOError> {
    let (width, height) = (grid.ncols, grid.nrows);
    if values.len() != width * height {
        return Err(format!(
            "cannot write {file}: {} samples for a {width}x{height} grid",
            values.len()
        )
        .into());
    }
    let north_up: Vec<f32> = values
        .chunks_exact(width)
        .rev()
        .flatten()
        .copied()
        .collect();
    let transform = [
        grid.min_lon as f64,
        grid.step_lon as f64,
        0.0,
        grid.max_lat as f64,
        0.0,
        -grid.step_lat as f64,
    ];
    let builder = georeferenced_builder(file, width, height, &transform, 4326, NODATAVAL)?
        .compression(Compression::Lzw)
        .tiff_variant(TiffVariant::BigTiff);
    write_band(file, &builder, width, height, &north_up)
}

/// Write an already north-up regular grid as a single-band GeoTIFF.
///
/// Used by the offline static-data converter.
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
    let builder = georeferenced_builder(file, width, height, transform, epsg, nodata)?
        .compression(Compression::Deflate)
        .predictor(Predictor::FloatingPoint)
        .tile_size(256, 256)
        .tiff_variant(TiffVariant::Auto);
    write_band(file, &builder, width, height, values)
}

/// A single-band builder with the CRS, a corner-based affine transform and
/// the nodata value set.
fn georeferenced_builder(
    file: &str,
    width: usize,
    height: usize,
    transform: &[f64; 6],
    epsg: u32,
    nodata: f32,
) -> Result<GeoTiffBuilder, RISICOError> {
    let width = u32::try_from(width).map_err(|_| format!("cannot write {file}: too many columns"))?;
    let height = u32::try_from(height).map_err(|_| format!("cannot write {file}: too many rows"))?;
    let epsg = u16::try_from(epsg).map_err(|_| format!("cannot write {file}: invalid EPSG:{epsg}"))?;
    let [origin_x, pixel_width, skew_x, origin_y, skew_y, pixel_height] = *transform;
    Ok(GeoTiffBuilder::new(width, height)
        .epsg(epsg)
        .transform(GeoTransform {
            origin_x,
            pixel_width,
            skew_x,
            origin_y,
            skew_y,
            pixel_height,
        })
        .nodata(&nodata.to_string()))
}

fn write_band(
    file: &str,
    builder: &GeoTiffBuilder,
    width: usize,
    height: usize,
    values: &[f32],
) -> Result<(), RISICOError> {
    let data = ArrayView2::from_shape((height, width), values)
        .map_err(|error| format!("cannot write {file}: {error}"))?;
    builder
        .write_2d(file, data)
        .map_err(|error| format!("cannot write GeoTIFF {file}: {error}").into())
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


#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::common::io::static_data::geotiff::{RasterLayerReader, STATIC_NODATA};
    use crate::common::io::streaming::GridWindow;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("risico-geotiff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn read_all(reader: &RasterLayerReader) -> Vec<f32> {
        reader
            .read_window(GridWindow {
                row: 0,
                col: 0,
                height: reader.grid.height,
                width: reader.grid.width,
            })
            .unwrap()
    }

    #[test]
    fn north_up_geotiff_round_trips() {
        // Not a multiple of the 256-pixel tile, so edge tiles are partial.
        let (width, height) = (300, 270);
        let transform = [6.5, 0.01, 0.0, 47.25, 0.0, -0.02];
        let mut values: Vec<f32> = (0..width * height).map(|i| i as f32 * 0.25).collect();
        values[17] = STATIC_NODATA;
        let path = scratch("north-up.tif");

        write_north_up_geotiff(
            path.to_str().unwrap(),
            width,
            height,
            &transform,
            4326,
            &values,
            STATIC_NODATA,
        )
        .unwrap();

        let reader = RasterLayerReader::open(&path).unwrap();
        assert_eq!((reader.grid.width, reader.grid.height), (width, height));
        assert_eq!(reader.grid.epsg, 4326);
        for (read, expected) in reader.grid.transform.iter().zip(transform) {
            assert!((read - expected).abs() < 1.0e-12, "{read} != {expected}");
        }
        assert!(reader.is_nodata(STATIC_NODATA));
        assert_eq!(read_all(&reader), values);
    }

    #[test]
    fn output_geotiff_is_written_north_up() {
        let grid = RegularGrid::new(3, 4, 40.0, 10.0, 41.0, 11.5);
        // South row first, as the model stores its grids.
        let values: Vec<f32> = (0..12).map(|i| i as f32).collect();
        let path = scratch("output.tif");

        write_to_geotiff(path.to_str().unwrap(), &grid, &values).unwrap();

        let reader = RasterLayerReader::open(&path).unwrap();
        assert_eq!((reader.grid.width, reader.grid.height), (4, 3));
        assert_eq!(reader.grid.epsg, 4326);
        let expected = [
            grid.min_lon as f64,
            grid.step_lon as f64,
            0.0,
            grid.max_lat as f64,
            0.0,
            -grid.step_lat as f64,
        ];
        for (read, expected) in reader.grid.transform.iter().zip(expected) {
            assert!((read - expected).abs() < 1.0e-9, "{read} != {expected}");
        }
        assert!(reader.is_nodata(NODATAVAL));
        let north_first: Vec<f32> = [8., 9., 10., 11., 4., 5., 6., 7., 0., 1., 2., 3.].to_vec();
        assert_eq!(read_all(&reader), north_first);
    }
}
