use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use log::debug;
use rayon::prelude::*;
use risico::{constants::NODATAVAL, models::output::Output};
use serde_json::Map;
use strum::EnumProperty;
use zarrs::{
    array::{Array, ArrayBuilder, DataType},
    array_subset::ArraySubset,
    filesystem::FilesystemStore,
    group::{Group, GroupBuilder},
};

use crate::common::{
    helpers::RISICOError,
    io::models::{grid::RegularGrid, output::OutputVariable},
};

use super::prelude::OutputSink;

pub struct ZarrWriter {
    path: PathBuf,
    name: String,
    run_date: DateTime<Utc>,
    stores: HashMap<String, Arc<Mutex<ZarrVariableStore>>>,
}

impl ZarrWriter {
    pub fn new(path: &str, name: &str, run_date: &DateTime<Utc>) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: *run_date,
            stores: HashMap::new(),
        }
    }

    fn ensure_store(
        &mut self,
        variable: &OutputVariable,
        grid: &RegularGrid,
    ) -> Result<Arc<Mutex<ZarrVariableStore>>, RISICOError> {
        if let Some(existing) = self.stores.get(variable.name()) {
            return Ok(existing.clone());
        }

        fs::create_dir_all(&self.path).map_err(|err| {
            RISICOError::from(format!(
                "Cannot prepare directory {:?} for Zarr outputs: {err}",
                self.path
            ))
        })?;

        let store = ZarrVariableStore::new(&self.path, &self.name, variable, grid, &self.run_date)?;
        let store = Arc::new(Mutex::new(store));
        self.stores
            .insert(variable.name().to_string(), store.clone());
        Ok(store)
    }
}

impl OutputSink for ZarrWriter {
    fn write(
        &mut self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let contexts: Vec<_> = variables
            .iter()
            .map(|variable| self.ensure_store(variable, grid))
            .collect::<Result<Vec<_>, RISICOError>>()?;

        let results: Vec<Result<(), RISICOError>> = variables
            .par_iter()
            .zip(contexts.into_par_iter())
            .map(|(variable, store)| {
                if let Some(values) = variable.get_variable_on_grid(output, lats, lons, grid) {
                    debug!(
                        "[ZARR] Writing variable {} for {}",
                        variable.name(),
                        output.time
                    );

                    let mut store = store
                        .lock()
                        .map_err(|err| RISICOError::from(format!("Mutex poisoned: {err}")))?;

                    store
                        .append_sample(
                            variable,
                            values.as_slice().expect("Should unwrap"),
                            grid,
                            output,
                        )
                        .map_err(|err| {
                            RISICOError::from(format!(
                                "Cannot append Zarr sample for {}: {err}",
                                variable.name()
                            ))
                        })?;

                    debug!(
                        "[ZARR] Done writing variable {} ({} samples total)",
                        variable.name(),
                        store.samples()
                    );
                }

                Ok(())
            })
            .collect();

        super::helpers::extract_errors("ZARR Errors", results)
    }
}

struct ZarrVariableStore {
    data: Array<FilesystemStore>,
    time: Array<FilesystemStore>,
    samples: u64,
    rows: u64,
    cols: u64,
}

