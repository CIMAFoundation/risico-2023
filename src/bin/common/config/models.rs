use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use std::f32::consts::PI;
use std::fs;
use std::sync::Arc;

use chrono::*;
use chrono::{DateTime, Utc};
use log::{info, warn};
use rayon::prelude::*;
use risico::{
    models::output::{Output, OutputVariableName},
    modules::angstrom::models::{
        AngstromCellPropertiesContainer, AngstromProperties, AngstromPropertiesElement,
        AngstromState, AngstromStateElement,
    },
    modules::fosberg::models::{
        FosbergCellPropertiesContainer, FosbergProperties, FosbergPropertiesElement, FosbergState,
        FosbergStateElement,
    },
    modules::fwi::{
        config::FWIModelConfig,
        models::{
            FWICellPropertiesContainer, FWIProperties, FWIPropertiesElement, FWIState,
            FWIStateElement, FWIWarmState,
        },
    },
    //    modules::portuguese::models::{PortugueseCellPropertiesContainer, PortugueseProperties, PortugueseState, PortugueseWarmState},
    modules::hdw::models::{
        HdwCellPropertiesContainer, HdwProperties, HdwPropertiesElement, HdwState, HdwStateElement,
    },
    modules::kbdi::{
        config::KBDIModelConfig,
        models::{
            KBDICellPropertiesContainer, KBDIProperties, KBDIPropertiesElement, KBDIState,
            KBDIStateElement, KBDIWarmState,
        },
    },
    modules::mark5::{
        config::Mark5ModelConfig,
        models::{
            Mark5CellPropertiesContainer, Mark5Properties, Mark5PropertiesElement, Mark5State,
            Mark5StateElement, Mark5WarmState,
        },
    },
    modules::nesterov::models::{
        NesterovCellPropertiesContainer, NesterovProperties, NesterovPropertiesElement,
        NesterovState, NesterovStateElement, NesterovWarmState,
    },
    modules::orieux::models::{
        OrieuxCellPropertiesContainer, OrieuxProperties, OrieuxPropertiesElement, OrieuxState,
        OrieuxStateElement, OrieuxWarmState,
    },
    modules::risico::{
        config::RISICOModelConfig,
        models::{
            RISICOCellPropertiesContainer, RISICOProperties, RISICOPropertiesElement, RISICOState,
            RISICOStateElement, RISICOVegetation, RISICOWarmState,
        },
    },
    modules::sharples::models::{
        SharplesCellPropertiesContainer, SharplesProperties, SharplesPropertiesElement,
        SharplesState, SharplesStateElement,
    },
};

use super::builder::{
    AngstromConfigBuilder,
    FWIConfigBuilder,
    FosbergConfigBuilder,
    //    PortugueseConfigBuilder,
    HdwConfigBuilder,
    KbdiConfigBuilder,
    Mark5ConfigBuilder,
    MissingWarmStatePolicy,
    NesterovConfigBuilder,
    OrieuxConfigBuilder,
    OutputTypeConfig,
    RISICOConfigBuilder,
    SharplesConfigBuilder,
    StaticDataConfig,
    StreamingExecutionConfig,
    TileStateResidency,
    WarmStateConfig,
};

use crate::common::helpers::RISICOError;
use crate::common::io::models::{
    output::{NativeOutputSource, OutputType, TiledOutputSource},
    palette::Palette,
};
use crate::common::io::static_data::geotiff::{RasterDomain, RasterGrid};
use crate::common::io::streaming::{SpatialTile, TilePlan, TiledNativeOutputs};
use crate::common::io::warm_state::netcdf::{
    load_latest_fwi, load_latest_risico, write_fwi_snapshot, write_risico_snapshot,
};

pub type PaletteMap = HashMap<String, Box<Palette>>;
// pub type ConfigMap = HashMap<String, Vec<String>>;

pub fn check_write_warm_state(time: &DateTime<Utc>, warm_state_hour: i64) -> bool {
    time.hour() as i64 == warm_state_hour
}

pub const WARM_STATE_HOUR: i64 = 0; // hour for writing warm state
pub const WARM_STATE_LAG_DAYS: i64 = 1; // number of days before the run date to search for the warm state file

fn warm_state_search_time(run_date: DateTime<Utc>, hour: i64, days_before: i64) -> DateTime<Utc> {
    run_date - Duration::try_days(days_before).expect("Should be valid")
        + Duration::try_hours(hour).expect("Should be valid")
}

pub fn find_warm_state(
    base_warm_file: &str,
    run_date: DateTime<Utc>,
    hour: i64,
    lag_days: i64,
) -> (Option<File>, DateTime<Utc>) {
    // for the last n days before date, try to read the warm state
    // compose the filename as base_warm_file_YYYYmmDDHHMM
    let mut current_date = run_date;
    let mut file: Option<File> = None;
    let end_search: i64 = lag_days + 4; // search for warm state files up to 4 days before the lag_days
    for days_before in lag_days..end_search {
        current_date = warm_state_search_time(run_date, hour, days_before);
        let filename = format!("{}{}", base_warm_file, current_date.format("%Y%m%d%H%M"));
        let file_handle = File::open(filename);
        if file_handle.is_err() {
            continue;
        }
        file = Some(file_handle.expect("Should unwrap"));
        break;
    }
    (file, current_date)
}

pub struct RISICOConfig {
    run_date: DateTime<Utc>,
    netcdf_warm_state_path: String,
    cell_indexes: Vec<u32>,
    grid: Option<RasterGrid>,
    warm_state: Vec<RISICOWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: RISICOProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct FWIConfig {
    run_date: DateTime<Utc>,
    netcdf_warm_state_path: String,
    cell_indexes: Vec<u32>,
    grid: Option<RasterGrid>,
    warm_state: Vec<FWIWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: FWIProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_time_offset: i64,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct Mark5Config {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<Mark5WarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: Mark5Properties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct KbdiConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<KBDIWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: KBDIProperties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct AngstromConfig {
    run_date: DateTime<Utc>,
    properties: AngstromProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct FosbergConfig {
    run_date: DateTime<Utc>,
    properties: FosbergProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct NesterovConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<NesterovWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: NesterovProperties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct SharplesConfig {
    run_date: DateTime<Utc>,
    properties: SharplesProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

pub struct OrieuxConfig {
    run_date: DateTime<Utc>,
    warm_state_path: String,
    warm_state: Vec<OrieuxWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: OrieuxProperties,
    palettes: PaletteMap,
    output_types_defs: Vec<OutputTypeConfig>,
}

// pub struct PortugueseConfig {
//     run_date: DateTime<Utc>,
//     warm_state_path: String,
//     warm_state: Vec<PortugueseWarmState>,
//     warm_state_time: DateTime<Utc>,
//     warm_state_hour: i64,
//     properties: PortugueseProperties,
//     palettes: PaletteMap,
//     output_types_defs: Vec<OutputTypeConfig>,
// }

pub struct HdwConfig {
    run_date: DateTime<Utc>,
    properties: HdwProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
}

/// Common spatial batching contract used by every enabled model.
///
/// Stateful and stateless models differ in their tile adapters, but selecting
/// the unit of work is independent of the numerical model.
pub trait TiledModelConfig {
    fn tile_plan(&self, execution: &StreamingExecutionConfig) -> Result<TilePlan, RISICOError>;
    fn output_types(&self) -> &[OutputTypeConfig];

    fn native_output_variables(&self) -> Vec<OutputVariableName> {
        let mut variables = Vec::new();
        for variable in self
            .output_types()
            .iter()
            .flat_map(|output| output.variables.iter())
            .map(|variable| variable.internal_name())
        {
            if !variables.contains(&variable) {
                variables.push(variable);
            }
        }
        variables
    }
}

impl TiledModelConfig for RISICOConfig {
    fn tile_plan(&self, execution: &StreamingExecutionConfig) -> Result<TilePlan, RISICOError> {
        match &self.grid {
            Some(grid) => TilePlan::raster(
                grid.clone(),
                &self.cell_indexes,
                execution.tile_height,
                execution.tile_width,
            ),
            None => TilePlan::cells(self.properties.len, execution.cells_per_tile),
        }
    }

    fn output_types(&self) -> &[OutputTypeConfig] {
        &self.output_types_defs
    }
}

impl TiledModelConfig for FWIConfig {
    fn tile_plan(&self, execution: &StreamingExecutionConfig) -> Result<TilePlan, RISICOError> {
        match &self.grid {
            Some(grid) => TilePlan::raster(
                grid.clone(),
                &self.cell_indexes,
                execution.tile_height,
                execution.tile_width,
            ),
            None => TilePlan::cells(self.properties.len, execution.cells_per_tile),
        }
    }

    fn output_types(&self) -> &[OutputTypeConfig] {
        &self.output_types_defs
    }
}

macro_rules! impl_legacy_tiled_model {
    ($config:ty) => {
        impl TiledModelConfig for $config {
            fn tile_plan(
                &self,
                execution: &StreamingExecutionConfig,
            ) -> Result<TilePlan, RISICOError> {
                TilePlan::cells(self.properties.len, execution.cells_per_tile)
            }

            fn output_types(&self) -> &[OutputTypeConfig] {
                &self.output_types_defs
            }
        }
    };
}

impl_legacy_tiled_model!(Mark5Config);
impl_legacy_tiled_model!(KbdiConfig);
impl_legacy_tiled_model!(AngstromConfig);
impl_legacy_tiled_model!(FosbergConfig);
impl_legacy_tiled_model!(NesterovConfig);
impl_legacy_tiled_model!(SharplesConfig);
impl_legacy_tiled_model!(OrieuxConfig);
impl_legacy_tiled_model!(HdwConfig);

/// Construct a model's existing in-memory types for one spatial tile.
///
/// This keeps numerical code unchanged: adapters only gather immutable
/// properties and initial state, and every model continues to produce the
/// shared `Output` representation.
pub trait TileModelFactory: TiledModelConfig {
    type Properties;
    type State;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties;
    fn tile_state(&self, tile: &SpatialTile) -> Self::State;

    /// Drop the whole-domain property arrays once every tile's properties
    /// have been built from them.
    ///
    /// Tile properties are immutable, so the runner builds them once and
    /// keeps them; the domain arrays they were gathered from would then be a
    /// second copy held for nothing. Coordinates must be taken before this.
    fn release_properties(&mut self) {}

    /// Drop the whole-domain warm state once every tile has been seeded from
    /// it and checkpointed.
    ///
    /// It is read exactly once per run, at the first timestamp, but it is one
    /// record per domain cell and the models that carry per-cell histories make
    /// it the largest array a run holds. Afterwards a tile resumes from its own
    /// checkpoint, so keeping it would be paying for the whole run what only
    /// the first step needs. Models without a warm state need no override.
    fn release_warm_state(&mut self) {}
}

fn select_cells<T: Clone>(data: &ndarray::Array1<T>, tile: &SpatialTile) -> ndarray::Array1<T> {
    tile.model_positions
        .iter()
        .map(|&position| data[position].clone())
        .collect()
}

/// Take a tile's slice of the domain warm state.
///
/// An empty domain warm state means it has already been released: every tile
/// was seeded from it and checkpointed, so the caller is about to overwrite
/// these records with the checkpoint and only needs a correctly sized shell.
fn select_warm_state<T: Clone + Default>(data: &[T], tile: &SpatialTile) -> Vec<T> {
    if data.is_empty() {
        return vec![T::default(); tile.len()];
    }
    tile.model_positions
        .iter()
        .map(|&position| data[position].clone())
        .collect()
}

impl TileModelFactory for RISICOConfig {
    type Properties = RISICOProperties;
    type State = RISICOState;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
        let data = select_cells(&self.properties.data, tile);
        RISICOProperties {
            len: data.len(),
            data,
            vegetations_dict: self.properties.vegetations_dict.clone(),
        }
    }

    fn tile_state(&self, tile: &SpatialTile) -> Self::State {
        RISICOState::new(
            &select_warm_state(&self.warm_state, tile),
            &self.warm_state_time,
            RISICOModelConfig::new(&self.model_version),
        )
    }

    fn release_properties(&mut self) {
        self.properties.data = ndarray::Array1::from_vec(Vec::new());
    }

    fn release_warm_state(&mut self) {
        self.warm_state = Vec::new();
    }
}

impl TileModelFactory for FWIConfig {
    type Properties = FWIProperties;
    type State = FWIState;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
        let data = select_cells(&self.properties.data, tile);
        FWIProperties {
            len: data.len(),
            data,
        }
    }

    fn tile_state(&self, tile: &SpatialTile) -> Self::State {
        FWIState::new(
            &select_warm_state(&self.warm_state, tile),
            &self.warm_state_time,
            FWIModelConfig::new(&self.model_version),
        )
    }

    fn release_properties(&mut self) {
        self.properties.data = ndarray::Array1::from_vec(Vec::new());
    }

    fn release_warm_state(&mut self) {
        self.warm_state = Vec::new();
    }
}

impl TileModelFactory for Mark5Config {
    type Properties = Mark5Properties;
    type State = Mark5State;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
        let data = select_cells(&self.properties.data, tile);
        Mark5Properties {
            len: data.len(),
            data,
        }
    }

    fn tile_state(&self, tile: &SpatialTile) -> Self::State {
        Mark5State::new(
            &select_warm_state(&self.warm_state, tile),
            &self.warm_state_time,
            Mark5ModelConfig::new(&self.model_version),
        )
    }

    fn release_properties(&mut self) {
        self.properties.data = ndarray::Array1::from_vec(Vec::new());
    }

    fn release_warm_state(&mut self) {
        self.warm_state = Vec::new();
    }
}

impl TileModelFactory for KbdiConfig {
    type Properties = KBDIProperties;
    type State = KBDIState;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
        let data = select_cells(&self.properties.data, tile);
        KBDIProperties {
            len: data.len(),
            data,
        }
    }

    fn tile_state(&self, tile: &SpatialTile) -> Self::State {
        KBDIState::new(
            &select_warm_state(&self.warm_state, tile),
            &self.warm_state_time,
            KBDIModelConfig::new(&self.model_version),
        )
    }

    fn release_properties(&mut self) {
        self.properties.data = ndarray::Array1::from_vec(Vec::new());
    }

    fn release_warm_state(&mut self) {
        self.warm_state = Vec::new();
    }
}

macro_rules! impl_stateless_tile_factory {
    ($config:ident, $properties:ident, $state:ident) => {
        impl TileModelFactory for $config {
            type Properties = $properties;
            type State = $state;

            fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
                let data = select_cells(&self.properties.data, tile);
                $properties {
                    len: data.len(),
                    data,
                }
            }

            fn tile_state(&self, tile: &SpatialTile) -> Self::State {
                $state::new(&self.run_date, tile.len())
            }

            fn release_properties(&mut self) {
                self.properties.data = ndarray::Array1::from_vec(Vec::new());
            }
        }
    };
}

impl_stateless_tile_factory!(AngstromConfig, AngstromProperties, AngstromState);
impl_stateless_tile_factory!(FosbergConfig, FosbergProperties, FosbergState);
impl_stateless_tile_factory!(SharplesConfig, SharplesProperties, SharplesState);
impl_stateless_tile_factory!(HdwConfig, HdwProperties, HdwState);

impl TileModelFactory for NesterovConfig {
    type Properties = NesterovProperties;
    type State = NesterovState;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
        let data = select_cells(&self.properties.data, tile);
        NesterovProperties {
            len: data.len(),
            data,
        }
    }

