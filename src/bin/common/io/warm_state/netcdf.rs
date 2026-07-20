use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use log::warn;
use netcdf::{extent::Extents, File};
use risico::modules::fwi::{constants::{DC_INIT, DMC_INIT, FFMC_INIT, NODATAVAL}, models::{FWIState, FWIWarmState}};
use risico::modules::risico::models::{RISICOState, RISICOWarmState};

use crate::common::helpers::RISICOError;

const STATE_SCHEMA_VERSION: i32 = 1;
const COMPRESSION_LEVEL: i32 = 4;

pub type RisicoWarmSnapshot = (Vec<RISICOWarmState>, DateTime<Utc>);
pub type FwiWarmSnapshot = (Vec<FWIWarmState>, DateTime<Utc>);

pub fn load_latest_risico(
    directory: impl AsRef<Path>,
    run_date: DateTime<Utc>,
    max_age_hours: i64,
    model_version: &str,
    grid_hash: &str,
    expected_cell_indexes: &[u32],
) -> Result<Option<RisicoWarmSnapshot>, RISICOError> {
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

    let oldest = run_date - Duration::hours(max_age_hours);
    let mut candidates: Vec<(DateTime<Utc>, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "nc"))
        .filter_map(|path| match snapshot_time(&path) {
            Ok(time) if time <= run_date && time >= oldest => Some((time, path)),
            Ok(_) => None,
            Err(error) => {
                warn!("Ignoring warm-state candidate {}: {error}", path.display());
                None
            }
        })
        .collect();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));

    for (time, path) in candidates {
        match read_risico_snapshot(&path, model_version, grid_hash, expected_cell_indexes) {
            Ok(state) => return Ok(Some((state, time))),
            Err(error) => warn!("Ignoring invalid warm state {}: {error}", path.display()),
        }
    }
    Ok(None)
}

pub fn write_risico_snapshot(
    directory: impl AsRef<Path>,
    state: &RISICOState,
    model_version: &str,
    grid_hash: &str,
    cell_indexes: &[u32],
) -> Result<PathBuf, RISICOError> {
    if state.data.len() != cell_indexes.len() {
        return Err(format!(
            "cannot write RISICO warm state: {} state cells but {} grid cells",
            state.data.len(),
            cell_indexes.len()
        )
        .into());
    }

    let directory = directory.as_ref();
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "cannot create warm-state directory {}: {error}",
            directory.display()
        )
    })?;

    let filename = format!("RISICO_{}.nc", state.time.format("%Y%m%dT%H%M%SZ"));
    let destination = directory.join(filename);
    let temporary = directory.join(format!(
        ".{}.tmp-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("RISICO-state.nc"),
        std::process::id()
    ));

    write_risico_file(&temporary, state, model_version, grid_hash, cell_indexes)?;

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

fn write_risico_file(
    path: &Path,
    state: &RISICOState,
    model_version: &str,
    grid_hash: &str,
    cell_indexes: &[u32],
) -> Result<(), RISICOError> {
    let mut file = netcdf::create_with(path, netcdf::Options::NETCDF4)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.add_attribute("state_schema_version", STATE_SCHEMA_VERSION)
        .map_err(netcdf_error)?;
    file.add_attribute("model", "RISICO")
        .map_err(netcdf_error)?;
    file.add_attribute("model_version", model_version)
        .map_err(netcdf_error)?;
    file.add_attribute("state_time", state.time.timestamp())
        .map_err(netcdf_error)?;
    file.add_attribute("grid_hash", grid_hash)
        .map_err(netcdf_error)?;
    file.add_attribute("active_cell_count", cell_indexes.len() as u64)
        .map_err(netcdf_error)?;
    file.add_dimension("cell", cell_indexes.len())
        .map_err(netcdf_error)?;

    let mut index_variable = file
        .add_variable::<u32>("cell_index", &["cell"])
        .map_err(netcdf_error)?;
    index_variable
        .compression(COMPRESSION_LEVEL, true)
        .map_err(netcdf_error)?;
    index_variable
        .put_values(cell_indexes, Extents::All)
        .map_err(netcdf_error)?;

    let fields: [(&str, Vec<f32>); 9] = [
        ("dffm", state.data.iter().map(|value| value.dffm).collect()),
        (
            "snow_cover",
            state.data.iter().map(|value| value.snow_cover).collect(),
        ),
        (
            "snow_cover_time",
            state
                .data
                .iter()
                .map(|value| value.snow_cover_time)
                .collect(),
        ),
        ("msi", state.data.iter().map(|value| value.MSI).collect()),
        (
            "msi_ttl",
            state.data.iter().map(|value| value.MSI_TTL).collect(),
        ),
        ("ndvi", state.data.iter().map(|value| value.NDVI).collect()),
        (
            "ndvi_time",
            state.data.iter().map(|value| value.NDVI_TIME).collect(),
        ),
        ("ndwi", state.data.iter().map(|value| value.NDWI).collect()),
        (
            "ndwi_time",
            state.data.iter().map(|value| value.NDWI_TIME).collect(),
        ),
    ];

    for (name, values) in fields {
        let mut variable = file
            .add_variable::<f32>(name, &["cell"])
            .map_err(netcdf_error)?;
        variable
            .compression(COMPRESSION_LEVEL, true)
            .map_err(netcdf_error)?;
        variable
            .put_values(&values, Extents::All)
            .map_err(netcdf_error)?;
    }

    drop(file);
    Ok(())
}

