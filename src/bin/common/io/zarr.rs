//! Zarr v3 output: one store per variable, one chunk per timestep.
//!
//! Ported from the `ZarrWriter` on the `refactor/output-writing` branch and
//! adapted to this branch's per-variable writer contract. Unlike the NetCDF
//! library, zarrs takes no global lock, so the variables of a timestep are
//! compressed and written in parallel, each behind its own mutex.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use log::debug;
use risico::constants::NODATAVAL;
use risico::version::FULL_VERSION;
use serde_json::{Map, Value};
use strum::EnumProperty;
use zarrs::{
    array::{codec::ZstdCodec, Array, ArrayBuilder, DataType},
    array_subset::ArraySubset,
    filesystem::FilesystemStore,
    group::{Group, GroupBuilder},
    metadata_ext::group::consolidated_metadata::{ConsolidatedMetadata, ConsolidatedMetadataKind},
    node::Node,
};

use crate::common::helpers::RISICOError;
use crate::common::io::models::{
    grid::RegularGrid,
    output::{OutputVariable, Writer},
};

/// Writes each variable to `<path>/<variable>.zarr`, appending one timestep
/// per call, like the NetCDF writer does with one file per variable.
pub struct ZarrWriter {
    path: PathBuf,
    name: String,
    run_date: DateTime<Utc>,
    /// zstd level, 1 to 22; 0 leaves chunks uncompressed.
    compression_level: i32,
    stores: HashMap<String, Mutex<VariableStore>>,
}

impl ZarrWriter {
    pub fn new(path: &str, name: &str, run_date: &DateTime<Utc>, compression_level: i32) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: *run_date,
            compression_level: compression_level.clamp(0, 22),
            stores: HashMap::new(),
        }
    }
}

impl Writer for ZarrWriter {
    fn prepare(
        &mut self,
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        fs::create_dir_all(&self.path).map_err(|error| {
            format!(
                "cannot create Zarr output directory {}: {error}",
                self.path.display()
            )
        })?;
        for variable in variables {
            if self.stores.contains_key(variable.output_name()) {
                continue;
            }
            let store = VariableStore::create(
                &self.path,
                &self.name,
                variable,
                grid,
                self.run_date,
                self.compression_level,
            )?;
            self.stores
                .insert(variable.output_name().to_string(), Mutex::new(store));
        }
        Ok(())
    }

    fn write_variable(
        &self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let store = self
            .stores
            .get(variable.output_name())
            .ok_or_else(|| format!("no open Zarr store for variable {}", variable.output_name()))?;
        if values.len() != grid.nrows * grid.ncols {
            return Err(format!(
                "Zarr sample for {} has {} values, the grid has {}",
                variable.output_name(),
                values.len(),
                grid.nrows * grid.ncols
            )
            .into());
        }
        debug!(
            "[ZARR] Writing variable {} for {time}",
            variable.output_name()
        );
        store
            .lock()
            .expect("Zarr store lock is poisoned")
            .append(values, time)
            .map_err(|error| {
                RISICOError::from(format!(
                    "cannot append {time} to Zarr store {}: {error}",
                    variable.output_name()
                ))
            })
    }

    /// Write the consolidated metadata of every store once its arrays have
    /// their final shape. Readers such as xarray `open_zarr(consolidated=True)`
    /// (the DDS xarray service) refuse a Zarr v3 store without it.
    fn finish(&mut self) -> Result<(), RISICOError> {
        for (name, store) in &self.stores {
            store
                .lock()
                .expect("Zarr store lock is poisoned")
                .consolidate()
                .map_err(|error| {
                    RISICOError::from(format!(
                        "cannot consolidate the metadata of Zarr store {name}: {error}"
                    ))
                })?;
        }
        Ok(())
    }
}

/// One variable's store: the data array and its time axis.
struct VariableStore {
    store: Arc<FilesystemStore>,
    data: Array<FilesystemStore>,
    time: Array<FilesystemStore>,
    samples: u64,
    rows: u64,
    cols: u64,
}

