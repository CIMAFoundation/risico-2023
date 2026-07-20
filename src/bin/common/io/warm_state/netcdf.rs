use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use log::warn;
use netcdf::{extent::Extents, File};
use risico::modules::fwi::{
    constants::{DC_INIT, DMC_INIT, FFMC_INIT, NODATAVAL},
    models::{FWIState, FWIWarmState},
};
use risico::modules::risico::models::{RISICOState, RISICOWarmState};

use crate::common::helpers::RISICOError;
use crate::common::io::static_data::geotiff::RasterGrid;

const STATE_SCHEMA_VERSION: i32 = 1;
const COMPRESSION_LEVEL: i32 = 4;
const FLOAT_FILL: f32 = f32::NAN;
const TIME_FILL: i64 = i64::MIN;

pub type RisicoWarmSnapshot = (Vec<RISICOWarmState>, DateTime<Utc>);
pub type FwiWarmSnapshot = (Vec<FWIWarmState>, DateTime<Utc>);

pub fn load_latest_risico(
    directory: impl AsRef<Path>,
    latest_time: DateTime<Utc>,
    max_age_hours: i64,
    model_version: &str,
    target_grid: &RasterGrid,
    target_cell_indexes: &[u32],
) -> Result<Option<RisicoWarmSnapshot>, RISICOError> {
    load_latest(directory, latest_time, max_age_hours, |path| {
        read_risico_snapshot(path, model_version, target_grid, target_cell_indexes)
    })
}

pub fn write_risico_snapshot(
    directory: impl AsRef<Path>,
    state: &RISICOState,
    model_version: &str,
    grid: &RasterGrid,
    cell_indexes: &[u32],
) -> Result<PathBuf, RISICOError> {
    validate_state_cells("RISICO", state.data.len(), grid, cell_indexes)?;
    publish_snapshot(directory, "RISICO", state.time, |path| {
        let mut file = create_grid_file(path, "RISICO", model_version, state.time, grid)?;
        file.add_attribute("active_cell_count", cell_indexes.len() as u64)
            .map_err(netcdf_error)?;
        write_active_mask(&mut file, grid, cell_indexes)?;

        let fields: [(&str, Vec<f32>); 9] = [
            ("dffm", state.data.iter().map(|v| v.dffm).collect()),
            (
                "snow_cover",
                state.data.iter().map(|v| v.snow_cover).collect(),
            ),
            (
                "snow_cover_time",
                state.data.iter().map(|v| v.snow_cover_time).collect(),
            ),
            ("msi", state.data.iter().map(|v| v.MSI).collect()),
            ("msi_ttl", state.data.iter().map(|v| v.MSI_TTL).collect()),
            ("ndvi", state.data.iter().map(|v| v.NDVI).collect()),
            (
                "ndvi_time",
                state.data.iter().map(|v| v.NDVI_TIME).collect(),
            ),
            ("ndwi", state.data.iter().map(|v| v.NDWI).collect()),
            (
                "ndwi_time",
                state.data.iter().map(|v| v.NDWI_TIME).collect(),
            ),
        ];
        for (name, values) in fields {
            write_active_grid(&mut file, name, grid, cell_indexes, &values)?;
        }
        Ok(())
    })
}

fn read_risico_snapshot(
    path: &Path,
    model_version: &str,
    target_grid: &RasterGrid,
    target_cell_indexes: &[u32],
) -> Result<Vec<RISICOWarmState>, RISICOError> {
    let file = open_grid_file(path, "RISICO", model_version)?;
    let source_grid = grid_from_file(&file)?;
    let source_indexes = sampling_indexes(&source_grid, target_grid, target_cell_indexes)?;
    validate_active_samples(&file, &source_grid, &source_indexes)?;
    let dffm = sample_float_grid(&file, "dffm", &source_grid, &source_indexes)?;
    let snow_cover = sample_float_grid(&file, "snow_cover", &source_grid, &source_indexes)?;
    let snow_cover_time =
        sample_float_grid(&file, "snow_cover_time", &source_grid, &source_indexes)?;
    let msi = sample_float_grid(&file, "msi", &source_grid, &source_indexes)?;
    let msi_ttl = sample_float_grid(&file, "msi_ttl", &source_grid, &source_indexes)?;
    let ndvi = sample_float_grid(&file, "ndvi", &source_grid, &source_indexes)?;
    let ndvi_time = sample_float_grid(&file, "ndvi_time", &source_grid, &source_indexes)?;
    let ndwi = sample_float_grid(&file, "ndwi", &source_grid, &source_indexes)?;
    let ndwi_time = sample_float_grid(&file, "ndwi_time", &source_grid, &source_indexes)?;

    Ok((0..target_cell_indexes.len())
        .map(|i| RISICOWarmState {
            dffm: dffm[i],
            snow_cover: snow_cover[i],
            snow_cover_time: snow_cover_time[i],
            MSI: msi[i],
            MSI_TTL: msi_ttl[i],
            NDVI: ndvi[i],
            NDVI_TIME: ndvi_time[i],
            NDWI: ndwi[i],
            NDWI_TIME: ndwi_time[i],
        })
        .collect())
}