fn snapshot_time(path: &Path) -> Result<DateTime<Utc>, RISICOError> {
    let file =
        netcdf::open(path).map_err(|error| format!("cannot open NetCDF snapshot: {error}"))?;
    let timestamp: i64 = attribute(&file, "state_time")?;
    DateTime::from_timestamp(timestamp, 0)
        .ok_or_else(|| format!("invalid state_time {timestamp}").into())
}

fn read_risico_snapshot(
    path: &Path,
    model_version: &str,
    grid_hash: &str,
    expected_cell_indexes: &[u32],
) -> Result<Vec<RISICOWarmState>, RISICOError> {
    let file =
        netcdf::open(path).map_err(|error| format!("cannot open NetCDF snapshot: {error}"))?;

    let schema_version: i32 = attribute(&file, "state_schema_version")?;
    if schema_version != STATE_SCHEMA_VERSION {
        return Err(format!("unsupported state schema version {schema_version}").into());
    }
    let stored_model = string_attribute(&file, "model")?;
    if stored_model != "RISICO" {
        return Err(format!("snapshot is for model {stored_model}, not RISICO").into());
    }
    let stored_model_version = string_attribute(&file, "model_version")?;
    if stored_model_version != model_version {
        return Err(format!(
            "snapshot model version {stored_model_version} does not match {model_version}"
        )
        .into());
    }
    let stored_grid_hash = string_attribute(&file, "grid_hash")?;
    if stored_grid_hash != grid_hash {
        return Err("snapshot grid hash does not match the configured static grid".into());
    }

    let cell_indexes = read_variable::<u32>(&file, "cell_index")?;
    if cell_indexes != expected_cell_indexes {
        return Err("snapshot cell indexes do not match the configured domain mask".into());
    }

    let dffm = read_variable::<f32>(&file, "dffm")?;
    let snow_cover = read_variable::<f32>(&file, "snow_cover")?;
    let snow_cover_time = read_variable::<f32>(&file, "snow_cover_time")?;
    let msi = read_variable::<f32>(&file, "msi")?;
    let msi_ttl = read_variable::<f32>(&file, "msi_ttl")?;
    let ndvi = read_variable::<f32>(&file, "ndvi")?;
    let ndvi_time = read_variable::<f32>(&file, "ndvi_time")?;
    let ndwi = read_variable::<f32>(&file, "ndwi")?;
    let ndwi_time = read_variable::<f32>(&file, "ndwi_time")?;

    let expected_len = expected_cell_indexes.len();
    for (name, len) in [
        ("dffm", dffm.len()),
        ("snow_cover", snow_cover.len()),
        ("snow_cover_time", snow_cover_time.len()),
        ("msi", msi.len()),
        ("msi_ttl", msi_ttl.len()),
        ("ndvi", ndvi.len()),
        ("ndvi_time", ndvi_time.len()),
        ("ndwi", ndwi.len()),
        ("ndwi_time", ndwi_time.len()),
    ] {
        if len != expected_len {
            return Err(format!(
                "snapshot variable {name} has {len} values, expected {expected_len}"
            )
            .into());
        }
    }

    Ok((0..expected_len)
        .map(|index| RISICOWarmState {
            dffm: dffm[index],
            snow_cover: snow_cover[index],
            snow_cover_time: snow_cover_time[index],
            MSI: msi[index],
            MSI_TTL: msi_ttl[index],
            NDVI: ndvi[index],
            NDVI_TIME: ndvi_time[index],
            NDWI: ndwi[index],
            NDWI_TIME: ndwi_time[index],
        })
        .collect())
}