impl ZarrVariableStore {
    fn new(
        base_path: &Path,
        product_name: &str,
        variable: &OutputVariable,
        grid: &RegularGrid,
        run_date: &DateTime<Utc>,
    ) -> Result<Self, RISICOError> {
        let store_path = base_path.join(format!("{}.zarr", variable.name()));
        fs::create_dir_all(&store_path).map_err(|err| {
            RISICOError::from(format!(
                "Cannot create Zarr directory {:?}: {err}",
                store_path
            ))
        })?;

        let store = Arc::new(
            FilesystemStore::new(&store_path)
                .map_err(|err| RISICOError::from(format!("Cannot open Zarr store: {err}")))?,
        );

        if Group::open(store.clone(), "/").is_err() {
            let mut group_builder = GroupBuilder::new();
            let mut attributes = Map::new();
            attributes.insert(
                "product".to_string(),
                serde_json::Value::String(product_name.to_string()),
            );
            attributes.insert(
                "variable".to_string(),
                serde_json::Value::String(variable.name().to_string()),
            );
            attributes.insert(
                "run_date".to_string(),
                serde_json::Value::String(run_date.to_rfc3339()),
            );
            group_builder.attributes(attributes);
            let group = group_builder
                .build(store.clone(), "/")
                .map_err(|err| RISICOError::from(format!("Cannot create Zarr group: {err}")))?;
            group.store_metadata().map_err(|err| {
                RISICOError::from(format!("Cannot store Zarr group metadata: {err}"))
            })?;
        }

        let data_path = format!("/{}", variable.name());

        let data = match Array::open(store.clone(), &data_path) {
            Ok(array) => array,
            Err(_) => {
                create_static_coordinate_arrays(store.clone(), grid)?;
                create_time_array(store.clone())?;
                create_data_array(store.clone(), variable, grid, run_date, &data_path)?;
                Array::open(store.clone(), &data_path)
                    .map_err(|err| RISICOError::from(format!("Cannot open data array: {err}")))?
            }
        };

        let time = Array::open(store.clone(), "/time")
            .map_err(|err| RISICOError::from(format!("Cannot open time array: {err}")))?;

        let samples = data.shape().first().copied().unwrap_or(0);
        let rows = grid.nrows as u64;
        let cols = grid.ncols as u64;

        Ok(Self {
            data,
            time,
            samples,
            rows,
            cols,
        })
    }

    fn append_sample(
        &mut self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        output: &Output,
    ) -> Result<(), String> {
        if values.len() != grid.nrows * grid.ncols {
            return Err(format!(
                "Unexpected data length for {}: got {}, expected {}",
                variable.name(),
                values.len(),
                grid.nrows * grid.ncols
            ));
        }

        let next_index = self.samples;
        let new_size = next_index + 1;

        self.data
            .set_shape(vec![new_size, self.rows, self.cols])
            .map_err(|err| format!("Cannot resize data array: {err}"))?;
        self.data
            .store_metadata()
            .map_err(|err| format!("Cannot persist data metadata: {err}"))?;

        let subset = ArraySubset::new_with_start_shape(
            vec![next_index, 0, 0],
            vec![1, self.rows, self.cols],
        )
        .map_err(|err| format!("Cannot create data subset: {err}"))?;

        self.data
            .store_array_subset_elements(&subset, values)
            .map_err(|err| format!("Cannot store data subset: {err}"))?;

        self.time
            .set_shape(vec![new_size])
            .map_err(|err| format!("Cannot resize time array: {err}"))?;
        self.time
            .store_metadata()
            .map_err(|err| format!("Cannot persist time metadata: {err}"))?;

        let time_subset = ArraySubset::new_with_start_shape(vec![next_index], vec![1])
            .map_err(|err| format!("Cannot create time subset: {err}"))?;
        let timestamp = output.time.timestamp();
        self.time
            .store_array_subset_elements(&time_subset, &[timestamp])
            .map_err(|err| format!("Cannot store time value: {err}"))?;

        self.samples = new_size;
        Ok(())
    }

    fn samples(&self) -> u64 {
        self.samples
    }
}