    fn tile_state(&self, tile: &SpatialTile) -> Self::State {
        NesterovState::new(
            &select_warm_state(&self.warm_state, tile),
            &self.warm_state_time,
        )
    }

    fn release_properties(&mut self) {
        self.properties.data = ndarray::Array1::from_vec(Vec::new());
    }

    fn release_warm_state(&mut self) {
        self.warm_state = Vec::new();
    }
}

impl TileModelFactory for OrieuxConfig {
    type Properties = OrieuxProperties;
    type State = OrieuxState;

    fn tile_properties(&self, tile: &SpatialTile) -> Self::Properties {
        let data = select_cells(&self.properties.data, tile);
        OrieuxProperties {
            len: data.len(),
            data,
        }
    }

    fn tile_state(&self, tile: &SpatialTile) -> Self::State {
        OrieuxState::new(
            &select_warm_state(&self.warm_state, tile),
            &self.warm_state_time,
        )
    }

    fn release_properties(&mut self) {
        self.properties.data = ndarray::Array1::from_vec(Vec::new());
    }

    fn release_warm_state(&mut self) {
        self.warm_state = Vec::new();
    }
}

/// Disk-backed persistence for live tile state between time-major steps.
///
/// The checkpoint contains the exact model state elements, including transient
/// daily accumulators and history vectors; warm-state records alone are not
/// sufficient to resume every model at the next input timestamp.
pub trait TileStatePersistence: TileModelFactory {
    fn restore_tile_state(&self, state: &mut Self::State, path: &Path) -> Result<(), RISICOError>;
    fn checkpoint_tile_state(&self, state: &Self::State, path: &Path) -> Result<(), RISICOError>;
}

fn write_tile_checkpoint<T: ::serde::Serialize>(
    path: &Path,
    time: DateTime<Utc>,
    data: &[T],
) -> Result<(), RISICOError> {
    let file = File::create(path).map_err(|error| {
        format!(
            "cannot create tile state checkpoint {}: {error}",
            path.display()
        )
    })?;
    let mut writer = BufWriter::new(file);
    bincode::serialize_into(&mut writer, &(time, data)).map_err(|error| {
        format!(
            "cannot encode tile state checkpoint {}: {error}",
            path.display()
        )
    })?;
    writer.flush().map_err(|error| {
        format!(
            "cannot flush tile state checkpoint {}: {error}",
            path.display()
        )
        .into()
    })
}

fn read_tile_checkpoint<T: ::serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<(DateTime<Utc>, Vec<T>), RISICOError> {
    let file = File::open(path).map_err(|error| {
        format!(
            "cannot open tile state checkpoint {}: {error}",
            path.display()
        )
    })?;
    bincode::deserialize_from(BufReader::new(file)).map_err(|error| {
        format!(
            "cannot decode tile state checkpoint {}: {error}",
            path.display()
        )
        .into()
    })
}

macro_rules! impl_tile_state_persistence {
    ($config:ty, $state:ty, $element:ty) => {
        impl TileStatePersistence for $config {
            fn restore_tile_state(
                &self,
                state: &mut $state,
                path: &Path,
            ) -> Result<(), RISICOError> {
                let expected_len = state.data.len();
                // Release the freshly constructed initial-state elements
                // before decoding the checkpoint, avoiding two tile states at
                // peak during every restore after the first timestamp.
                state.data = Vec::new().into();
                let (time, data): (DateTime<Utc>, Vec<$element>) = read_tile_checkpoint(path)?;
                if data.len() != expected_len {
                    return Err(format!(
                        "tile state checkpoint {} has {} cells, expected {expected_len}",
                        path.display(),
                        data.len()
                    )
                    .into());
                }
                state.time = time;
                state.data = data.into();
                Ok(())
            }

            fn checkpoint_tile_state(
                &self,
                state: &$state,
                path: &Path,
            ) -> Result<(), RISICOError> {
                write_tile_checkpoint(
                    path,
                    state.time,
                    state
                        .data
                        .as_slice()
                        .expect("model tile state is contiguous"),
                )
            }
        }
    };
}

impl_tile_state_persistence!(RISICOConfig, RISICOState, RISICOStateElement);
impl_tile_state_persistence!(FWIConfig, FWIState, FWIStateElement);
impl_tile_state_persistence!(Mark5Config, Mark5State, Mark5StateElement);
impl_tile_state_persistence!(KbdiConfig, KBDIState, KBDIStateElement);
impl_tile_state_persistence!(AngstromConfig, AngstromState, AngstromStateElement);
impl_tile_state_persistence!(FosbergConfig, FosbergState, FosbergStateElement);
impl_tile_state_persistence!(NesterovConfig, NesterovState, NesterovStateElement);
impl_tile_state_persistence!(SharplesConfig, SharplesState, SharplesStateElement);
impl_tile_state_persistence!(OrieuxConfig, OrieuxState, OrieuxStateElement);
impl_tile_state_persistence!(HdwConfig, HdwState, HdwStateElement);

/// What one model costs in memory, per cell, so a budget can be turned into a
/// tile concurrency.
///
/// Both figures are of the model's own arrays. Input decoding, the output
/// writer and the source-grid caches sit outside them, so a run always needs
/// some headroom above what this predicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileMemoryModel {
    /// Held for every domain cell for the whole run, whatever the tiling is.
    /// Excludes the live state, which is charged by `state_per_cell`.
    pub resident_per_cell: usize,
    /// Held per tile cell only while that tile is in flight. Excludes the
    /// live state.
    pub in_flight_per_cell: usize,
    /// One cell's live model state, heap included. Charged to the whole
    /// domain when the state stays in memory, to a tile in flight when it is
    /// checkpointed.
    pub state_per_cell: usize,
    /// One cell's warm-state record, as gathered for a snapshot. A timestep
    /// that writes one holds the whole domain's records until they are on
    /// disk.
    pub warm_state_per_cell: usize,
}