pub fn load_latest_fwi(
    directory: impl AsRef<Path>,
    run_date: DateTime<Utc>,
    max_age_hours: i64,
    model_version: &str,
    grid_hash: &str,
    expected_cell_indexes: &[u32],
) -> Result<Option<FwiWarmSnapshot>, RISICOError> {
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
    let oldest = run_date - Duration::hours(max_age_hours);
    let mut candidates: Vec<(DateTime<Utc>, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "nc"))
        .filter_map(|path| match snapshot_time(&path) {
            Ok(time) if time <= run_date && time >= oldest => Some((time, path)),
            Ok(_) => None,
            Err(error) => {
                warn!("Ignoring warm-state candidate {}: {error}", path.display());
                None
            }
        })
        .collect();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));

    for (time, path) in candidates {
        match read_fwi_snapshot(&path, model_version, grid_hash, expected_cell_indexes) {
            Ok(state) => return Ok(Some((state, time))),
            Err(error) => warn!("Ignoring invalid warm state {}: {error}", path.display()),
        }
    }
    Ok(None)
}

pub fn write_fwi_snapshot(
    directory: impl AsRef<Path>,
    state: &FWIState,
    model_version: &str,
    grid_hash: &str,
    cell_indexes: &[u32],
) -> Result<PathBuf, RISICOError> {
    if state.data.len() != cell_indexes.len() {
        return Err(format!(
            "cannot write FWI warm state: {} state cells but {} grid cells",
            state.data.len(),
            cell_indexes.len()
        )
        .into());
    }
    for (index, cell) in state.data.iter().enumerate() {
        let length = cell.dates.len();
        let histories_valid = model_version == "legacy"
            || [cell.ffmc.len(), cell.dmc.len(), cell.dc.len(), cell.rain.len()]
                .iter()
                .all(|candidate| *candidate == length);
        if !histories_valid {
            return Err(format!(
                "cannot write FWI warm state: cell {index} history arrays have different lengths"
            )
            .into());
        }
    }

    let directory = directory.as_ref();
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "cannot create warm-state directory {}: {error}",
            directory.display()
        )
    })?;
    let destination = directory.join(format!("FWI_{}.nc", state.time.format("%Y%m%dT%H%M%SZ")));
    let temporary = directory.join(format!(
        ".{}.tmp-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("FWI-state.nc"),
        std::process::id()
    ));
    write_fwi_file(&temporary, state, model_version, grid_hash, cell_indexes)?;
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