fn create_static_coordinate_arrays(
    store: Arc<FilesystemStore>,
    grid: &RegularGrid,
) -> Result<(), RISICOError> {
    let latitude_path = "/latitude";
    let longitude_path = "/longitude";

    let latitudes: Vec<f32> = (0..grid.nrows)
        .map(|i| grid.min_lat + (grid.max_lat - grid.min_lat) * (i as f32) / (grid.nrows as f32))
        .collect();
    let longitudes: Vec<f32> = (0..grid.ncols)
        .map(|i| grid.min_lon + (grid.max_lon - grid.min_lon) * (i as f32) / (grid.ncols as f32))
        .collect();

    if Array::open(store.clone(), latitude_path).is_err() {
        let mut lat_builder = ArrayBuilder::new(
            vec![grid.nrows as u64],
            vec![grid.nrows as u64],
            DataType::Float32,
            0.0f32,
        );
        lat_builder.dimension_names(Some(["latitude"]));
        let latitude_array = lat_builder
            .build(store.clone(), latitude_path)
            .map_err(|err| RISICOError::from(format!("Cannot create latitude array: {err}")))?;
        latitude_array
            .store_metadata()
            .map_err(|err| RISICOError::from(format!("Cannot store latitude metadata: {err}")))?;
        let lat_subset = ArraySubset::new_with_shape(vec![grid.nrows as u64]);
        latitude_array
            .store_array_subset_elements(&lat_subset, &latitudes)
            .map_err(|err| RISICOError::from(format!("Cannot store latitude values: {err}")))?;
    }

    if Array::open(store.clone(), longitude_path).is_err() {
        let mut lon_builder = ArrayBuilder::new(
            vec![grid.ncols as u64],
            vec![grid.ncols as u64],
            DataType::Float32,
            0.0f32,
        );
        lon_builder.dimension_names(Some(["longitude"]));
        let longitude_array = lon_builder
            .build(store.clone(), longitude_path)
            .map_err(|err| RISICOError::from(format!("Cannot create longitude array: {err}")))?;
        longitude_array
            .store_metadata()
            .map_err(|err| RISICOError::from(format!("Cannot store longitude metadata: {err}")))?;
        let lon_subset = ArraySubset::new_with_shape(vec![grid.ncols as u64]);
        longitude_array
            .store_array_subset_elements(&lon_subset, &longitudes)
            .map_err(|err| RISICOError::from(format!("Cannot store longitude values: {err}")))?;
    }

    Ok(())
}

fn create_time_array(store: Arc<FilesystemStore>) -> Result<(), RISICOError> {
    let mut builder = ArrayBuilder::new(vec![0u64], vec![1u64], DataType::Int64, 0i64);
    builder.dimension_names(Some(["time"]));
    let mut attributes = Map::new();
    attributes.insert(
        "units".to_string(),
        serde_json::Value::String("seconds since 1970-01-01 00:00:00".to_string()),
    );
    attributes.insert(
        "calendar".to_string(),
        serde_json::Value::String("proleptic_gregorian".to_string()),
    );
    builder.attributes(attributes);

    let time_array = builder
        .build(store.clone(), "/time")
        .map_err(|err| RISICOError::from(format!("Cannot create time array: {err}")))?;
    time_array
        .store_metadata()
        .map_err(|err| RISICOError::from(format!("Cannot store time metadata: {err}")))?;
    Ok(())
}

fn create_data_array(
    store: Arc<FilesystemStore>,
    variable: &OutputVariable,
    grid: &RegularGrid,
    run_date: &DateTime<Utc>,
    data_path: &str,
) -> Result<(), RISICOError> {
    let rows = grid.nrows as u64;
    let cols = grid.ncols as u64;

    let mut builder = ArrayBuilder::new(
        vec![0u64, rows, cols],
        vec![1u64, rows, cols],
        DataType::Float32,
        NODATAVAL,
    );
    builder.dimension_names(Some(["time", "latitude", "longitude"]));

    let mut attributes = Map::new();
    if let Some(units) = variable.internal_name().get_str("units") {
        attributes.insert(
            "units".to_string(),
            serde_json::Value::String(units.to_string()),
        );
    }
    let long_name = variable
        .internal_name()
        .get_str("long_name")
        .unwrap_or(variable.name());
    attributes.insert(
        "long_name".to_string(),
        serde_json::Value::String(long_name.to_string()),
    );
    attributes.insert(
        "run_date".to_string(),
        serde_json::Value::String(run_date.to_rfc3339()),
    );
    builder.attributes(attributes);

    let data_array = builder
        .build(store.clone(), data_path)
        .map_err(|err| RISICOError::from(format!("Cannot create data array: {err}")))?;
    data_array
        .store_metadata()
        .map_err(|err| RISICOError::from(format!("Cannot store data metadata: {err}")))?;
    Ok(())
}