pub fn load_latest_fwi(
    directory: impl AsRef<Path>,
    latest_time: DateTime<Utc>,
    max_age_hours: i64,
    model_version: &str,
    target_grid: &RasterGrid,
    target_cell_indexes: &[u32],
) -> Result<Option<FwiWarmSnapshot>, RISICOError> {
    load_latest(directory, latest_time, max_age_hours, |path| {
        read_fwi_snapshot(path, model_version, target_grid, target_cell_indexes)
    })
}

pub fn write_fwi_snapshot(
    directory: impl AsRef<Path>,
    state: &FWIState,
    model_version: &str,
    grid: &RasterGrid,
    cell_indexes: &[u32],
) -> Result<PathBuf, RISICOError> {
    validate_state_cells("FWI", state.data.len(), grid, cell_indexes)?;
    if model_version != "legacy" {
        for (index, cell) in state.data.iter().enumerate() {
            let length = cell.dates.len();
            if [
                cell.ffmc.len(),
                cell.dmc.len(),
                cell.dc.len(),
                cell.rain.len(),
            ]
            .iter()
            .any(|candidate| *candidate != length)
            {
                return Err(format!(
                    "cannot write FWI warm state: cell {index} history arrays have different lengths"
                )
                .into());
            }
        }
    }

    publish_snapshot(directory, "FWI", state.time, |path| {
        let mut file = create_grid_file(path, "FWI", model_version, state.time, grid)?;
        file.add_attribute("active_cell_count", cell_indexes.len() as u64)
            .map_err(netcdf_error)?;
        file.add_attribute("history_layout", "dense_grid_v1")
            .map_err(netcdf_error)?;
        write_active_mask(&mut file, grid, cell_indexes)?;

        let history_count = state
            .data
            .iter()
            .map(|cell| {
                if model_version == "legacy" {
                    usize::from(
                        !cell.ffmc.is_empty() || !cell.dmc.is_empty() || !cell.dc.is_empty(),
                    )
                } else {
                    cell.dates.len()
                }
            })
            .max()
            .unwrap_or(0)
            .max(1);
        file.add_dimension("history", history_count)
            .map_err(netcdf_error)?;

        let grid_len = grid.width * grid.height;
        let mut counts = vec![0_u32; grid_len];
        let mut times = vec![TIME_FILL; history_count * grid_len];
        let mut ffmc = vec![FLOAT_FILL; history_count * grid_len];
        let mut dmc = vec![FLOAT_FILL; history_count * grid_len];
        let mut dc = vec![FLOAT_FILL; history_count * grid_len];
        let mut rain = vec![FLOAT_FILL; history_count * grid_len];

        for (&cell_index, cell) in cell_indexes.iter().zip(&state.data) {
            let spatial = cell_index as usize;
            if model_version == "legacy" {
                let has_state =
                    !cell.ffmc.is_empty() || !cell.dmc.is_empty() || !cell.dc.is_empty();
                counts[spatial] = u32::from(has_state);
                if has_state {
                    times[spatial] = state.time.timestamp();
                    ffmc[spatial] = cell.ffmc.first().copied().unwrap_or(FFMC_INIT);
                    dmc[spatial] = cell.dmc.first().copied().unwrap_or(DMC_INIT);
                    dc[spatial] = cell.dc.first().copied().unwrap_or(DC_INIT);
                    rain[spatial] = cell.rain.last().copied().unwrap_or(NODATAVAL);
                }
            } else {
                counts[spatial] = cell.dates.len() as u32;
                for history in 0..cell.dates.len() {
                    let index = history * grid_len + spatial;
                    times[index] = cell.dates[history].timestamp();
                    ffmc[index] = cell.ffmc[history];
                    dmc[index] = cell.dmc[history];
                    dc[index] = cell.dc[history];
                    rain[index] = cell.rain[history];
                }
            }
        }

        write_compressed_variable(&mut file, "history_count", &["y", "x"], &counts, None)?;
        write_compressed_variable(
            &mut file,
            "history_time",
            &["history", "y", "x"],
            &times,
            Some(TIME_FILL),
        )?;
        for (name, values) in [("ffmc", ffmc), ("dmc", dmc), ("dc", dc), ("rain", rain)] {
            write_compressed_variable(
                &mut file,
                name,
                &["history", "y", "x"],
                &values,
                Some(FLOAT_FILL),
            )?;
        }
        Ok(())
    })
}