fn write_fwi_file(
    path: &Path,
    state: &FWIState,
    model_version: &str,
    grid_hash: &str,
    cell_indexes: &[u32],
) -> Result<(), RISICOError> {
    let observation_count: usize = state
        .data
        .iter()
        .map(|cell| {
            if model_version == "legacy" {
                usize::from(!cell.ffmc.is_empty() || !cell.dmc.is_empty() || !cell.dc.is_empty())
            } else {
                cell.dates.len()
            }
        })
        .sum();
    let mut starts = Vec::with_capacity(state.data.len());
    let mut counts = Vec::with_capacity(state.data.len());
    let mut times = Vec::with_capacity(observation_count);
    let mut ffmc = Vec::with_capacity(observation_count);
    let mut dmc = Vec::with_capacity(observation_count);
    let mut dc = Vec::with_capacity(observation_count);
    let mut rain = Vec::with_capacity(observation_count);
    for cell in &state.data {
        starts.push(times.len() as u64);
        if model_version == "legacy" {
            let has_state = !cell.ffmc.is_empty() || !cell.dmc.is_empty() || !cell.dc.is_empty();
            counts.push(u32::from(has_state));
            if has_state {
                times.push(state.time.timestamp());
                ffmc.push(cell.ffmc.first().copied().unwrap_or(FFMC_INIT));
                dmc.push(cell.dmc.first().copied().unwrap_or(DMC_INIT));
                dc.push(cell.dc.first().copied().unwrap_or(DC_INIT));
                rain.push(cell.rain.last().copied().unwrap_or(NODATAVAL));
            }
        } else {
            counts.push(cell.dates.len() as u32);
            times.extend(cell.dates.iter().map(DateTime::timestamp));
            ffmc.extend_from_slice(&cell.ffmc);
            dmc.extend_from_slice(&cell.dmc);
            dc.extend_from_slice(&cell.dc);
            rain.extend_from_slice(&cell.rain);
        }
    }

    let mut file = netcdf::create_with(path, netcdf::Options::NETCDF4)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.add_attribute("state_schema_version", STATE_SCHEMA_VERSION)
        .map_err(netcdf_error)?;
    file.add_attribute("model", "FWI").map_err(netcdf_error)?;
    file.add_attribute("model_version", model_version)
        .map_err(netcdf_error)?;
    file.add_attribute("state_time", state.time.timestamp())
        .map_err(netcdf_error)?;
    file.add_attribute("grid_hash", grid_hash)
        .map_err(netcdf_error)?;
    file.add_attribute("active_cell_count", cell_indexes.len() as u64)
        .map_err(netcdf_error)?;
    file.add_attribute("history_layout", "contiguous_ragged_v1")
        .map_err(netcdf_error)?;
    file.add_dimension("cell", cell_indexes.len())
        .map_err(netcdf_error)?;
    file.add_dimension("observation", observation_count)
        .map_err(netcdf_error)?;

    write_compressed_variable(&mut file, "cell_index", &["cell"], cell_indexes)?;
    write_compressed_variable(&mut file, "history_start", &["cell"], &starts)?;
    write_compressed_variable(&mut file, "history_count", &["cell"], &counts)?;
    write_compressed_variable(&mut file, "history_time", &["observation"], &times)?;
    write_compressed_variable(&mut file, "ffmc", &["observation"], &ffmc)?;
    write_compressed_variable(&mut file, "dmc", &["observation"], &dmc)?;
    write_compressed_variable(&mut file, "dc", &["observation"], &dc)?;
    write_compressed_variable(&mut file, "rain", &["observation"], &rain)?;
    drop(file);
    Ok(())
}