/// Per-cell memory cost of a model, used to derive tile concurrency.
pub trait TileMemoryFootprint {
    fn memory_model(&self) -> TileMemoryModel;
}

/// A model's per-cell costs applied to one tile plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanMemoryEstimate {
    /// Bytes held for the whole run, whatever the concurrency.
    pub resident: u64,
    /// Bytes one tile in flight adds, sized from the largest tile so the
    /// figure holds for every step of the plan rather than on average.
    pub per_tile: u64,
}

impl TileMemoryModel {
    /// The plan's cost with the live state either held for the whole domain
    /// or carried by the tiles in flight.
    pub fn estimate(&self, plan: &TilePlan, resident_state: bool) -> PlanMemoryEstimate {
        let cell_count = plan.cell_count() as u64;
        let largest_tile = plan
            .tiles()
            .iter()
            .map(|tile| tile.len())
            .max()
            .unwrap_or(0) as u64;
        let state = self.state_per_cell as u64;
        let (resident_state, in_flight_state) = if resident_state {
            (state, 0)
        } else {
            (0, state)
        };
        PlanMemoryEstimate {
            resident: cell_count * (self.resident_per_cell as u64 + resident_state),
            per_tile: largest_tile * (self.in_flight_per_cell as u64 + in_flight_state),
        }
    }

    /// What holding every tile's state in memory costs for this plan.
    pub fn state_bytes(&self, plan: &TilePlan) -> u64 {
        plan.cell_count() as u64 * self.state_per_cell as u64
    }

    /// What one timestep's gathered warm-state records cost for this plan.
    pub fn warm_state_bytes(&self, plan: &TilePlan) -> u64 {
        plan.cell_count() as u64 * self.warm_state_per_cell as u64
    }
}

/// What finishing one timestep costs in memory while it is being written.
///
/// Every output variable in flight holds one accumulator over its output grid
/// (a value and a count per pixel) and then the finished grid, and each of
/// the slots a variable is split over reads one tile at a time.
pub fn postprocess_peak_bytes(
    grid_pixels: impl IntoIterator<Item = usize>,
    cores: usize,
    largest_tile: usize,
) -> u64 {
    const ACCUMULATOR_AND_RESULT: u64 = 3 * std::mem::size_of::<f32>() as u64;
    let mut pixels: Vec<u64> = grid_pixels
        .into_iter()
        .map(|pixels| pixels as u64)
        .collect();
    if pixels.is_empty() {
        return 0;
    }
    let cores = cores.max(1);
    pixels.sort_unstable_by(|left, right| right.cmp(left));
    let grids: u64 = pixels.iter().take(cores).sum();
    let tile_buffers = cores as u64 * largest_tile as u64 * std::mem::size_of::<f32>() as u64;
    grids * ACCUMULATOR_AND_RESULT + tile_buffers
}

/// How one model run is laid out once the plan, the budget and the
/// configuration have been reconciled.
///
/// Decided once, before any tile runs, and shared by the runner and the
/// inspector so both describe the same run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionDecision {
    /// Whether every tile's state stays in memory between timesteps.
    pub resident_state: bool,
    /// How many tiles may run at once. `None` leaves it to rayon: one per
    /// core.
    pub tile_concurrency: Option<usize>,
    /// The plan's cost under the chosen residency.
    pub estimate: PlanMemoryEstimate,
    /// What the state costs when it is held in memory.
    pub state_bytes: u64,
    pub budget: Option<u64>,
    /// The most tiles the plan and the machine could run at once.
    pub ceiling: usize,
    /// Whether one timestep's output is written while the next is stepped.
    pub overlap_postprocess: bool,
    /// What finishing a timestep adds while it overlaps the next step: its
    /// output grids in flight and its gathered warm state.
    pub finish_bytes: u64,
}

impl ExecutionDecision {
    /// Reconcile the plan with the configuration.
    ///
    /// `output_bytes` is what the output writer holds while a timestep is
    /// resampled, from `postprocess_peak_bytes`.
    pub fn resolve(
        footprint: TileMemoryModel,
        execution: &StreamingExecutionConfig,
        plan: &TilePlan,
        cores: usize,
        output_bytes: u64,
    ) -> Result<Self, RISICOError> {
        let ceiling = plan.tiles().len().min(cores).max(1);
        let budget = execution.max_memory_bytes()?;
        let state_bytes = footprint.state_bytes(plan);
        let finish_bytes = output_bytes + footprint.warm_state_bytes(plan);

        let resident_state = match execution.tile_state {
            TileStateResidency::Memory => true,
            TileStateResidency::Disk => false,
            TileStateResidency::Auto => match budget {
                // Without a budget nothing asks for memory to be bounded, so
                // the state stays where it is cheapest to reach.
                None => true,
                Some(budget) => {
                    // Judge the memory layout at the concurrency it would run
                    // at: an explicit one, else the least that still fits.
                    let in_memory = footprint.estimate(plan, true);
                    let threads = execution.tile_concurrency.unwrap_or(1);
                    in_memory.at_concurrency(threads) <= budget
                }
            },
        };

        let estimate = footprint.estimate(plan, resident_state);
        let tile_concurrency = match (execution.tile_concurrency, budget) {
            (Some(threads), _) => Some(threads),
            (None, Some(budget)) => Some(estimate.tiles_within(budget, ceiling)),
            (None, None) => None,
        };

        let step_peak = estimate.at_concurrency(tile_concurrency.unwrap_or(ceiling));
        let overlap_postprocess = match (execution.overlap_postprocess, budget) {
            (Some(overlap), _) => overlap,
            (None, None) => true,
            (None, Some(budget)) => step_peak + finish_bytes <= budget,
        };

        Ok(Self {
            resident_state,
            tile_concurrency,
            estimate,
            state_bytes,
            budget,
            ceiling,
            overlap_postprocess,
            finish_bytes,
        })
    }

    /// The concurrency the run will actually use.
    pub fn effective_concurrency(&self) -> usize {
        self.tile_concurrency.unwrap_or(self.ceiling)
    }

    /// The resident set the estimate predicts for this run.
    pub fn expected_peak(&self) -> u64 {
        let step = self.estimate.at_concurrency(self.effective_concurrency());
        if self.overlap_postprocess {
            step + self.finish_bytes
        } else {
            step
        }
    }

    /// Whether the budget cannot be met even one tile at a time.
    pub fn over_budget(&self) -> bool {
        self.budget
            .is_some_and(|budget| self.estimate.at_concurrency(1) > budget)
    }
}

impl PlanMemoryEstimate {
    /// How many tiles a budget pays for once the whole-domain arrays are
    /// charged.
    ///
    /// Always at least one: a budget smaller than the domain's fixed cost
    /// cannot be honoured by tiling at all, and refusing to run would be less
    /// useful than running at the smallest footprint available.
    pub fn tiles_within(&self, budget: u64, ceiling: usize) -> usize {
        if self.per_tile == 0 {
            return ceiling.max(1);
        }
        let affordable = budget.saturating_sub(self.resident) / self.per_tile;
        (affordable as usize).clamp(1, ceiling.max(1))
    }

    /// The resident set this plan is expected to reach at a concurrency.
    pub fn at_concurrency(&self, tiles: usize) -> u64 {
        self.resident + self.per_tile * tiles as u64
    }
}

/// What the counted arrays miss for whole-domain data: the static layers held
/// while the properties are assembled, the handler's source-grid index caches
/// and the writer's buffers.
///
/// Calibrated against measured runs rather than derived, so it is deliberately
/// on the generous side: overestimating costs some parallelism, underestimating
/// overshoots the budget the operator asked for.
const RESIDENT_OVERHEAD: f64 = 1.75;

/// What the counted arrays miss for a tile in flight: the checkpoint
/// serialization buffers, the per-variable gather temporaries and allocator
/// retention.
///
/// Calibrated the same way and for the same reason. It used to also cover the
/// domain-sized arrays a timestep's output was joined into; those are gone, so
/// a tile in flight now costs little more than the arrays counted below.
const IN_FLIGHT_OVERHEAD: f64 = 1.5;

macro_rules! impl_tile_memory_footprint {
    ($config:ty, $properties:ty, $state:ty, $warm:ty) => {
        impl_tile_memory_footprint!($config, $properties, $state, $warm, 0);
    };
    ($config:ty, $properties:ty, $state:ty, $warm:ty, $state_heap:expr) => {
        impl TileMemoryFootprint for $config {
            fn memory_model(&self) -> TileMemoryModel {
                use std::mem::size_of;

                // Whole-domain arrays: the model's properties (gathered per
                // tile once, then held for the run), the coordinates and the
                // plan's cell bookkeeping, plus the warm state. The warm state
                // is released once the tiles are seeded, but it is held while
                // they are, so it belongs in the floor a run cannot get under
                // however few tiles are allowed at once.
                let resident_per_cell = size_of::<$properties>()
                    + size_of::<$warm>()
                    + 2 * size_of::<f32>()
                    + size_of::<usize>()
                    + size_of::<u32>();

                // A tile in flight holds the decoded input row, the warm
                // state records it hands back, the model's output elements,
                // and one f32 plane per output variable in the scratch it is
                // written to. Its properties were built once and are counted
                // with the domain; its working state is charged separately,
                // since where that lives is decided per run.
                let outputs = self.native_output_variables().len();
                let in_flight_per_cell = size_of::<risico::models::input::InputElement>()
                    + size_of::<$warm>()
                    + size_of::<risico::models::output::OutputElement>()
                    + outputs * size_of::<f32>();

                // The state element plus whatever it owns on the heap: models
                // with a per-cell history keep most of their state there.
                let state_per_cell = size_of::<$state>() + $state_heap;

                TileMemoryModel {
                    resident_per_cell: (resident_per_cell as f64 * RESIDENT_OVERHEAD) as usize,
                    in_flight_per_cell: (in_flight_per_cell as f64 * IN_FLIGHT_OVERHEAD) as usize,
                    state_per_cell: (state_per_cell as f64 * IN_FLIGHT_OVERHEAD) as usize,
                    warm_state_per_cell: size_of::<$warm>(),
                }
            }
        }
    };
}