fn read_fwi_snapshot(
    path: &Path,
    model_version: &str,
    target_grid: &RasterGrid,
    target_cell_indexes: &[u32],
) -> Result<Vec<FWIWarmState>, RISICOError> {
    let file = open_grid_file(path, "FWI", model_version)?;
    if string_attribute(&file, "history_layout")? != "dense_grid_v1" {
        return Err("unsupported FWI history layout".into());
    }
    let source_grid = grid_from_file(&file)?;
    let source_indexes = sampling_indexes(&source_grid, target_grid, target_cell_indexes)?;
    validate_active_samples(&file, &source_grid, &source_indexes)?;
    let counts = read_variable::<u32>(&file, "history_count")?;
    validate_grid_len("history_count", counts.len(), &source_grid)?;
    let history_len = file
        .dimension("history")
        .ok_or("missing NetCDF dimension history")?
        .len();
    let grid_len = source_grid.width * source_grid.height;
    let expected_len = history_len * grid_len;
    let times = read_variable::<i64>(&file, "history_time")?;
    let ffmc = read_variable::<f32>(&file, "ffmc")?;
    let dmc = read_variable::<f32>(&file, "dmc")?;
    let dc = read_variable::<f32>(&file, "dc")?;
    let rain = read_variable::<f32>(&file, "rain")?;
    for (name, len) in [
        ("history_time", times.len()),
        ("ffmc", ffmc.len()),
        ("dmc", dmc.len()),
        ("dc", dc.len()),
        ("rain", rain.len()),
    ] {
        if len != expected_len {
            return Err(format!(
                "snapshot variable {name} has {len} values, expected {expected_len}"
            )
            .into());
        }
    }

    source_indexes
        .into_iter()
        .enumerate()
        .map(|(target, source)| {
            let count = counts[source] as usize;
            if count > history_len {
                return Err(format!(
                    "FWI history count {count} exceeds {history_len} at target cell {target}"
                )
                .into());
            }
            let mut dates = Vec::with_capacity(count);
            let mut cell_ffmc = Vec::with_capacity(count);
            let mut cell_dmc = Vec::with_capacity(count);
            let mut cell_dc = Vec::with_capacity(count);
            let mut cell_rain = Vec::with_capacity(count);
            for history in 0..count {
                let index = history * grid_len + source;
                let timestamp = times[index];
                let values = [ffmc[index], dmc[index], dc[index], rain[index]];
                if timestamp == TIME_FILL || values.iter().any(|value| is_fill(*value)) {
                    return Err(format!(
                        "FWI snapshot has fill data at target cell {target}, history {history}"
                    )
                    .into());
                }
                dates.push(DateTime::from_timestamp(timestamp, 0).ok_or_else(|| {
                    RISICOError::from(format!(
                        "invalid history timestamp {timestamp} at target cell {target}"
                    ))
                })?);
                cell_ffmc.push(values[0]);
                cell_dmc.push(values[1]);
                cell_dc.push(values[2]);
                cell_rain.push(values[3]);
            }
            Ok(FWIWarmState {
                dates,
                ffmc: cell_ffmc,
                dmc: cell_dmc,
                dc: cell_dc,
                rain: cell_rain,
            })
        })
        .collect()
}