impl VariableStore {
    /// Create a fresh store, replacing one left by an earlier run of the same
    /// output, as the NetCDF writer replaces its files.
    fn create(
        base: &Path,
        product: &str,
        variable: &OutputVariable,
        grid: &RegularGrid,
        run_date: DateTime<Utc>,
        compression_level: i32,
    ) -> Result<Self, RISICOError> {
        let store_path = base.join(format!("{}.zarr", variable.output_name()));
        if store_path.exists() {
            fs::remove_dir_all(&store_path).map_err(|error| {
                format!(
                    "cannot replace Zarr store {}: {error}",
                    store_path.display()
                )
            })?;
        }
        fs::create_dir_all(&store_path).map_err(|error| {
            format!("cannot create Zarr store {}: {error}", store_path.display())
        })?;
        let store = Arc::new(FilesystemStore::new(&store_path).map_err(|error| {
            format!("cannot open Zarr store {}: {error}", store_path.display())
        })?);

        let mut attributes = Map::new();
        attributes.insert("product".into(), Value::String(product.to_string()));
        attributes.insert(
            "variable".into(),
            Value::String(variable.output_name().to_string()),
        );
        attributes.insert("run_date".into(), Value::String(run_date.to_rfc3339()));
        attributes.insert(
            "risico_version".into(),
            Value::String(FULL_VERSION.to_string()),
        );
        attributes.insert(
            "creation_date".into(),
            Value::String(Utc::now().to_rfc3339()),
        );
        attributes.insert("missing_value".into(), Value::from(NODATAVAL));
        let mut group = GroupBuilder::new();
        group.attributes(attributes);
        group
            .build(store.clone(), "/")
            .and_then(|group| group.store_metadata().map_err(Into::into))
            .map_err(|error| format!("cannot create Zarr group: {error}"))?;

        let rows = grid.nrows as u64;
        let cols = grid.ncols as u64;
        // The same axes as the NetCDF output, so the two can be read alike.
        let latitudes: Vec<f32> = (0..grid.nrows)
            .map(|i| grid.min_lat + grid.step_lat * (i as f32))
            .collect();
        let longitudes: Vec<f32> = (0..grid.ncols)
            .map(|i| grid.min_lon + grid.step_lon * (i as f32))
            .collect();
        write_axis(&store, "/latitude", "latitude", &latitudes)?;
        write_axis(&store, "/longitude", "longitude", &longitudes)?;

        let mut time_attributes = Map::new();
        time_attributes.insert(
            "units".into(),
            Value::String("seconds since 1970-01-01 00:00:00.0".into()),
        );
        time_attributes.insert(
            "calendar".into(),
            Value::String("proleptic_gregorian".into()),
        );
        let mut time_builder = ArrayBuilder::new(vec![0u64], vec![1u64], DataType::Int64, 0i64);
        time_builder
            .dimension_names(Some(["time"]))
            .attributes(time_attributes);
        let time = time_builder
            .build(store.clone(), "/time")
            .map_err(|error| format!("cannot create Zarr time array: {error}"))?;
        time.store_metadata()
            .map_err(|error| format!("cannot store Zarr time metadata: {error}"))?;

        let mut data_attributes = Map::new();
        if let Some(units) = variable.internal_name().get_str("units") {
            data_attributes.insert("units".into(), Value::String(units.to_string()));
        }
        let long_name = variable
            .internal_name()
            .get_str("long_name")
            .unwrap_or(variable.output_name());
        data_attributes.insert("long_name".into(), Value::String(long_name.to_string()));
        data_attributes.insert("missing_value".into(), Value::from(NODATAVAL));
        // One timestep per chunk, like the NetCDF output, so a map is one
        // read; zstd because it is the fastest codec that still shrinks the
        // grids well, and it is part of the Zarr v3 core codec set.
        let mut data_builder = ArrayBuilder::new(
            vec![0u64, rows, cols],
            vec![1u64, rows, cols],
            DataType::Float32,
            NODATAVAL,
        );
        data_builder
            .dimension_names(Some(["time", "latitude", "longitude"]))
            .attributes(data_attributes);
        if compression_level > 0 {
            data_builder
                .bytes_to_bytes_codecs(vec![Arc::new(ZstdCodec::new(compression_level, false))]);
        }
        let data = data_builder
            .build(store.clone(), &format!("/{}", variable.output_name()))
            .map_err(|error| format!("cannot create Zarr data array: {error}"))?;
        data.store_metadata()
            .map_err(|error| format!("cannot store Zarr data metadata: {error}"))?;

        Ok(Self {
            store,
            data,
            time,
            samples: 0,
            rows,
            cols,
        })
    }