impl_tile_memory_footprint!(
    RISICOConfig,
    RISICOPropertiesElement,
    RISICOStateElement,
    RISICOWarmState
);
/// Heap behind one cell's history of `entries` samples, each a timestamp and
/// `values` floats.
const fn history_heap(entries: usize, values: usize) -> usize {
    entries * (std::mem::size_of::<DateTime<Utc>>() + values * std::mem::size_of::<f32>())
}

// FWI keeps an hourly window of eight series; KBDI and Mark5 a daily window
// of one. Each holds one entry more than the window while a step is folded in.
impl_tile_memory_footprint!(
    FWIConfig,
    FWIPropertiesElement,
    FWIStateElement,
    FWIWarmState,
    history_heap(risico::modules::fwi::constants::TIME_WINDOW as usize + 1, 8)
);
impl_tile_memory_footprint!(
    Mark5Config,
    Mark5PropertiesElement,
    Mark5StateElement,
    Mark5WarmState,
    history_heap(
        risico::modules::mark5::constants::TIME_WINDOW as usize + 1,
        1
    )
);
impl_tile_memory_footprint!(
    KbdiConfig,
    KBDIPropertiesElement,
    KBDIStateElement,
    KBDIWarmState,
    history_heap(
        risico::modules::kbdi::constants::TIME_WINDOW as usize + 1,
        1
    )
);
impl_tile_memory_footprint!(
    NesterovConfig,
    NesterovPropertiesElement,
    NesterovStateElement,
    NesterovWarmState
);
impl_tile_memory_footprint!(
    OrieuxConfig,
    OrieuxPropertiesElement,
    OrieuxStateElement,
    OrieuxWarmState
);
impl_tile_memory_footprint!(
    AngstromConfig,
    AngstromPropertiesElement,
    AngstromStateElement,
    ()
);
impl_tile_memory_footprint!(
    FosbergConfig,
    FosbergPropertiesElement,
    FosbergStateElement,
    ()
);
impl_tile_memory_footprint!(
    SharplesConfig,
    SharplesPropertiesElement,
    SharplesStateElement,
    ()
);
impl_tile_memory_footprint!(HdwConfig, HdwPropertiesElement, HdwStateElement, ());

pub struct TileStep {
    pub output: Option<Output>,
    pub write_warm_state: bool,
}

/// Model-specific operations used by the single production tile runner.
///
/// The runner owns spatial batching and persistence. Implementations retain
/// only the small differences in each model's store/update/output schedule.
pub trait TileModelRuntime: TileModelFactory + TileStatePersistence + TileMemoryFootprint {
    type WarmState: Clone + Default;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>);
    fn output_writer(&self) -> Result<OutputWriter, RISICOError>;
    fn step(
        &self,
        state: &mut Self::State,
        properties: &Self::Properties,
        input: &risico::models::input::Input,
    ) -> TileStep;
    fn tile_warm_state(&self, state: &Self::State) -> Vec<Self::WarmState>;
    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError>;
}

impl TileModelRuntime for RISICOConfig {
    type WarmState = RISICOWarmState;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.properties.get_coords()
    }

    fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
        self.get_output_writer()
    }

    fn step(
        &self,
        state: &mut RISICOState,
        properties: &RISICOProperties,
        input: &risico::models::input::Input,
    ) -> TileStep {
        state.update(properties, input);
        TileStep {
            output: self
                .should_write_output(&state.time)
                .then(|| state.output(properties, input)),
            write_warm_state: self.should_write_warm_state(&state.time),
        }
    }

    fn tile_warm_state(&self, state: &RISICOState) -> Vec<Self::WarmState> {
        state
            .data
            .iter()
            .map(|state| RISICOWarmState {
                dffm: state.dffm,
                snow_cover: state.snow_cover,
                snow_cover_time: state.snow_cover_time,
                MSI: state.MSI,
                MSI_TTL: state.MSI_TTL,
                NDVI: state.NDVI,
                NDVI_TIME: state.NDVI_TIME,
                NDWI: state.NDWI,
                NDWI_TIME: state.NDWI_TIME,
            })
            .collect()
    }

    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let state = RISICOState::new(records, &time, RISICOModelConfig::new(&self.model_version));
        self.write_warm_state(&state, time)
    }
}

impl TileModelRuntime for FWIConfig {
    type WarmState = FWIWarmState;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.properties.get_coords()
    }

    fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
        self.get_output_writer()
    }

    fn step(
        &self,
        state: &mut FWIState,
        properties: &FWIProperties,
        input: &risico::models::input::Input,
    ) -> TileStep {
        state.update(properties, input);
        TileStep {
            output: self
                .should_write_output(&state.time)
                .then(|| state.output(properties)),
            write_warm_state: self.should_write_warm_state(&state.time),
        }
    }

    fn tile_warm_state(&self, state: &FWIState) -> Vec<Self::WarmState> {
        state
            .data
            .iter()
            .map(|state| FWIWarmState {
                dates: state.dates.clone(),
                ffmc: state.ffmc.clone(),
                dmc: state.dmc.clone(),
                dc: state.dc.clone(),
                rain: state.rain.clone(),
            })
            .collect()
    }

    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let state = FWIState::new(records, &time, FWIModelConfig::new(&self.model_version));
        self.write_warm_state(&state, time)
    }
}

impl TileModelRuntime for Mark5Config {
    type WarmState = Mark5WarmState;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.properties.get_coords()
    }

    fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
        self.get_output_writer()
    }

    fn step(
        &self,
        state: &mut Mark5State,
        properties: &Mark5Properties,
        input: &risico::models::input::Input,
    ) -> TileStep {
        state.store(input, properties);
        let write = self.should_write_warm_state(&state.time);
        TileStep {
            output: write.then(|| state.output(properties)),
            write_warm_state: write,
        }
    }

    fn tile_warm_state(&self, state: &Mark5State) -> Vec<Self::WarmState> {
        state
            .data
            .iter()
            .map(|state| Mark5WarmState {
                dates: state.dates.clone(),
                daily_rain: state.daily_rain.clone(),
                smd: state.smd,
            })
            .collect()
    }

    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let state = Mark5State::new(records, &time, Mark5ModelConfig::new(&self.model_version));
        self.write_warm_state(&state, time)
    }
}

impl TileModelRuntime for KbdiConfig {
    type WarmState = KBDIWarmState;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.properties.get_coords()
    }

    fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
        self.get_output_writer()
    }

    fn step(
        &self,
        state: &mut KBDIState,
        properties: &KBDIProperties,
        input: &risico::models::input::Input,
    ) -> TileStep {
        state.store(input);
        let write = self.should_write_warm_state(&state.time);
        if write {
            state.update(properties);
        }
        TileStep {
            output: write.then(|| state.output()),
            write_warm_state: write,
        }
    }

    fn tile_warm_state(&self, state: &KBDIState) -> Vec<Self::WarmState> {
        state
            .data
            .iter()
            .map(|state| KBDIWarmState {
                dates: state.dates.clone(),
                daily_rain: state.daily_rain.clone(),
                kbdi: state.kbdi,
            })
            .collect()
    }

    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let state = KBDIState::new(records, &time, KBDIModelConfig::new(&self.model_version));
        self.write_warm_state(&state, time)
    }
}

macro_rules! impl_stateless_tile_runtime {
    ($config:ty, $state:ty, $properties:ty) => {
        impl TileModelRuntime for $config {
            type WarmState = ();

            fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
                self.properties.get_coords()
            }

            fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
                self.get_output_writer()
            }

            fn step(
                &self,
                state: &mut $state,
                _properties: &$properties,
                input: &risico::models::input::Input,
            ) -> TileStep {
                state.store(input);
                TileStep {
                    output: self
                        .should_write_output(&state.time)
                        .then(|| state.output()),
                    write_warm_state: false,
                }
            }

            fn tile_warm_state(&self, _state: &$state) -> Vec<Self::WarmState> {
                Vec::new()
            }

            fn write_warm_state_records(
                &self,
                _records: &[Self::WarmState],
                _time: DateTime<Utc>,
            ) -> Result<(), RISICOError> {
                Ok(())
            }
        }
    };
}

impl_stateless_tile_runtime!(AngstromConfig, AngstromState, AngstromProperties);
impl_stateless_tile_runtime!(FosbergConfig, FosbergState, FosbergProperties);
impl_stateless_tile_runtime!(SharplesConfig, SharplesState, SharplesProperties);
impl_stateless_tile_runtime!(HdwConfig, HdwState, HdwProperties);

impl TileModelRuntime for NesterovConfig {
    type WarmState = NesterovWarmState;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.properties.get_coords()
    }

    fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
        self.get_output_writer()
    }

    fn step(
        &self,
        state: &mut NesterovState,
        properties: &NesterovProperties,
        input: &risico::models::input::Input,
    ) -> TileStep {
        state.store(input, properties);
        let write = self.should_write_warm_state(&state.time);
        if write {
            state.update();
        }
        TileStep {
            output: write.then(|| state.output()),
            write_warm_state: write,
        }
    }

    fn tile_warm_state(&self, state: &NesterovState) -> Vec<Self::WarmState> {
        state
            .data
            .iter()
            .map(|state| NesterovWarmState {
                nesterov: state.nesterov,
            })
            .collect()
    }

    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let state = NesterovState::new(records, &time);
        self.write_warm_state(&state, time)
    }
}

impl TileModelRuntime for OrieuxConfig {
    type WarmState = OrieuxWarmState;

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.properties.get_coords()
    }

    fn output_writer(&self) -> Result<OutputWriter, RISICOError> {
        self.get_output_writer()
    }

    fn step(
        &self,
        state: &mut OrieuxState,
        properties: &OrieuxProperties,
        input: &risico::models::input::Input,
    ) -> TileStep {
        state.store(input);
        let write = self.should_write_warm_state(&state.time);
        if write {
            state.update(properties);
        }
        TileStep {
            output: write.then(|| state.output()),
            write_warm_state: write,
        }
    }

    fn tile_warm_state(&self, state: &OrieuxState) -> Vec<Self::WarmState> {
        state
            .data
            .iter()
            .map(|state| OrieuxWarmState {
                orieux_wr: state.orieux_wr,
            })
            .collect()
    }

    fn write_warm_state_records(
        &self,
        records: &[Self::WarmState],
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let state = OrieuxState::new(records, &time);
        self.write_warm_state(&state, time)
    }
}

pub struct OutputWriter {
    outputs: Vec<OutputType>,
}