fn read_fwi_snapshot(
    path: &Path,
    model_version: &str,
    grid_hash: &str,
    expected_cell_indexes: &[u32],
) -> Result<Vec<FWIWarmState>, RISICOError> {
    let file = netcdf::open(path)
        .map_err(|error| format!("cannot open NetCDF snapshot {}: {error}", path.display()))?;
    let schema_version: i32 = attribute(&file, "state_schema_version")?;
    if schema_version != STATE_SCHEMA_VERSION {
        return Err(format!("unsupported state schema version {schema_version}").into());
    }
    if string_attribute(&file, "model")? != "FWI" {
        return Err("snapshot is not an FWI state".into());
    }
    let stored_version = string_attribute(&file, "model_version")?;
    if stored_version != model_version {
        return Err(format!(
            "snapshot model version {stored_version} does not match {model_version}"
        )
        .into());
    }
    if string_attribute(&file, "grid_hash")? != grid_hash {
        return Err("snapshot grid hash does not match the configured static grid".into());
    }
    let history_layout = string_attribute(&file, "history_layout")?;
    if history_layout != "contiguous_ragged_v1" {
        return Err("unsupported FWI history layout".into());
    }
    let cell_indexes = read_variable::<u32>(&file, "cell_index")?;
    if cell_indexes != expected_cell_indexes {
        return Err("snapshot cell indexes do not match the configured domain mask".into());
    }

    let starts = read_variable::<u64>(&file, "history_start")?;
    let counts = read_variable::<u32>(&file, "history_count")?;
    let times = read_variable::<i64>(&file, "history_time")?;
    let ffmc = read_variable::<f32>(&file, "ffmc")?;
    let dmc = read_variable::<f32>(&file, "dmc")?;
    let dc = read_variable::<f32>(&file, "dc")?;
    let rain = read_variable::<f32>(&file, "rain")?;
    let observation_count = times.len();
    if starts.len() != cell_indexes.len() || counts.len() != cell_indexes.len() {
        return Err("FWI snapshot ragged-array indexes have invalid lengths".into());
    }
    if [ffmc.len(), dmc.len(), dc.len(), rain.len()]
        .iter()
        .any(|length| *length != observation_count)
    {
        return Err("FWI snapshot history variables have different lengths".into());
    }

    starts
        .into_iter()
        .zip(counts)
        .enumerate()
        .map(|(cell, (start, count))| {
            let start = usize::try_from(start)
                .map_err(|_| RISICOError::from(format!("invalid history start for cell {cell}")))?;
            let end = start
                .checked_add(count as usize)
                .filter(|end| *end <= observation_count)
                .ok_or_else(|| {
                    RISICOError::from(format!("invalid history range for cell {cell}"))
                })?;
            let dates = times[start..end]
                .iter()
                .map(|timestamp| {
                    DateTime::from_timestamp(*timestamp, 0).ok_or_else(|| {
                        RISICOError::from(format!(
                            "invalid history timestamp {timestamp} for cell {cell}"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FWIWarmState {
                dates,
                ffmc: ffmc[start..end].to_vec(),
                dmc: dmc[start..end].to_vec(),
                dc: dc[start..end].to_vec(),
                rain: rain[start..end].to_vec(),
            })
        })
        .collect()
}

fn write_compressed_variable<T>(
    file: &mut netcdf::MutableFile,
    name: &str,
    dimensions: &[&str],
    values: &[T],
) -> Result<(), RISICOError>
where
    T: netcdf::NcPutGet,
{
    let mut variable = file
        .add_variable::<T>(name, dimensions)
        .map_err(netcdf_error)?;
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
    use risico_test_support::temporary_directory;

    // Kept local so production code does not acquire a temporary-file dependency.
    mod risico_test_support {
        use std::path::PathBuf;

        pub fn temporary_directory(name: &str) -> PathBuf {
            std::env::temp_dir().join(format!(
                "risico-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock should be after epoch")
                    .as_nanos()
            ))
        }
    }

    #[test]
    fn risico_snapshot_roundtrip() {
        let directory = temporary_directory("state-roundtrip");
        let time = DateTime::from_timestamp(1_700_000_000, 0).expect("valid test time");
        let warm_state = vec![
            RISICOWarmState {
                dffm: 12.5,
                snow_cover: 0.25,
                snow_cover_time: 123.0,
                MSI: 0.4,
                MSI_TTL: 10.0,
                NDVI: 0.6,
                NDVI_TIME: 456.0,
                NDWI: 0.2,
                NDWI_TIME: 789.0,
            },
            RISICOWarmState::default(),
        ];
        let state = RISICOState::new(&warm_state, &time, RISICOModelConfig::new("v2023"));
        let indexes = [3, 9];

        write_risico_snapshot(&directory, &state, "v2023", "grid-1", &indexes)
            .expect("snapshot should be written");
        let (loaded, loaded_time) = load_latest_risico(
            &directory,
            time + Duration::hours(1),
            24,
            "v2023",
            "grid-1",
            &indexes,
        )
        .expect("snapshot lookup should succeed")
        .expect("snapshot should be found");

        assert_eq!(loaded_time, time);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].dffm, 12.5);
        assert_eq!(loaded[0].NDWI_TIME, 789.0);
        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn mismatched_grid_is_not_loaded() {
        let directory = temporary_directory("state-grid-mismatch");
        let time = DateTime::from_timestamp(1_700_000_000, 0).expect("valid test time");
        let warm_state = vec![RISICOWarmState::default()];
        let state = RISICOState::new(&warm_state, &time, RISICOModelConfig::new("v2023"));
        write_risico_snapshot(&directory, &state, "v2023", "grid-1", &[3])
            .expect("snapshot should be written");

        let loaded = load_latest_risico(
            &directory,
            time + Duration::hours(1),
            24,
            "v2023",
            "grid-2",
            &[3],
        )
        .expect("snapshot lookup should succeed");
        assert!(loaded.is_none());
        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn invalid_newest_snapshot_falls_back_to_older_valid_snapshot() {
        let directory = temporary_directory("state-older-fallback");
        let older_time = DateTime::from_timestamp(1_700_000_000, 0).expect("valid test time");
        let newer_time = older_time + Duration::hours(1);
        let older_state = RISICOState::new(
            &[RISICOWarmState {
                dffm: 11.0,
                ..RISICOWarmState::default()
            }],
            &older_time,
            RISICOModelConfig::new("v2023"),
        );
        let newer_state = RISICOState::new(
            &[RISICOWarmState {
                dffm: 22.0,
                ..RISICOWarmState::default()
            }],
            &newer_time,
            RISICOModelConfig::new("v2023"),
        );
        write_risico_snapshot(&directory, &older_state, "v2023", "grid-1", &[3])
            .expect("older snapshot should be written");
        write_risico_snapshot(&directory, &newer_state, "v2023", "wrong-grid", &[3])
            .expect("newer snapshot should be written");

        let (loaded, loaded_time) = load_latest_risico(
            &directory,
            newer_time + Duration::hours(1),
            24,
            "v2023",
            "grid-1",
            &[3],
        )
        .expect("snapshot lookup should succeed")
        .expect("the older valid snapshot should be found");
        assert_eq!(loaded_time, older_time);
        assert_eq!(loaded[0].dffm, 11.0);
        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn fwi_ragged_history_roundtrip() {
        let directory = temporary_directory("fwi-state-roundtrip");
        let time = DateTime::from_timestamp(1_700_000_000, 0).expect("valid test time");
        let previous = time - Duration::hours(1);
        let warm_state = vec![
            FWIWarmState {
                dates: vec![previous, time],
                ffmc: vec![80.0, 81.0],
                dmc: vec![6.0, 6.5],
                dc: vec![15.0, 15.5],
                rain: vec![0.0, 1.25],
                ..FWIWarmState::default()
            },
            FWIWarmState {
                dates: vec![time],
                ffmc: vec![75.0],
                dmc: vec![4.0],
                dc: vec![12.0],
                rain: vec![2.0],
                ..FWIWarmState::default()
            },
        ];
        let state = FWIState::new(&warm_state, &time, FWIModelConfig::new("v2023"));
        let indexes = [2, 7];

        write_fwi_snapshot(&directory, &state, "v2023", "fwi-grid", &indexes)
            .expect("FWI snapshot should be written");
        let (loaded, loaded_time) = load_latest_fwi(
            &directory,
            time + Duration::hours(1),
            24,
            "v2023",
            "fwi-grid",
            &indexes,
        )
        .expect("snapshot lookup should succeed")
        .expect("snapshot should be found");

        assert_eq!(loaded_time, time);
        assert_eq!(loaded[0].dates, vec![previous, time]);
        assert_eq!(loaded[0].ffmc, vec![80.0, 81.0]);
        assert_eq!(loaded[1].rain, vec![2.0]);
        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn legacy_fwi_scalar_state_roundtrip_matches_deployed_warm_state() {
        let directory = temporary_directory("fwi-legacy-state-roundtrip");
        let time = DateTime::from_timestamp(1_700_000_000, 0).expect("valid test time");
        let dates = vec![time - Duration::hours(2), time - Duration::hours(1), time];
        let warm_state = vec![FWIWarmState {
            dates: dates.clone(),
            ffmc: vec![81.0],
            dmc: vec![6.5],
            dc: vec![15.5],
            rain: vec![0.0, 1.0, 0.25],
        }];
        let state = FWIState::new(&warm_state, &time, FWIModelConfig::new("legacy"));

        write_fwi_snapshot(&directory, &state, "legacy", "fwi-grid", &[2])
            .expect("legacy snapshot should accept scalar moisture histories");
        let (loaded, _) = load_latest_fwi(
            &directory,
            time + Duration::hours(1),
            24,
            "legacy",
            "fwi-grid",
            &[2],
        )
        .expect("snapshot lookup should succeed")
        .expect("snapshot should be found");

        assert_eq!(loaded[0].dates, vec![time]);
        assert_eq!(loaded[0].ffmc, vec![81.0]);
        assert_eq!(loaded[0].dmc, vec![6.5]);
        assert_eq!(loaded[0].dc, vec![15.5]);
        assert_eq!(loaded[0].rain, vec![0.25]);
        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }
}