    /// Inline the metadata of the arrays into the root group (`consolidated_metadata`).
    fn consolidate(&self) -> Result<(), String> {
        let root = Node::open(&self.store, "/")
            .map_err(|error| format!("cannot open the store hierarchy: {error}"))?;
        let metadata = root
            .consolidate_metadata()
            .ok_or_else(|| "the store root is not a group".to_string())?;
        let mut group = Group::open(self.store.clone(), "/")
            .map_err(|error| format!("cannot open the root group: {error}"))?;
        group.set_consolidated_metadata(Some(ConsolidatedMetadata {
            metadata,
            kind: ConsolidatedMetadataKind::Inline,
        }));
        group
            .store_metadata()
            .map_err(|error| format!("cannot store the root group metadata: {error}"))
    }

    /// Grow both arrays by one timestep and write it.
    fn append(&mut self, values: &[f32], time: DateTime<Utc>) -> Result<(), String> {
        let index = self.samples;
        let samples = index + 1;

        self.data
            .set_shape(vec![samples, self.rows, self.cols])
            .map_err(|error| format!("cannot grow data array: {error}"))?;
        let subset =
            ArraySubset::new_with_start_shape(vec![index, 0, 0], vec![1, self.rows, self.cols])
                .map_err(|error| format!("cannot address data chunk: {error}"))?;
        self.data
            .store_array_subset_elements(&subset, values)
            .map_err(|error| format!("cannot store data chunk: {error}"))?;
        self.data
            .store_metadata()
            .map_err(|error| format!("cannot store data metadata: {error}"))?;

        self.time
            .set_shape(vec![samples])
            .map_err(|error| format!("cannot grow time array: {error}"))?;
        let time_subset = ArraySubset::new_with_start_shape(vec![index], vec![1])
            .map_err(|error| format!("cannot address time chunk: {error}"))?;
        self.time
            .store_array_subset_elements(&time_subset, &[time.timestamp()])
            .map_err(|error| format!("cannot store time value: {error}"))?;
        self.time
            .store_metadata()
            .map_err(|error| format!("cannot store time metadata: {error}"))?;

        self.samples = samples;
        Ok(())
    }
}

/// Write a fixed coordinate axis as one uncompressed chunk.
fn write_axis(
    store: &Arc<FilesystemStore>,
    path: &str,
    dimension: &str,
    values: &[f32],
) -> Result<(), RISICOError> {
    let len = values.len() as u64;
    let mut builder = ArrayBuilder::new(vec![len], vec![len], DataType::Float32, 0.0f32);
    builder.dimension_names(Some([dimension]));
    let array = builder
        .build(store.clone(), path)
        .map_err(|error| format!("cannot create Zarr axis {path}: {error}"))?;
    array
        .store_metadata()
        .map_err(|error| format!("cannot store Zarr axis metadata {path}: {error}"))?;
    array
        .store_array_subset_elements(&ArraySubset::new_with_shape(vec![len]), values)
        .map_err(|error| format!("cannot store Zarr axis {path}: {error}"))?;
    Ok(())
}