impl OutputWriter {
    /// Build every configured output type.
    ///
    /// An output type that cannot be built (typically a missing grid file) is
    /// an error rather than a silent omission: a run that writes nothing
    /// otherwise looks successful and is only noticed downstream.
    pub fn new(
        outputs_defs: &[OutputTypeConfig],
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<Self, RISICOError> {
        let outputs = outputs_defs
            .iter()
            .map(|definition| {
                OutputType::new(definition, date, palettes).map_err(|error| {
                    RISICOError::from(format!(
                        "cannot build output type {}: {error}",
                        definition.name
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { outputs })
    }

    /// Complete every output once the last timestep is written. Formats
    /// that keep files open across the run (NetCDF) write their metadata
    /// here; the others have nothing to do.
    pub fn finish(&mut self) -> Result<(), RISICOError> {
        for output_type in &mut self.outputs {
            output_type.finish().map_err(|error| {
                RISICOError::from(format!(
                    "cannot finish output {}: {error}",
                    output_type.name()
                ))
            })?;
        }
        Ok(())
    }

    /// What this writer holds while a timestep's variables are resampled.
    pub fn peak_bytes(&self, cores: usize, largest_tile: usize) -> u64 {
        let grids = self.outputs.iter().flat_map(|output| {
            let pixels = output.grid().nrows * output.grid().ncols;
            output.variables().iter().map(move |_| pixels)
        });
        postprocess_peak_bytes(grids, cores, largest_tile)
    }

    /// Write a timestep whose native output is held as one scratch per tile.
    pub fn write_tiled_output(
        &mut self,
        lats: &[f32],
        lons: &[f32],
        time: DateTime<Utc>,
        output: &TiledNativeOutputs,
    ) -> Result<(), RISICOError> {
        self.write_source(lats, lons, &TiledOutputSource::new(time, output))
    }

    /// Resample and write every configured variable of one timestep.
    ///
    /// Each variable is postprocessed on its own and holds a single output
    /// grid, so the timestep's cost is set by how many variables are in flight
    /// rather than by the domain. Variables run side by side; a variable only
    /// splits its own work when there are threads left over, which keeps the
    /// number of live output grids close to the number of cores.
    fn write_source(
        &mut self,
        lats: &[f32],
        lons: &[f32],
        output: &dyn NativeOutputSource,
    ) -> Result<(), RISICOError> {
        for output_type in &mut self.outputs {
            output_type.prepare()?;
        }

        let requests: Vec<(usize, usize)> = self
            .outputs
            .iter()
            .enumerate()
            .flat_map(|(index, output_type)| {
                (0..output_type.variables().len()).map(move |variable| (index, variable))
            })
            .collect();
        if requests.is_empty() {
            return Ok(());
        }
        let parallelism = (rayon::current_num_threads() / requests.len()).max(1);

        let outputs = &self.outputs;
        requests.par_iter().try_for_each(|&(index, variable)| {
            let output_type = &outputs[index];
            let variable = &output_type.variables()[variable];
            output_type
                .write_variable(variable, output, lats, lons, parallelism)
                .map_err(|error| {
                    RISICOError::from(format!(
                        "cannot write {} to output {}: {error}",
                        variable.output_name(),
                        output_type.name()
                    ))
                })
        })
    }
}

pub fn load_palettes(palettes_defs: &HashMap<String, String>) -> HashMap<String, Box<Palette>> {
    let mut palettes: HashMap<String, Box<Palette>> = HashMap::new();

    for (name, path) in palettes_defs.iter() {
        if let Ok(palette) = Palette::load_palette(path) {
            palettes.insert(name.to_string(), Box::new(palette));
        }
    }
    palettes
}

impl RISICOConfig {
    pub fn new(
        config_defs: &RISICOConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<RISICOConfig, RISICOError> {
        let palettes = load_palettes(palettes);

        let (props_container, (ppf_summer, ppf_winter), vegetations_dict, cell_indexes, grid) =
            match &config_defs.static_data {
                StaticDataConfig::GeoTiff {
                    domain_mask,
                    slope,
                    aspect,
                    vegetation_id,
                    vegetation_catalog,
                    ppf_summer,
                    ppf_winter,
                } => {
                    let domain = RasterDomain::open(domain_mask)?;
                    let (lats, lons) = domain.coordinates();
                    let slope = slope
                        .as_deref()
                        .ok_or("RISICO GeoTIFF static data requires slope")?;
                    let aspect = aspect
                        .as_deref()
                        .ok_or("RISICO GeoTIFF static data requires aspect")?;
                    let vegetation_id = vegetation_id
                        .as_deref()
                        .ok_or("RISICO GeoTIFF static data requires vegetation_id")?;
                    let vegetation_catalog = vegetation_catalog
                        .clone()
                        .ok_or("RISICO GeoTIFF static data requires vegetation_catalog")?;
                    let ppf_layers = match (ppf_summer, ppf_winter) {
                        (None, None) => None,
                        (Some(summer), Some(winter)) => Some((summer.as_str(), winter.as_str())),
                        _ => {
                            return Err(
                                "ppf_summer and ppf_winter must either both be configured or both omitted"
                                    .into(),
                            )
                        }
                    };

                    // Decoding a full-domain layer dominates configuration time and
                    // each layer opens its own file handle, so read them concurrently.
                    let mut requests = vec![
                        (slope, "slope"),
                        (aspect, "aspect"),
                        (vegetation_id, "vegetation_id"),
                    ];
                    if let Some((summer, winter)) = ppf_layers {
                        requests.push((summer, "ppf_summer"));
                        requests.push((winter, "ppf_winter"));
                    }
                    let layers = requests
                        .into_par_iter()
                        .map(|(path, name)| domain.read_required_layer(path, name))
                        .collect::<Result<Vec<_>, RISICOError>>()?;
                    let mut layers = layers.into_iter();
                    let slope_values = layers.next().expect("slope layer was requested");
                    let aspect_values = layers.next().expect("aspect layer was requested");
                    let vegetation_values =
                        layers.next().expect("vegetation_id layer was requested");

                    let mut slopes = slope_values;
                    slopes.par_iter_mut().for_each(|value| *value *= PI / 180.0);
                    let mut aspects = aspect_values;
                    aspects
                        .par_iter_mut()
                        .for_each(|value| *value *= PI / 180.0);
                    let vegetations_dict = RISICOConfig::read_vegetation(&vegetation_catalog)
                        .map_err(|error| format!("error reading {vegetation_catalog}, {error}"))?;
                    // Resolving each raster id against the catalog here keeps
                    // one shared handle per cell; formatting the id back into
                    // an owned string per cell would cost more than every other
                    // static layer put together.
                    let by_id: HashMap<i64, Arc<RISICOVegetation>> = vegetations_dict
                        .iter()
                        .filter_map(|(id, vegetation)| {
                            let parsed = id.parse::<i64>().ok()?;
                            (parsed.to_string() == *id).then(|| (parsed, vegetation.clone()))
                        })
                        .collect();
                    let default_vegetation = Arc::new(RISICOVegetation::default());
                    let vegetations = vegetation_values
                        .into_par_iter()
                        .map(|value| {
                            let rounded = value.round();
                            if !value.is_finite() || (value - rounded).abs() > 1.0e-4 {
                                Err(RISICOError::from(format!(
                                    "vegetation_id must contain finite integer values, found {value}"
                                )))
                            } else {
                                Ok(by_id
                                    .get(&(rounded as i64))
                                    .unwrap_or(&default_vegetation)
                                    .clone())
                            }
                        })
                        .collect::<Result<Vec<_>, RISICOError>>()?;

                    // Kept as two arrays rather than one of pairs: the model
                    // stores them separately, so pairing them up would only
                    // build a third domain-sized array to take apart again.
                    let ppf = match ppf_layers {
                        None => (
                            vec![1.0; domain.cell_indexes.len()],
                            vec![1.0; domain.cell_indexes.len()],
                        ),
                        Some(_) => {
                            let summer = layers.next().expect("ppf_summer layer was requested");
                            let winter = layers.next().expect("ppf_winter layer was requested");
                            (summer, winter)
                        }
                    };
                    let props = RISICOCellPropertiesContainer {
                        lats,
                        lons,
                        slopes,
                        aspects,
                        vegetations,
                    };
                    (
                        props,
                        ppf,
                        vegetations_dict,
                        domain.cell_indexes,
                        Some(domain.grid),
                    )
                }
            };

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len()
            || n_cells != props_container.slopes.len()
            || n_cells != props_container.aspects.len()
            || n_cells != props_container.vegetations.len()
        {
            panic!("All properties must have the same length");
        }
        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let WarmStateConfig::NetCdf {
            directory,
            max_age_hours,
            on_missing,
            ..
        } = &config_defs.warm_state;

        let grid_ref = grid
            .as_ref()
            .ok_or("NetCDF warm state requires GeoTIFF static_data so grid geometry is known")?;
        let netcdf_warm_state_path = directory.clone();
        // Match legacy lookup semantics: a run must not seed itself from a
        // snapshot written by an earlier execution of that same run.
        let latest_warm_state_time =
            warm_state_search_time(date, warm_state_hour, warm_state_lag_days);
        let (warm_state, warm_state_time) = if let Some(snapshot) = load_latest_risico(
            directory,
            latest_warm_state_time,
            max_age_hours.unwrap_or(120),
            &config_defs.model_version,
            grid_ref,
            &cell_indexes,
        )? {
            snapshot
        } else {
            match on_missing {
                MissingWarmStatePolicy::Defaults => (
                    vec![RISICOWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ),
                MissingWarmStatePolicy::Error => {
                    return Err(format!(
                        "no valid RISICO warm state found in {directory} and defaults are disabled"
                    )
                    .into())
                }
            }
        };
        if warm_state.len() != n_cells {
            return Err(format!(
                "RISICO warm state has {} cells but static data has {n_cells}",
                warm_state.len()
            )
            .into());
        }
        let props =
            RISICOProperties::new(props_container, vegetations_dict, ppf_summer, ppf_winter);

        let config = RISICOConfig {
            run_date: date,
            netcdf_warm_state_path,
            cell_indexes,
            grid,
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    /// Read the cells from a file.
    /// :param file_path: The path to the file.
    /// :return: A list of cells.
    pub fn read_vegetation(
        file_path: &str,
    ) -> Result<HashMap<String, Arc<RISICOVegetation>>, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("Can't open file: {err}."))?;
        let mut vegetations: HashMap<String, Arc<RISICOVegetation>> = HashMap::new();

        let reader = BufReader::new(file);

        for (index, line) in reader.lines().enumerate() {
            let line =
                line.map_err(|err| format!("Error reading {file_path} at line #{index}: {err}"))?;
            if index == 0 && line.starts_with("#") || line.is_empty() {
                // skip header and empty lines
                continue;
            }
            let line_elements: Vec<&str> = line.split_whitespace().collect::<Vec<&str>>();

            let n_elements = line_elements.len();
            if n_elements < 9 {
                return Err(format!("Invalid line in file {file_path}: {line}").into());
            }

            //  [TODO] refactor this for using error handling
            let id = line_elements[0].to_string();
            let d0 = line_elements[1].parse::<f32>().map_err(|_| {
                format!("Invalid `d0` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let d1 = line_elements[2].parse::<f32>().map_err(|_| {
                format!("Invalid `d1` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let hhv = line_elements[3].parse::<f32>().map_err(|_| {
                format!("Invalid `hhv` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let umid = line_elements[4].parse::<f32>().map_err(|_| {
                format!("Invalid `umid` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let v0 = line_elements[5].parse::<f32>().map_err(|_| {
                format!("Invalid `v0` value in file {file_path} at line #{index}: '{line}'")
            })?;
            #[allow(non_snake_case)]
            let T0 = line_elements[6].parse::<f32>().map_err(|_| {
                format!("Invalid `T0` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let sat = line_elements[7].parse::<f32>().map_err(|_| {
                format!("Invalid `sat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let use_ndvi = match n_elements {
                10.. => line_elements[8].parse::<bool>().map_err(|_| {
                    format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
                })?,
                _ => false,
            };
            let name = line_elements[n_elements - 1].to_string();

            let veg_id = id.clone();

            let veg = Arc::new(RISICOVegetation {
                id,
                d0,
                d1,
                hhv,
                umid,
                v0,
                T0,
                sat,
                name,
                use_ndvi,
            });

            vegetations.insert(veg_id, veg);
        }

        Result::Ok(vegetations)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    pub fn write_warm_state(
        &self,
        state: &RISICOState,
        _warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let grid = self
            .grid
            .as_ref()
            .ok_or("NetCDF warm state is missing its grid")?;
        write_risico_snapshot(
            &self.netcdf_warm_state_path,
            state,
            &self.model_version,
            grid,
            &self.cell_indexes,
        )?;
        Ok(())
    }
}

impl FWIConfig {
    pub fn new(
        config_defs: &FWIConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<FWIConfig, RISICOError> {
        let palettes = load_palettes(palettes);

        let StaticDataConfig::GeoTiff { domain_mask, .. } = &config_defs.static_data;
        let domain = RasterDomain::open(domain_mask)?;
        let (lats, lons) = domain.coordinates();
        let (props_container, cell_indexes, grid) = (
            FWICellPropertiesContainer { lats, lons },
            domain.cell_indexes,
            Some(domain.grid),
        );

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }

        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let WarmStateConfig::NetCdf {
            directory,
            max_age_hours,
            on_missing,
            ..
        } = &config_defs.warm_state;

        let grid_ref = grid
            .as_ref()
            .ok_or("NetCDF warm state requires GeoTIFF static_data so grid geometry is known")?;
        let netcdf_warm_state_path = directory.clone();
        // Keep the NetCDF path numerically equivalent to the previous legacy lookup.
        // In particular, the default one-day lag excludes same-run files.
        let latest_warm_state_time =
            warm_state_search_time(date, warm_state_hour, warm_state_lag_days);
        let (warm_state, warm_state_time) = if let Some(snapshot) = load_latest_fwi(
            directory,
            latest_warm_state_time,
            max_age_hours.unwrap_or(120),
            &config_defs.model_version,
            grid_ref,
            &cell_indexes,
        )? {
            snapshot
        } else {
            match on_missing {
                MissingWarmStatePolicy::Defaults => (
                    vec![FWIWarmState::default(); n_cells],
                    date - Duration::try_days(1).expect("Should be a valid duration"),
                ),
                MissingWarmStatePolicy::Error => {
                    return Err(format!(
                        "no valid FWI warm state found in {directory} and defaults are disabled"
                    )
                    .into())
                }
            }
        };
        if warm_state.len() != n_cells {
            return Err(format!(
                "FWI warm state has {} cells but static data has {n_cells}",
                warm_state.len()
            )
            .into());
        }

        if !["legacy", "cffdrs", "sliding_window"].contains(&config_defs.model_version.as_str()) {
            warn!("FWI: unknown model_version {:?}, the legacy model is used", config_defs.model_version);
        }
        info!(
            "FWI ({}): warm state at {} for run date {}",
            config_defs.model_version,
            warm_state_time.format("%Y-%m-%d %H:%M"),
            date.format("%Y-%m-%d %H:%M")
        );

        let mut props = FWIProperties::new(props_container);
        if config_defs.model_version == "cffdrs" {
            // daily step at local standard noon: one time-zone lookup per cell
            props.compute_standard_offsets(&date);
        }

        // if config_defs.model_version = 'legacy' them put output_time_resolution to 24
        let mut output_time_resolution = config_defs.output_time_resolution.unwrap_or(24);
        if config_defs.model_version == "legacy" && output_time_resolution != 24 {
            warn!("Using legacy model version, setting output_time_resolution to 24");
            output_time_resolution = 24;
        }
        let output_time_offset = config_defs.output_time_offset.unwrap_or(0);

        let config = FWIConfig {
            run_date: date,
            netcdf_warm_state_path,
            cell_indexes,
            grid,
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_time_resolution,
            output_time_offset,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    /// Outputs at run date + offset + k * resolution hours (k may be negative).
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours() - self.output_time_offset;
        hours.rem_euclid(self.output_time_resolution as i64) == 0
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    pub fn write_warm_state(
        &self,
        state: &FWIState,
        _warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let grid = self
            .grid
            .as_ref()
            .ok_or("NetCDF warm state is missing its grid")?;
        write_fwi_snapshot(
            &self.netcdf_warm_state_path,
            state,
            &self.model_version,
            grid,
            &self.cell_indexes,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod fwi_warm_state_tests {
    use super::*;

    #[test]
    fn tile_checkpoint_roundtrips_transient_and_history_state() {
        let path = std::env::temp_dir().join(format!(
            "risico-tile-checkpoint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("valid system clock")
                .as_nanos()
        ));
        let time = Utc
            .with_ymd_and_hms(2026, 7, 23, 12, 0, 0)
            .single()
            .expect("valid test date");
        let data = vec![FWIStateElement {
            dates: vec![time - Duration::hours(1)],
            ffmc: vec![82.0],
            dmc: vec![15.0],
            dc: vec![120.0],
            rain: vec![1.5],
            humidity: vec![35.0],
            temperature: vec![29.0],
            wind_speed: vec![4.0],
            rain24h: vec![2.0],
            noon_temperature: 28.0,
            noon_humidity: 33.0,
            noon_wind_speed: 4000.0,
        }];

        write_tile_checkpoint(&path, time, &data).expect("checkpoint should be written");
        let (restored_time, restored): (DateTime<Utc>, Vec<FWIStateElement>) =
            read_tile_checkpoint(&path).expect("checkpoint should be restored");

        assert_eq!(restored_time, time);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].dates, data[0].dates);
        assert_eq!(restored[0].ffmc, data[0].ffmc);
        assert_eq!(restored[0].humidity, data[0].humidity);
        assert_eq!(restored[0].rain24h, data[0].rain24h);
        fs::remove_file(path).expect("checkpoint should be removable");
    }

    fn decision_fixture() -> (TileMemoryModel, TilePlan) {
        // 100 cells in tiles of 10: resident 10 B, in flight 4 B, state 6 B
        // per cell. Domain without state: 1000 B; a tile without state: 40 B.
        let footprint = TileMemoryModel {
            resident_per_cell: 10,
            in_flight_per_cell: 4,
            state_per_cell: 6,
            warm_state_per_cell: 2,
        };
        (footprint, TilePlan::cells(100, 10).unwrap())
    }

    fn execution(
        max_memory: Option<&str>,
        tile_state: TileStateResidency,
    ) -> StreamingExecutionConfig {
        StreamingExecutionConfig {
            max_memory: max_memory.map(str::to_string),
            tile_state,
            ..StreamingExecutionConfig::default()
        }
    }

    #[test]
    fn the_state_is_charged_where_it_lives() {
        let (footprint, plan) = decision_fixture();
        let in_memory = footprint.estimate(&plan, true);
        assert_eq!(
            in_memory,
            PlanMemoryEstimate {
                resident: 1600,
                per_tile: 40
            }
        );
        let on_disk = footprint.estimate(&plan, false);
        assert_eq!(
            on_disk,
            PlanMemoryEstimate {
                resident: 1000,
                per_tile: 100
            }
        );
        assert_eq!(footprint.state_bytes(&plan), 600);
        assert_eq!(footprint.warm_state_bytes(&plan), 200);
    }

    #[test]
    fn auto_keeps_the_state_in_memory_when_the_budget_has_room() {
        let (footprint, plan) = decision_fixture();
        // 1640 B is exactly the domain with state and one tile in flight.
        let fits = ExecutionDecision::resolve(
            footprint,
            &execution(Some("1640"), TileStateResidency::Auto),
            &plan,
            8,
            0,
        )
        .unwrap();
        assert!(fits.resident_state);
        assert_eq!(fits.tile_concurrency, Some(1));
        assert!(!fits.over_budget());

        // One byte less and the state goes to disk, where the same budget
        // affords six tiles of 100 B over the 1000 B domain.
        let spills = ExecutionDecision::resolve(
            footprint,
            &execution(Some("1639"), TileStateResidency::Auto),
            &plan,
            8,
            0,
        )
        .unwrap();
        assert!(!spills.resident_state);
        assert_eq!(spills.tile_concurrency, Some(6));
        assert_eq!(spills.expected_peak(), 1600);
    }

    #[test]
    fn auto_without_a_budget_leaves_the_state_in_memory_and_rayon_in_charge() {
        let (footprint, plan) = decision_fixture();
        let decision = ExecutionDecision::resolve(
            footprint,
            &execution(None, TileStateResidency::Auto),
            &plan,
            8,
            0,
        )
        .unwrap();
        assert!(decision.resident_state);
        assert_eq!(decision.tile_concurrency, None);
        assert_eq!(decision.ceiling, 8);
        assert_eq!(decision.effective_concurrency(), 8);
        // The finish overlaps the step, so its 200 B warm state counts too.
        assert_eq!(decision.expected_peak(), 1600 + 8 * 40 + 200);
    }

    #[test]
    fn an_explicit_residency_is_taken_as_given() {
        let (footprint, plan) = decision_fixture();
        let forced_memory = ExecutionDecision::resolve(
            footprint,
            &execution(Some("500"), TileStateResidency::Memory),
            &plan,
            8,
            0,
        )
        .unwrap();
        assert!(forced_memory.resident_state);
        assert!(forced_memory.over_budget());
        assert_eq!(forced_memory.tile_concurrency, Some(1));

        let forced_disk = ExecutionDecision::resolve(
            footprint,
            &execution(Some("100000"), TileStateResidency::Disk),
            &plan,
            4,
            0,
        )
        .unwrap();
        assert!(!forced_disk.resident_state);
        assert_eq!(forced_disk.tile_concurrency, Some(4));
    }

    #[test]
    fn an_explicit_concurrency_is_what_auto_judges_the_budget_at() {
        let (footprint, plan) = decision_fixture();
        let mut config = execution(Some("1700"), TileStateResidency::Auto);
        config.tile_concurrency = Some(4);
        // 1600 + 4 * 40 = 1760 > 1700: four tiles do not fit with the state
        // in memory, so it goes to disk and the four tiles are kept.
        let decision = ExecutionDecision::resolve(footprint, &config, &plan, 8, 0).unwrap();
        assert!(!decision.resident_state);
        assert_eq!(decision.tile_concurrency, Some(4));
    }

    #[test]
    fn the_overlap_is_taken_only_when_a_finished_timestep_fits_beside_the_step() {
        let (footprint, plan) = decision_fixture();
        // Without a budget the overlap is free to take.
        let unbounded = ExecutionDecision::resolve(
            footprint,
            &execution(None, TileStateResidency::Auto),
            &plan,
            8,
            0,
        )
        .unwrap();
        assert!(unbounded.overlap_postprocess);
        assert_eq!(unbounded.finish_bytes, 200);
        assert_eq!(unbounded.expected_peak(), 1600 + 8 * 40 + 200);

        // State on disk at 1000 + 6 * 100 = 1600; finishing costs 300 + 200.
        let fits = ExecutionDecision::resolve(
            footprint,
            &execution(Some("2100"), TileStateResidency::Disk),
            &plan,
            6,
            300,
        )
        .unwrap();
        assert_eq!(fits.tile_concurrency, Some(6));
        assert!(fits.overlap_postprocess);
        assert_eq!(fits.expected_peak(), 2100);

        let tight = ExecutionDecision::resolve(
            footprint,
            &execution(Some("2099"), TileStateResidency::Disk),
            &plan,
            6,
            300,
        )
        .unwrap();
        assert!(!tight.overlap_postprocess);
        assert_eq!(tight.expected_peak(), 1600);

        let mut forced = execution(Some("2099"), TileStateResidency::Disk);
        forced.overlap_postprocess = Some(true);
        let forced = ExecutionDecision::resolve(footprint, &forced, &plan, 6, 300).unwrap();
        assert!(forced.overlap_postprocess);
    }

    #[test]
    fn postprocess_cost_counts_the_largest_grids_in_flight_and_one_tile_per_slot() {
        // Four cores over three grids: all three grids and four tile buffers.
        assert_eq!(
            postprocess_peak_bytes([100, 50, 10], 4, 1000),
            160 * 12 + 4 * 1000 * 4
        );
        // Two cores over three grids: only the two largest are in flight.
        assert_eq!(
            postprocess_peak_bytes([100, 50, 10], 2, 1000),
            150 * 12 + 2 * 4000
        );
        assert_eq!(postprocess_peak_bytes([], 8, 1000), 0);
    }

    #[test]
    fn a_budget_pays_for_what_is_left_after_the_domain() {
        const GB: u64 = 1_000_000_000;
        let estimate = PlanMemoryEstimate {
            resident: 4 * GB,
            per_tile: GB / 2,
        };
        // 8 GB budget, 4 GB domain, 500 MB per tile: eight tiles fit.
        assert_eq!(estimate.tiles_within(8 * GB, 14), 8);
        // Raising the budget buys more tiles until the ceiling stops it.
        assert_eq!(estimate.tiles_within(12 * GB, 14), 14);
        // Lowering it gives them back.
        assert_eq!(estimate.tiles_within(5 * GB, 14), 2);
        // Below the domain's own cost it still runs, one tile at a time.
        assert_eq!(estimate.tiles_within(GB, 14), 1);
        assert_eq!(estimate.tiles_within(0, 14), 1);
        assert_eq!(estimate.at_concurrency(2), 5 * GB);
    }

    #[test]
    fn the_ceiling_is_never_exceeded_and_never_reaches_zero() {
        const GB: u64 = 1_000_000_000;
        let estimate = PlanMemoryEstimate {
            resident: 0,
            per_tile: GB,
        };
        assert_eq!(estimate.tiles_within(1_000 * GB, 4), 4);
        assert_eq!(estimate.tiles_within(1_000 * GB, 0), 1);
        // A model that costs nothing per tile still respects the ceiling.
        let free = PlanMemoryEstimate {
            resident: 0,
            per_tile: 0,
        };
        assert_eq!(free.tiles_within(GB, 6), 6);
    }

    #[test]
    fn netcdf_cutoff_uses_the_same_lag_as_legacy_lookup() {
        let run_date = Utc
            .with_ymd_and_hms(2024, 1, 2, 0, 0, 0)
            .single()
            .expect("valid test date");

        assert_eq!(
            warm_state_search_time(run_date, 0, 1),
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
                .single()
                .expect("valid warm-state date")
        );
    }
}

impl Mark5Config {
    // New Mark5 configuration
    pub fn new(
        config_defs: &Mark5ConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<Mark5Config, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = Mark5Config::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            return Err(format!("All properties must have the same length").into());
        }
        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let (warm_state, warm_state_time) = Mark5Config::read_warm_state(
            &config_defs.warm_state_path,
            date,
            &warm_state_hour,
            &warm_state_lag_days,
        )
        .unwrap_or((
            vec![Mark5WarmState::default(); n_cells],
            date - Duration::try_days(1).expect("Should be a valid duration"),
        ));
        let props = Mark5Properties::new(props_container);
        let config = Mark5Config {
            run_date: date,
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read the cells from a file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<Mark5CellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("Can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut mean_rains: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("Can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 3 {
                let error_message = format!(
                    "
                    Invalid line in file {file_path} at #{index}:
                    Expected 3 elements, found {} in line: 
                {line}",
                    line_parts.len()
                );
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let mean_rain = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `mean_rain` value in file {file_path} at line #{index}: '{line}'")
            })?;
            lons.push(lon);
            lats.push(lat);
            mean_rains.push(mean_rain);
        }
        let props = Mark5CellPropertiesContainer {
            lats,
            lons,
            mean_rains,
        };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
        lag_days: &i64,
    ) -> Option<(Vec<Mark5WarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour, *lag_days);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<Mark5WarmState> = Vec::new();
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let dates = components[0]
                .split(",")
                .map(|date| {
                    NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
                        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
                        .unwrap_or_else(|_| panic!("Could not parse date from {}", date))
                })
                .collect();
            let daily_rain = components[1]
                .split(",")
                .map(|rain| {
                    rain.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse FFMC value from {}", rain))
                })
                .collect();
            let smd = components[2]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(Mark5WarmState {
                dates,
                daily_rain,
                smd,
            });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &Mark5State,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let dates = state.dates.clone();
            let daily_rain = state.daily_rain.clone();
            let smd = state.smd;
            let line = format!(
                "{}\t{}\t{}",
                dates
                    .iter()
                    .map(|value| format!("{}", value.format("%Y%m%d%H%M")))
                    .collect::<Vec<String>>()
                    .join(","),
                daily_rain
                    .iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                smd
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl KbdiConfig {
    // Keetch-Byram Drought Index configuration
    pub fn new(
        config_defs: &KbdiConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<KbdiConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = KbdiConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            return Err(format!("All properties must have the same length").into());
        }
        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let (warm_state, warm_state_time) = KbdiConfig::read_warm_state(
            &config_defs.warm_state_path,
            date,
            &warm_state_hour,
            &warm_state_lag_days,
        )
        .unwrap_or((
            vec![KBDIWarmState::default(); n_cells],
            date - Duration::try_days(1).expect("Should be a valid duration"),
        ));
        let props = KBDIProperties::new(props_container);
        let config = KbdiConfig {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Reads the properties from a file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<KBDICellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut mean_rains: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 3 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let mean_rain = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `mean_rain` value in file {file_path} at line #{index}: '{line}'")
            })?;
            lons.push(lon);
            lats.push(lat);
            mean_rains.push(mean_rain);
        }

        let props = KBDICellPropertiesContainer {
            lats,
            lons,
            mean_rains,
        };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
        lag_days: &i64,
    ) -> Option<(Vec<KBDIWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour, *lag_days);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<KBDIWarmState> = Vec::new();
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let dates = components[0]
                .split(",")
                .map(|date| {
                    NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
                        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
                        .unwrap_or_else(|_| panic!("Could not parse date from {}", date))
                })
                .collect();
            let daily_rain = components[1]
                .split(",")
                .map(|rain| {
                    rain.parse::<f32>()
                        .unwrap_or_else(|_| panic!("Could not parse FFMC value from {}", rain))
                })
                .collect();
            let kbdi = components[2]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(KBDIWarmState {
                dates,
                daily_rain,
                kbdi,
            });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &KBDIState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let dates = state.dates.clone();
            let daily_rain = state.daily_rain.clone();
            let kbdi = state.kbdi;
            let line = format!(
                "{}\t{}\t{}",
                dates
                    .iter()
                    .map(|value| format!("{}", value.format("%Y%m%d%H%M")))
                    .collect::<Vec<String>>()
                    .join(","),
                daily_rain
                    .iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                kbdi
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl AngstromConfig {
    // New Angstrom index configuration
    pub fn new(
        config_defs: &AngstromConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<AngstromConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = AngstromConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = AngstromProperties::new(props_container);
        let config = AngstromConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<AngstromCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;
            lons.push(lon);
            lats.push(lat);
        }
        let props = AngstromCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        // the Angstrom index is computed every 24 hours (once a day)
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}

impl FosbergConfig {
    // New Fosberg index configuration
    pub fn new(
        config_defs: &FosbergConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<FosbergConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = FosbergConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = FosbergProperties::new(props_container);
        let config = FosbergConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<FosbergCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }
        let props = FosbergCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}

impl NesterovConfig {
    // New Nesterov index configuration
    pub fn new(
        config_defs: &NesterovConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<NesterovConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = NesterovConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let (warm_state, warm_state_time) = NesterovConfig::read_warm_state(
            &config_defs.warm_state_path,
            date,
            &warm_state_hour,
            &warm_state_lag_days,
        )
        .unwrap_or((
            vec![NesterovWarmState::default(); n_cells],
            date - Duration::try_days(1).expect("Should be a valid duration"),
        ));
        let props = NesterovProperties::new(props_container);
        let config = NesterovConfig {
            run_date: date,
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<NesterovCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }

        let props = NesterovCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
        lag_days: &i64,
    ) -> Option<(Vec<NesterovWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour, *lag_days);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<NesterovWarmState> = Vec::new();

        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let nesterov = components[0]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(NesterovWarmState { nesterov });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &NesterovState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let nesterov = state.nesterov;
            let line = format!("{}", nesterov);
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

impl SharplesConfig {
    // New Sharples index configuration
    pub fn new(
        config_defs: &SharplesConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<SharplesConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = SharplesConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = SharplesProperties::new(props_container);
        let config = SharplesConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<SharplesCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }
        let props = SharplesCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}

impl OrieuxConfig {
    // New Orieux index configuration
    pub fn new(
        config_defs: &OrieuxConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<OrieuxConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = OrieuxConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let (warm_state, warm_state_time) = OrieuxConfig::read_warm_state(
            &config_defs.warm_state_path,
            date,
            &warm_state_hour,
            &warm_state_lag_days,
        )
        .unwrap_or((
            vec![OrieuxWarmState::default(); n_cells],
            date - Duration::try_days(1).expect("Should be a valid duration"),
        ));
        let props = OrieuxProperties::new(props_container);
        let config = OrieuxConfig {
            run_date: date,
            warm_state_path: config_defs.warm_state_path.clone(),
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<OrieuxCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut heat_indices: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 3 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let hindex = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `hindex` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
            heat_indices.push(hindex);
        }
        let props = OrieuxCellPropertiesContainer {
            lats,
            lons,
            heat_indices,
        };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
        lag_days: &i64,
    ) -> Option<(Vec<OrieuxWarmState>, DateTime<Utc>)> {
        let (file, current_date) = find_warm_state(base_warm_file, run_date, *hour, *lag_days);
        let file = match file {
            Some(file) => file,
            None => {
                warn!(
                    "WARNING: Could not find a valid warm state file for run date {}",
                    run_date.format("%Y-%m-%d")
                );
                return None;
            }
        };
        info!(
            "Loading warm state from {}",
            current_date.format("%Y-%m-%d %H:%M")
        );
        let mut warm_state: Vec<OrieuxWarmState> = Vec::new();
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Err(line) = line {
                warn!("Error reading warm state file: {}", line);
                return None;
            }
            let line = line.expect("Should unwrap line");
            let components: Vec<&str> = line.split_whitespace().collect();
            let orieux_wr = components[0]
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
            warm_state.push(OrieuxWarmState { orieux_wr });
        }
        Some((warm_state, current_date))
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &OrieuxState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
        for state in &state.data {
            let orieux_wr = state.orieux_wr;
            let line = format!("{}", orieux_wr);
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

// UNDER CONSTRUCTION
//impl PortugueseConfig {
//
//    // New Portuguese index configuration
//    pub fn new(
//        config_defs: &PortugueseConfigBuilder,
//        date: DateTime<Utc>,
//        palettes: &HashMap<String, String>,
//    ) -> Result<PortugueseConfig, RISICOError> {
//        let palettes = load_palettes(palettes);
//        let cells_file = &config_defs.cells_file_path;
//        let props_container = PortugueseConfig::properties_from_file(cells_file)
//            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
//        let n_cells = props_container.lons.len();
//        if n_cells != props_container.lats.len()
//        {
//            panic!("All properties must have the same length");
//        }
//        let warm_state_hour = if config_defs.warm_state_hour > 0 {
//            config_defs.warm_state_hour.clone()
//        } else {
//            24
//        };
//        let (warm_state, warm_state_time) = PortugueseConfig::read_warm_state(&config_defs.warm_state_path, date, &warm_state_hour)
//            .unwrap_or((
//                vec![PortugueseWarmState::default(); n_cells],
//                date - Duration::try_days(1).expect("Should be a valid duration"),
//            ));
//        let props = PortugueseProperties::new(props_container);
//        let config = PortugueseConfig {
//            run_date: date,
//            warm_state_path: config_defs.warm_state_path.clone(),
//            warm_state,
//            warm_state_time,
//            warm_state_hour: warm_state_hour,
//            properties: props,
//            palettes,
//            output_types_defs: config_defs.output_types.clone(),
//        };
//        Ok(config)
//    }
//
//    // Read properties from file
//    pub fn properties_from_file(file_path: &str) -> Result<PortugueseCellPropertiesContainer, RISICOError> {
//        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
//        let mut lons: Vec<f32> = Vec::new();
//        let mut lats: Vec<f32> = Vec::new();
//        let reader = BufReader::new(file);
//        for line in reader.lines() {
//            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
//            if line.starts_with("#") {
//                // skip header
//                continue;
//            }
//            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
//            if line_parts.len() < 2 {
//                let error_message = format!("Invalid line in file: {}", line);
//                return Err(error_message.into());
//            }
//            let lon = line_parts[0]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
//            let lat = line_parts[1]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
//            lons.push(lon);
//            lats.push(lat);
//        }
//        let props = PortugueseCellPropertiesContainer {
//            lats,
//            lons,
//        };
//        Ok(props)
//    }
//
//    pub fn get_properties(&self) -> &PortugueseProperties {
//        &self.properties
//    }
//
//    pub fn new_state(&self) -> PortugueseState {
//        PortugueseState::new(&self.warm_state, &self.warm_state_time)
//    }
//
//    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
//        OutputWriter::new(
//            self.output_types_defs.as_slice(),
//            &self.run_date,
//            &self.palettes,
//        ))
//    }
//
//    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> (bool, DateTime<Utc>) {
//        let time_diff = time.signed_duration_since(self.run_date);
//        let minutes = time_diff.num_minutes();
//        // Approximation to the closest hour
//        let approximate_hours = if minutes % 60 >= 30 {
//            (minutes / 60) + 1
//        } else {
//            minutes / 60
//        };
//        let warm_state_time = self.run_date + Duration::try_hours(approximate_hours).expect("Should be valid");
//        let should_write= (approximate_hours % self.warm_state_hour == 0) && (approximate_hours > 0);
//        (should_write, warm_state_time)
//    }
//
//    #[allow(non_snake_case)]
//    /// Reads the warm state from the file
//    /// The warm state is stored in a file with the following structure:
//    /// base_warm_file_YYYYmmDDHHMM
//    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
//    pub fn read_warm_state(
//        base_warm_file: &str,
//        run_date: DateTime<Utc>,
//        offset: &i64,
//    ) -> Option<(Vec<PortugueseWarmState>, DateTime<Utc>)> {
//        // for the last n days before date, try to read the warm state
//        // compose the filename as base_warm_file_YYYYmmDDHHMM
//        let mut file: Option<File> = None;
//        let mut current_date = run_date;
//        for days_before in 1..4 {
//            current_date = run_date - Duration::try_days(days_before).expect("Should be valid");
//            // add the offset to the current date
//            current_date = current_date + Duration::try_hours(*offset).expect("Should be valid");
//            let filename = format!("{}{}", base_warm_file, current_date.format("%Y%m%d%H%M"));
//            let file_handle = File::open(filename);
//            if file_handle.is_err() {
//                continue;
//            }
//            file = Some(file_handle.expect("Should unwrap"));
//            break;
//        }
//        let file = match file {
//            Some(file) => file,
//            None => {
//                warn!(
//                    "WARNING: Could not find a valid warm state file for run date {}",
//                    run_date.format("%Y-%m-%d")
//                );
//                return None;
//            }
//        };
//
//        info!(
//            "Loading warm state from {}",
//            current_date.format("%Y-%m-%d %H:%M")
//        );
//        let mut warm_state: Vec<PortugueseWarmState> = Vec::new();
//        let reader = io::BufReader::new(file);
//        for line in reader.lines() {
//            if let Err(line) = line {
//                warn!("Error reading warm state file: {}", line);
//                return None;
//            }
//            let line = line.expect("Should unwrap line");
//            let components: Vec<&str> = line.split_whitespace().collect();
//            let sum_ign = components[0]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
//            let cum_index = components[1]
//                .parse::<f32>()
//                .unwrap_or_else(|_| panic!("Could not parse snow_cover from {}", line));
//            warm_state.push(PortugueseWarmState {
//                sum_ign,
//                cum_index
//            });
//        }
//        Some((warm_state, current_date))
//    }
//
//    #[allow(non_snake_case)]
//    pub fn write_warm_state(&self, state: &PortugueseState, warm_state_time: DateTime<Utc>) -> Result<(), RISICOError> {
//        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
//        let warm_state_name = format!("{}{}", self.warm_state_path, date_string);
//        let mut warm_state_file = File::create(&warm_state_name)
//            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;
//        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);
//        for state in &state.data {
//            let sum_ign = state.sum_ign.clone();
//            let cum_index = state.cum_index.clone();
//            let line = format!(
//                "{}\t{}",
//                sum_ign,
//                cum_index
//            );
//            writeln!(warm_state_writer, "{}", line)
//                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
//        }
//        Ok(())
//    }
//}

impl HdwConfig {
    // New Hot-dry-wind index configuration
    pub fn new(
        config_defs: &HdwConfigBuilder,
        date: DateTime<Utc>,
        palettes: &HashMap<String, String>,
    ) -> Result<HdwConfig, RISICOError> {
        let palettes = load_palettes(palettes);
        let cells_file = &config_defs.cells_file_path;
        let props_container = HdwConfig::properties_from_file(cells_file)
            .map_err(|error| format!("error reading {}, {error}", cells_file))?;
        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }
        let props = HdwProperties::new(props_container);
        let config = HdwConfig {
            run_date: date,
            properties: props,
            palettes,
            output_time_resolution: config_defs.output_time_resolution,
            output_types_defs: config_defs.output_types.clone(),
        };
        Ok(config)
    }

    // Read properties from file
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<HdwCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;
        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }
            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();
            if line_parts.len() < 2 {
                let error_message = format!("Invalid line in file: {}", line);
                return Err(error_message.into());
            }
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }
        let props = HdwCellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        )
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}