fn load_latest<T>(
    directory: impl AsRef<Path>,
    latest_time: DateTime<Utc>,
    max_age_hours: i64,
    read: impl Fn(&Path) -> Result<Vec<T>, RISICOError>,
) -> Result<Option<(Vec<T>, DateTime<Utc>)>, RISICOError> {
    let directory = directory.as_ref();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot read warm-state directory {}: {error}",
                directory.display()
            )
            .into())
        }
    };
    let oldest = latest_time - Duration::hours(max_age_hours);
    let mut candidates: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "nc"))
        .filter_map(|path| match snapshot_time(&path) {
            Ok(time) if time <= latest_time && time >= oldest => Some((time, path)),
            Ok(_) => None,
            Err(error) => {
                warn!("Ignoring warm-state candidate {}: {error}", path.display());
                None
            }
        })
        .collect();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    for (time, path) in candidates {
        match read(&path) {
            Ok(state) => return Ok(Some((state, time))),
            Err(error) => warn!("Ignoring invalid warm state {}: {error}", path.display()),
        }
    }
    Ok(None)
}

fn publish_snapshot(
    directory: impl AsRef<Path>,
    model: &str,
    time: DateTime<Utc>,
    write: impl FnOnce(&Path) -> Result<(), RISICOError>,
) -> Result<PathBuf, RISICOError> {
    let directory = directory.as_ref();
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "cannot create warm-state directory {}: {error}",
            directory.display()
        )
    })?;
    let destination = directory.join(format!("{model}_{}.nc", time.format("%Y%m%dT%H%M%SZ")));
    let temporary = directory.join(format!(
        ".{}.tmp-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state.nc"),
        std::process::id()
    ));
    write(&temporary)?;
    fs::File::open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("cannot sync {}: {error}", temporary.display()))?;
    fs::rename(&temporary, &destination).map_err(|error| {
        format!(
            "cannot publish warm state {} as {}: {error}",
            temporary.display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

fn create_grid_file(
    path: &Path,
    model: &str,
    model_version: &str,
    state_time: DateTime<Utc>,
    grid: &RasterGrid,
) -> Result<netcdf::MutableFile, RISICOError> {
    let mut file = netcdf::create_with(path, netcdf::Options::NETCDF4)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.add_attribute("state_schema_version", STATE_SCHEMA_VERSION)
        .map_err(netcdf_error)?;
    file.add_attribute("model", model).map_err(netcdf_error)?;
    file.add_attribute("model_version", model_version)
        .map_err(netcdf_error)?;
    file.add_attribute("state_time", state_time.timestamp())
        .map_err(netcdf_error)?;
    file.add_attribute("epsg", grid.epsg)
        .map_err(netcdf_error)?;
    file.add_attribute("geotransform", grid.transform.to_vec())
        .map_err(netcdf_error)?;
    file.add_attribute("spatial_layout", "regular_grid_y_x")
        .map_err(netcdf_error)?;
    file.add_dimension("y", grid.height).map_err(netcdf_error)?;
    file.add_dimension("x", grid.width).map_err(netcdf_error)?;

    let mut x = file
        .add_variable::<f64>("x", &["x"])
        .map_err(netcdf_error)?;
    x.add_attribute("standard_name", "longitude")
        .map_err(netcdf_error)?;
    x.add_attribute("units", "degrees_east")
        .map_err(netcdf_error)?;
    x.put_values(&grid.x_coordinates(), Extents::All)
        .map_err(netcdf_error)?;
    let mut y = file
        .add_variable::<f64>("y", &["y"])
        .map_err(netcdf_error)?;
    y.add_attribute("standard_name", "latitude")
        .map_err(netcdf_error)?;
    y.add_attribute("units", "degrees_north")
        .map_err(netcdf_error)?;
    y.put_values(&grid.y_coordinates(), Extents::All)
        .map_err(netcdf_error)?;
    Ok(file)
}

fn open_grid_file(path: &Path, model: &str, model_version: &str) -> Result<File, RISICOError> {
    let file = netcdf::open(path)
        .map_err(|error| format!("cannot open NetCDF snapshot {}: {error}", path.display()))?;
    let schema: i32 = attribute(&file, "state_schema_version")?;
    if schema != STATE_SCHEMA_VERSION {
        return Err(format!("unsupported state schema version {schema}").into());
    }
    if string_attribute(&file, "model")? != model {
        return Err(format!("snapshot is not a {model} state").into());
    }
    let stored_version = string_attribute(&file, "model_version")?;
    if stored_version != model_version {
        return Err(format!(
            "snapshot model version {stored_version} does not match {model_version}"
        )
        .into());
    }
    if string_attribute(&file, "spatial_layout")? != "regular_grid_y_x" {
        return Err("unsupported warm-state spatial layout".into());
    }
    Ok(file)
}

fn grid_from_file(file: &File) -> Result<RasterGrid, RISICOError> {
    let width = file
        .dimension("x")
        .ok_or("missing NetCDF dimension x")?
        .len();
    let height = file
        .dimension("y")
        .ok_or("missing NetCDF dimension y")?
        .len();
    let epsg: u32 = attribute(file, "epsg")?;
    let values = match file
        .attribute("geotransform")
        .ok_or("missing NetCDF attribute geotransform")?
        .value()
        .map_err(netcdf_error)?
    {
        netcdf::AttrValue::Doubles(values) => values,
        _ => return Err("NetCDF attribute geotransform is not a double array".into()),
    };
    let transform: [f64; 6] = values.try_into().map_err(|values: Vec<f64>| {
        RISICOError::from(format!(
            "NetCDF geotransform has {} values, expected 6",
            values.len()
        ))
    })?;
    if width == 0
        || height == 0
        || transform[1] <= 0.0
        || transform[5] >= 0.0
        || transform[2] != 0.0
        || transform[4] != 0.0
    {
        return Err("warm-state grid must be non-empty, north-up, and unrotated".into());
    }
    Ok(RasterGrid {
        width,
        height,
        epsg,
        transform,
    })
}

fn sampling_indexes(
    source: &RasterGrid,
    target: &RasterGrid,
    target_cell_indexes: &[u32],
) -> Result<Vec<usize>, RISICOError> {
    if source.epsg != target.epsg {
        return Err(format!(
            "snapshot EPSG:{} does not match target EPSG:{}",
            source.epsg, target.epsg
        )
        .into());
    }
    target_cell_indexes
        .iter()
        .enumerate()
        .map(|(position, &cell)| {
            if cell as usize >= target.width * target.height {
                return Err(format!("target cell index {cell} is outside its grid").into());
            }
            let (x, y) = target.cell_center_f64(cell);
            source.nearest_cell_index(x, y).ok_or_else(|| {
                RISICOError::from(format!(
                    "target cell {position} at ({x}, {y}) is outside the warm-state grid"
                ))
            })
        })
        .collect()
}

fn write_active_grid(
    file: &mut netcdf::MutableFile,
    name: &str,
    grid: &RasterGrid,
    cell_indexes: &[u32],
    active_values: &[f32],
) -> Result<(), RISICOError> {
    let mut values = vec![FLOAT_FILL; grid.width * grid.height];
    for (&cell, &value) in cell_indexes.iter().zip(active_values) {
        values[cell as usize] = value;
    }
    write_compressed_variable(file, name, &["y", "x"], &values, Some(FLOAT_FILL))
}

fn write_active_mask(
    file: &mut netcdf::MutableFile,
    grid: &RasterGrid,
    cell_indexes: &[u32],
) -> Result<(), RISICOError> {
    let mut values = vec![0_u8; grid.width * grid.height];
    for &cell in cell_indexes {
        values[cell as usize] = 1;
    }
    write_compressed_variable(file, "active", &["y", "x"], &values, None)
}

fn validate_active_samples(
    file: &File,
    source_grid: &RasterGrid,
    source_indexes: &[usize],
) -> Result<(), RISICOError> {
    let active = read_variable::<u8>(file, "active")?;
    validate_grid_len("active", active.len(), source_grid)?;
    if let Some((target, _)) = source_indexes
        .iter()
        .enumerate()
        .find(|(_, source)| active[**source] == 0)
    {
        Err(format!("target cell {target} maps to an inactive warm-state pixel").into())
    } else {
        Ok(())
    }
}

fn sample_float_grid(
    file: &File,
    name: &str,
    source_grid: &RasterGrid,
    source_indexes: &[usize],
) -> Result<Vec<f32>, RISICOError> {
    let values = read_variable::<f32>(file, name)?;
    validate_grid_len(name, values.len(), source_grid)?;
    Ok(source_indexes
        .iter()
        .map(|&source| values[source])
        .collect())
}

fn validate_state_cells(
    model: &str,
    state_len: usize,
    grid: &RasterGrid,
    cell_indexes: &[u32],
) -> Result<(), RISICOError> {
    if state_len != cell_indexes.len() {
        return Err(format!(
            "cannot write {model} warm state: {state_len} state cells but {} active grid cells",
            cell_indexes.len()
        )
        .into());
    }
    let grid_len = grid.width * grid.height;
    if let Some(index) = cell_indexes
        .iter()
        .find(|index| **index as usize >= grid_len)
    {
        return Err(
            format!("active cell index {index} is outside the {grid_len}-cell grid").into(),
        );
    }
    Ok(())
}

fn validate_grid_len(name: &str, len: usize, grid: &RasterGrid) -> Result<(), RISICOError> {
    let expected = grid.width * grid.height;
    if len != expected {
        Err(format!("snapshot variable {name} has {len} values, expected {expected}").into())
    } else {
        Ok(())
    }
}

fn is_fill(value: f32) -> bool {
    value.is_nan()
}

fn snapshot_time(path: &Path) -> Result<DateTime<Utc>, RISICOError> {
    let file =
        netcdf::open(path).map_err(|error| format!("cannot open NetCDF snapshot: {error}"))?;
    let timestamp: i64 = attribute(&file, "state_time")?;
    DateTime::from_timestamp(timestamp, 0)
        .ok_or_else(|| format!("invalid state_time {timestamp}").into())
}

fn write_compressed_variable<T>(
    file: &mut netcdf::MutableFile,
    name: &str,
    dimensions: &[&str],
    values: &[T],
    fill: Option<T>,
) -> Result<(), RISICOError>
where
    T: netcdf::NcPutGet + Copy,
{
    let mut variable = file
        .add_variable::<T>(name, dimensions)
        .map_err(netcdf_error)?;
    if let Some(fill) = fill {
        variable.set_fill_value(fill).map_err(netcdf_error)?;
    }
    variable
        .compression(COMPRESSION_LEVEL, true)
        .map_err(netcdf_error)?;
    variable
        .put_values(values, Extents::All)
        .map_err(netcdf_error)
}

fn attribute<T>(file: &File, name: &str) -> Result<T, RISICOError>
where
    T: TryFrom<netcdf::AttrValue>,
    <T as TryFrom<netcdf::AttrValue>>::Error: std::fmt::Display,
{
    let value = file
        .attribute(name)
        .ok_or_else(|| format!("missing NetCDF attribute {name}"))?
        .value()
        .map_err(netcdf_error)?;
    value
        .try_into()
        .map_err(|error| format!("invalid NetCDF attribute {name}: {error}").into())
}

fn string_attribute(file: &File, name: &str) -> Result<String, RISICOError> {
    match file
        .attribute(name)
        .ok_or_else(|| format!("missing NetCDF attribute {name}"))?
        .value()
        .map_err(netcdf_error)?
    {
        netcdf::AttrValue::Str(value) => Ok(value),
        _ => Err(format!("NetCDF attribute {name} is not a string").into()),
    }
}

fn read_variable<T>(file: &File, name: &str) -> Result<Vec<T>, RISICOError>
where
    T: netcdf::NcPutGet,
{
    file.variable(name)
        .ok_or_else(|| RISICOError::from(format!("missing NetCDF variable {name}")))?
        .values::<T, _>(Extents::All)
        .map_err(netcdf_error)
}

fn netcdf_error(error: netcdf::error::Error) -> RISICOError {
    format!("NetCDF error: {error}").into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use risico::modules::fwi::config::FWIModelConfig;
    use risico::modules::risico::config::RISICOModelConfig;

    fn temporary_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "risico-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn grid(width: usize, height: usize, pixel_size: f64) -> RasterGrid {
        RasterGrid {
            width,
            height,
            epsg: 4326,
            transform: [10.0, pixel_size, 0.0, 50.0, 0.0, -pixel_size],
        }
    }

    #[test]
    fn risico_grid_snapshot_roundtrip_and_nearest_neighbour_resampling() {
        let directory = temporary_directory("risico-grid-state");
        let time = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let source_grid = grid(2, 2, 1.0);
        let source_indexes = [0, 1, 2, 3];
        let warm_state: Vec<_> = [10.0, 20.0, 30.0, 40.0]
            .into_iter()
            .map(|dffm| RISICOWarmState {
                dffm,
                ..RISICOWarmState::default()
            })
            .collect();
        let state = RISICOState::new(&warm_state, &time, RISICOModelConfig::new("v2023"));
        write_risico_snapshot(&directory, &state, "v2023", &source_grid, &source_indexes).unwrap();

        let fine_grid = grid(4, 4, 0.5);
        let fine_indexes: Vec<u32> = (0..16).collect();
        let (loaded, loaded_time) = load_latest_risico(
            &directory,
            time + Duration::hours(1),
            24,
            "v2023",
            &fine_grid,
            &fine_indexes,
        )
        .unwrap()
        .unwrap();
        assert_eq!(loaded_time, time);
        assert_eq!(
            loaded.iter().map(|v| v.dffm).collect::<Vec<_>>(),
            vec![
                10.0, 10.0, 20.0, 20.0, 10.0, 10.0, 20.0, 20.0, 30.0, 30.0, 40.0, 40.0, 30.0, 30.0,
                40.0, 40.0,
            ]
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn inactive_source_pixel_rejects_snapshot() {
        let directory = temporary_directory("risico-grid-fill");
        let time = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let source_grid = grid(2, 1, 1.0);
        let state = RISICOState::new(
            &[RISICOWarmState {
                dffm: 10.0,
                ..RISICOWarmState::default()
            }],
            &time,
            RISICOModelConfig::new("v2023"),
        );
        write_risico_snapshot(&directory, &state, "v2023", &source_grid, &[0]).unwrap();
        let loaded = load_latest_risico(&directory, time, 0, "v2023", &source_grid, &[1]).unwrap();
        assert!(loaded.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fwi_history_is_stored_as_grids_and_resampled() {
        let directory = temporary_directory("fwi-grid-state");
        let time = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let previous = time - Duration::hours(1);
        let source_grid = grid(1, 1, 1.0);
        let warm_state = [FWIWarmState {
            dates: vec![previous, time],
            ffmc: vec![80.0, 81.0],
            dmc: vec![6.0, 6.5],
            dc: vec![15.0, 15.5],
            rain: vec![0.0, 1.25],
        }];
        let state = FWIState::new(&warm_state, &time, FWIModelConfig::new("v2023"));
        write_fwi_snapshot(&directory, &state, "v2023", &source_grid, &[0]).unwrap();

        let target_grid = grid(2, 2, 0.5);
        let (loaded, _) =
            load_latest_fwi(&directory, time, 0, "v2023", &target_grid, &[0, 1, 2, 3])
                .unwrap()
                .unwrap();
        assert_eq!(loaded.len(), 4);
        assert!(loaded.iter().all(|cell| cell.dates == vec![previous, time]));
        assert!(loaded.iter().all(|cell| cell.ffmc == vec![80.0, 81.0]));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_fwi_uses_scalar_grid_values() {
        let directory = temporary_directory("fwi-legacy-grid-state");
        let time = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let source_grid = grid(1, 1, 1.0);
        let warm_state = [FWIWarmState {
            dates: vec![time - Duration::hours(1), time],
            ffmc: vec![81.0],
            dmc: vec![6.5],
            dc: vec![15.5],
            rain: vec![0.0, 0.25],
        }];
        let state = FWIState::new(&warm_state, &time, FWIModelConfig::new("legacy"));
        write_fwi_snapshot(&directory, &state, "legacy", &source_grid, &[0]).unwrap();
        let (loaded, _) = load_latest_fwi(&directory, time, 0, "legacy", &source_grid, &[0])
            .unwrap()
            .unwrap();
        assert_eq!(loaded[0].dates, vec![time]);
        assert_eq!(loaded[0].ffmc, vec![81.0]);
        assert_eq!(loaded[0].rain, vec![0.25]);
        fs::remove_dir_all(directory).unwrap();
    }
}
