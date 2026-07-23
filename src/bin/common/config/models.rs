use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
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
        AngstromCellPropertiesContainer, AngstromProperties, AngstromState,
    },
    modules::fosberg::models::{FosbergCellPropertiesContainer, FosbergProperties, FosbergState},
    modules::fwi::{
        config::FWIModelConfig,
        models::{FWICellPropertiesContainer, FWIProperties, FWIState, FWIWarmState},
    },
    //    modules::portuguese::models::{PortugueseCellPropertiesContainer, PortugueseProperties, PortugueseState, PortugueseWarmState},
    modules::hdw::models::{HdwCellPropertiesContainer, HdwProperties, HdwState},
    modules::kbdi::{
        config::KBDIModelConfig,
        models::{KBDICellPropertiesContainer, KBDIProperties, KBDIState, KBDIWarmState},
    },
    modules::mark5::{
        config::Mark5ModelConfig,
        models::{Mark5CellPropertiesContainer, Mark5Properties, Mark5State, Mark5WarmState},
    },
    modules::nesterov::models::{
        NesterovCellPropertiesContainer, NesterovProperties, NesterovState, NesterovWarmState,
    },
    modules::orieux::models::{
        OrieuxCellPropertiesContainer, OrieuxProperties, OrieuxState, OrieuxWarmState,
    },
    modules::risico::{
        config::RISICOModelConfig,
        models::{
            RISICOCellPropertiesContainer, RISICOProperties, RISICOState, RISICOVegetation,
            RISICOWarmState,
        },
    },
    modules::sharples::models::{
        SharplesCellPropertiesContainer, SharplesProperties, SharplesState,
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
    WarmStateConfig,
};

use crate::common::helpers::RISICOError;
use crate::common::io::models::{
    output::{MappedOutputSource, NativeOutputSource, OutputType},
    palette::Palette,
};
use crate::common::io::static_data::geotiff::{RasterDomain, RasterGrid};
use crate::common::io::streaming::{MappedNativeOutputs, SpatialTile, TilePlan};
use crate::common::io::warm_state::legacy::{read_fwi, read_risico};
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
    warm_state_path: Option<String>,
    netcdf_warm_state_path: Option<String>,
    cell_indexes: Vec<u32>,
    grid: Option<RasterGrid>,
    warm_state: Vec<RISICOWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: RISICOProperties,
    palettes: PaletteMap,
    // use_temperature_effect: bool,  // DEPRECATED
    // use_ndvi: bool,  // DEPRECATED
    output_time_resolution: u32,
    output_types_defs: Vec<OutputTypeConfig>,
    model_version: String,
}

pub struct FWIConfig {
    run_date: DateTime<Utc>,
    warm_state_path: Option<String>,
    netcdf_warm_state_path: Option<String>,
    cell_indexes: Vec<u32>,
    grid: Option<RasterGrid>,
    warm_state: Vec<FWIWarmState>,
    warm_state_time: DateTime<Utc>,
    warm_state_hour: i64,
    properties: FWIProperties,
    palettes: PaletteMap,
    output_time_resolution: u32,
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
}

fn select_cells<T: Clone>(data: &ndarray::Array1<T>, tile: &SpatialTile) -> ndarray::Array1<T> {
    tile.model_positions
        .iter()
        .map(|&position| data[position].clone())
        .collect()
}

fn select_warm_state<T: Clone>(data: &[T], tile: &SpatialTile) -> Vec<T> {
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
}

pub struct TileStep {
    pub output: Option<Output>,
    pub write_warm_state: bool,
}

/// Model-specific operations used by the single production tile runner.
///
/// The runner owns spatial batching and persistence. Implementations retain
/// only the small differences in each model's store/update/output schedule.
pub trait TileModelRuntime: TileModelFactory {
    type WarmState: Clone;

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
    pub fn new(
        outputs_defs: &[OutputTypeConfig],
        date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Self {
        Self {
            outputs: outputs_defs
                .iter()
                .filter_map(|t| OutputType::new(t, date, palettes).ok())
                .collect(),
        }
    }

    pub fn write_mapped_output(
        &mut self,
        lats: &[f32],
        lons: &[f32],
        time: DateTime<Utc>,
        output: &MappedNativeOutputs,
    ) -> Result<(), RISICOError> {
        self.write_source(lats, lons, &MappedOutputSource::new(time, output))
    }

    fn write_source(
        &mut self,
        lats: &[f32],
        lons: &[f32],
        output: &dyn NativeOutputSource,
    ) -> Result<(), RISICOError> {
        self.outputs.par_iter_mut().for_each(|output_type| {
            match output_type.write_variables(lats, lons, output) {
                Ok(_) => (),
                Err(e) => warn!("Error writing output: {}", e),
            }
        });
        Ok(())
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

        let (props_container, ppf, vegetation_file, cell_indexes, grid) = match (
            &config_defs.static_data,
            &config_defs.cells_file_path,
        ) {
            (Some(_), Some(_)) => {
                return Err(
                    "configure either static_data or cells_file_path for RISICO, not both".into(),
                )
            }
            (None, None) => return Err("RISICO requires static_data or cells_file_path".into()),
            (None, Some(cells_file)) => {
                let props_container = RISICOConfig::properties_from_file(cells_file)
                    .map_err(|error| format!("error reading {cells_file}, {error}"))?;
                let ppf = match &config_defs.ppf_file {
                    Some(ppf_file) => RISICOConfig::read_ppf(ppf_file)
                        .map_err(|error| format!("error reading {ppf_file}, {error}"))?,
                    None => vec![(1.0, 1.0); props_container.lons.len()],
                };
                let vegetation_file = config_defs
                    .vegetation_file
                    .clone()
                    .ok_or("legacy RISICO static data requires vegetation_file")?;
                (props_container, ppf, vegetation_file, Vec::new(), None)
            }
            (Some(static_data), None) => match static_data {
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
                    let slopes = domain
                        .read_required_layer(slope, "slope")?
                        .into_iter()
                        .map(|value| value * PI / 180.0)
                        .collect();
                    let aspects = domain
                        .read_required_layer(aspect, "aspect")?
                        .into_iter()
                        .map(|value| value * PI / 180.0)
                        .collect();
                    let vegetations = domain
                        .read_required_layer(vegetation_id, "vegetation_id")?
                        .into_iter()
                        .map(|value| {
                            let rounded = value.round();
                            if !value.is_finite() || (value - rounded).abs() > 1.0e-4 {
                                Err(format!(
                                    "vegetation_id must contain finite integer values, found {value}"
                                )
                                .into())
                            } else {
                                Ok((rounded as i64).to_string())
                            }
                        })
                        .collect::<Result<Vec<_>, RISICOError>>()?;

                    let ppf = match (ppf_summer, ppf_winter) {
                        (None, None) => vec![(1.0, 1.0); domain.cell_indexes.len()],
                        (Some(summer), Some(winter)) => domain
                            .read_required_layer(summer, "ppf_summer")?
                            .into_iter()
                            .zip(domain.read_required_layer(winter, "ppf_winter")?)
                            .collect(),
                        _ => {
                            return Err(
                                "ppf_summer and ppf_winter must either both be configured or both omitted"
                                    .into(),
                            )
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
                        vegetation_catalog,
                        domain.cell_indexes,
                        Some(domain.grid),
                    )
                }
            },
        };

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len()
            || n_cells != props_container.slopes.len()
            || n_cells != props_container.aspects.len()
            || n_cells != props_container.vegetations.len()
        {
            panic!("All properties must have the same length");
        }

        let vegetations_dict = RISICOConfig::read_vegetation(&vegetation_file)
            .map_err(|error| format!("error reading {vegetation_file}, {error}"))?;

        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let mut netcdf_warm_state_path = None;
        let (warm_state, warm_state_time) = match &config_defs.warm_state {
            None => {
                let path = config_defs
                    .warm_state_path
                    .as_deref()
                    .ok_or("legacy RISICO warm state requires warm_state_path")?;
                RISICOConfig::read_warm_state(path, date, &warm_state_hour, &warm_state_lag_days)
                    .unwrap_or((
                        vec![RISICOWarmState::default(); n_cells],
                        date - Duration::try_days(1).expect("Should be a valid duration"),
                    ))
            }
            Some(WarmStateConfig::NetCdf {
                directory,
                legacy_fallback,
                max_age_hours,
                on_missing,
            }) => {
                let grid = grid.as_ref().ok_or(
                    "NetCDF warm state requires GeoTIFF static_data so grid geometry is known",
                )?;
                netcdf_warm_state_path = Some(directory.clone());
                // Match legacy lookup semantics: a run must not seed itself from a
                // snapshot written by an earlier execution of that same run.
                let latest_warm_state_time =
                    warm_state_search_time(date, warm_state_hour, warm_state_lag_days);
                if let Some(snapshot) = load_latest_risico(
                    directory,
                    latest_warm_state_time,
                    max_age_hours.unwrap_or(120),
                    &config_defs.model_version,
                    grid,
                    &cell_indexes,
                )? {
                    snapshot
                } else {
                    let fallback = legacy_fallback
                        .as_deref()
                        .or(config_defs.warm_state_path.as_deref())
                        .and_then(|path| {
                            RISICOConfig::read_warm_state(
                                path,
                                date,
                                &warm_state_hour,
                                &warm_state_lag_days,
                            )
                        });
                    match fallback {
                        Some(snapshot) => snapshot,
                        None if *on_missing == MissingWarmStatePolicy::Defaults => (
                            vec![RISICOWarmState::default(); n_cells],
                            date - Duration::try_days(1).expect("Should be a valid duration"),
                        ),
                        None => {
                            return Err(format!(
                                "no valid RISICO warm state found in {directory} and defaults are disabled"
                            )
                            .into())
                        }
                    }
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

        let ppf_summer = ppf.iter().map(|(s, _)| *s).collect();
        let ppf_winter = ppf.iter().map(|(_, w)| *w).collect();

        let props =
            RISICOProperties::new(props_container, vegetations_dict, ppf_summer, ppf_winter);

        let config = RISICOConfig {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            netcdf_warm_state_path,
            cell_indexes,
            grid,
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            // use_temperature_effect: config_defs.use_temperature_effect,  // DEPRECATED
            // use_ndvi: config_defs.use_ndvi,  // DEPRECATED
            output_time_resolution: config_defs.output_time_resolution,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    /// Read the cells from a file.
    /// :param file_path: The path to the file.
    /// :return: A list of cells.
    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<RISICOCellPropertiesContainer, RISICOError> {
        let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;

        let mut lons: Vec<f32> = Vec::new();
        let mut lats: Vec<f32> = Vec::new();
        let mut slopes: Vec<f32> = Vec::new();
        let mut aspects: Vec<f32> = Vec::new();
        let mut vegetations: Vec<String> = Vec::new();

        let reader = BufReader::new(file);

        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|err| format!("can't read from file: {err}."))?;
            if line.starts_with("#") {
                // skip header
                continue;
            }

            let line_parts: Vec<&str> = line.trim().split(char::is_whitespace).collect();

            if line_parts.len() < 5 {
                let error_message = format!(
                    "Invalid line in file {file_path}: 
                expected 5 elements, found {} in line #{index}:
                {line}",
                    line_parts.len()
                );
                return Err(error_message.into());
            }

            //  [TODO] refactor this for using error handling
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let slope = line_parts[2].parse::<f32>().map_err(|_| {
                format!("Invalid `slope` value in file {file_path} at line #{index}: '{line}'")
            })?;
            let aspect = line_parts[3].parse::<f32>().map_err(|_| {
                format!("Invalid `aspect` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let vegetation = line_parts[4].to_string();

            let slope = slope * PI / 180.0;
            let aspect = aspect * PI / 180.0;

            lons.push(lon);
            lats.push(lat);
            slopes.push(slope);
            aspects.push(aspect);
            vegetations.push(vegetation);
        }

        let props = RISICOCellPropertiesContainer {
            lats,
            lons,
            slopes,
            aspects,
            vegetations,
        };
        Ok(props)
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

    /// Reads the PPF file and returns a vector of with (ppf_summer, ppf_winter) tuples
    /// The PPF file is a text file with the following structure:
    /// ppf_summer ppf_winter
    /// where ppf_summer and ppf_winter are floats
    pub fn read_ppf(ppf_file: &str) -> Result<Vec<(f32, f32)>, RISICOError> {
        let file = File::open(ppf_file)
            .map_err(|error| format!("Could not open file {}: {}", ppf_file, error))?;

        let reader = io::BufReader::new(file);
        let mut ppf: Vec<(f32, f32)> = Vec::new();
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    return Err(format!("Error reading PPF file {}: {}", ppf_file, error).into());
                }
            };
            let components: Vec<&str> = line.split_whitespace().collect();
            let ppf_summer = components[0].parse::<f32>().map_err(|err| {
                format!("Could not parse value from PPF file {}: {}", ppf_file, err)
            })?;

            let ppf_winter = components[1].parse::<f32>().map_err(|err| {
                format!("Could not parse value from PPF file {}: {}", ppf_file, err)
            })?;
            ppf.push((ppf_summer, ppf_winter));
        }
        Ok(ppf)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    /// The warm state is stored in a text file with the following structure:
    /// dffm
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
        lag_days: &i64,
    ) -> Option<(Vec<RISICOWarmState>, DateTime<Utc>)> {
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

        let source = format!("RISICO warm state at {current_date}");
        match read_risico(io::BufReader::new(file), &source) {
            Ok(warm_state) => Some((warm_state, current_date)),
            Err(error) => {
                warn!("Could not read legacy warm state: {error}");
                None
            }
        }
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &RISICOState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        if let Some(directory) = &self.netcdf_warm_state_path {
            let grid = self
                .grid
                .as_ref()
                .ok_or("NetCDF warm state is missing its grid")?;
            write_risico_snapshot(
                directory,
                state,
                &self.model_version,
                grid,
                &self.cell_indexes,
            )?;
            return Ok(());
        }

        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_path = self
            .warm_state_path
            .as_deref()
            .ok_or("legacy warm-state path is not configured")?;
        let warm_state_name = format!("{}{}", warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;

        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);

        for state in &state.data {
            let dffm = state.dffm;

            let MSI = state.MSI; //cell.state.MSI;
            let MSI_TTL = state.MSI_TTL; //cell.state.MSI_TTL;
            let NDVI = state.NDVI; //cell.state.NDVI;
            let NDVI_TIME = state.NDVI_TIME; //cell.state.NDVI_TIME;
            let NDWI = state.NDWI; //cell.state.NDWI;
            let NDWI_TIME = state.NDWI_TIME; //cell.state.NDWI_TTL;
            let snow_cover = state.snow_cover; //cell.state.snow_cover;
            let snow_cover_time = state.snow_cover_time; //cell.state.snow_cover_time;

            let line = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                dffm, snow_cover, snow_cover_time, MSI, MSI_TTL, NDVI, NDVI_TIME, NDWI, NDWI_TIME
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
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

        let (props_container, cell_indexes, grid) =
            match (&config_defs.static_data, &config_defs.cells_file_path) {
                (Some(_), Some(_)) => {
                    return Err(
                        "configure either static_data or cells_file_path for FWI, not both".into(),
                    )
                }
                (None, None) => return Err("FWI requires static_data or cells_file_path".into()),
                (None, Some(cells_file)) => (
                    FWIConfig::properties_from_file(cells_file)
                        .map_err(|error| format!("error reading {cells_file}, {error}"))?,
                    Vec::new(),
                    None,
                ),
                (Some(StaticDataConfig::GeoTiff { domain_mask, .. }), None) => {
                    let domain = RasterDomain::open(domain_mask)?;
                    let (lats, lons) = domain.coordinates();
                    (
                        FWICellPropertiesContainer { lats, lons },
                        domain.cell_indexes,
                        Some(domain.grid),
                    )
                }
            };

        let n_cells = props_container.lons.len();
        if n_cells != props_container.lats.len() {
            panic!("All properties must have the same length");
        }

        let warm_state_hour = config_defs.warm_state_hour.unwrap_or(WARM_STATE_HOUR);
        let warm_state_lag_days = config_defs
            .warm_state_lag_days
            .unwrap_or(WARM_STATE_LAG_DAYS);

        let mut netcdf_warm_state_path = None;
        let (warm_state, warm_state_time) = match &config_defs.warm_state {
            None => {
                let path = config_defs
                    .warm_state_path
                    .as_deref()
                    .ok_or("legacy FWI warm state requires warm_state_path")?;
                FWIConfig::read_warm_state(path, date, &warm_state_hour, &warm_state_lag_days)
                    .unwrap_or((
                        vec![FWIWarmState::default(); n_cells],
                        date - Duration::try_days(1).expect("Should be a valid duration"),
                    ))
            }
            Some(WarmStateConfig::NetCdf {
                directory,
                legacy_fallback,
                max_age_hours,
                on_missing,
            }) => {
                let grid = grid.as_ref().ok_or(
                    "NetCDF warm state requires GeoTIFF static_data so grid geometry is known",
                )?;
                netcdf_warm_state_path = Some(directory.clone());
                // Keep the NetCDF path numerically equivalent to find_warm_state.
                // In particular, the default one-day lag excludes same-run files.
                let latest_warm_state_time =
                    warm_state_search_time(date, warm_state_hour, warm_state_lag_days);
                if let Some(snapshot) = load_latest_fwi(
                    directory,
                    latest_warm_state_time,
                    max_age_hours.unwrap_or(120),
                    &config_defs.model_version,
                    grid,
                    &cell_indexes,
                )? {
                    snapshot
                } else {
                    let fallback = legacy_fallback
                        .as_deref()
                        .or(config_defs.warm_state_path.as_deref())
                        .and_then(|path| {
                            FWIConfig::read_warm_state(
                                path,
                                date,
                                &warm_state_hour,
                                &warm_state_lag_days,
                            )
                        });
                    match fallback {
                        Some(snapshot) => snapshot,
                        None if *on_missing == MissingWarmStatePolicy::Defaults => (
                            vec![FWIWarmState::default(); n_cells],
                            date - Duration::try_days(1).expect("Should be a valid duration"),
                        ),
                        None => {
                            return Err(format!(
                                "no valid FWI warm state found in {directory} and defaults are disabled"
                            )
                            .into())
                        }
                    }
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

        let props = FWIProperties::new(props_container);

        // if config_defs.model_version = 'legacy' them put output_time_resolution to 24
        let mut output_time_resolution = config_defs.output_time_resolution.unwrap_or(24);
        if config_defs.model_version == "legacy" && output_time_resolution != 24 {
            warn!("Using legacy model version, setting output_time_resolution to 24");
            output_time_resolution = 24;
        }

        let config = FWIConfig {
            run_date: date,
            // model_name: config_defs.model_name.clone(),
            warm_state_path: config_defs.warm_state_path.clone(),
            netcdf_warm_state_path,
            cell_indexes,
            grid,
            warm_state,
            warm_state_time,
            warm_state_hour,
            properties: props,
            palettes,
            output_time_resolution,
            model_version: config_defs.model_version.clone(),
            output_types_defs: config_defs.output_types.clone(),
        };

        Ok(config)
    }

    pub fn properties_from_file(
        file_path: &str,
    ) -> Result<FWICellPropertiesContainer, RISICOError> {
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

            //  [TODO] refactor this for using error handling
            let lon = line_parts[0].parse::<f32>().map_err(|_| {
                format!("Invalid `lon` value in file {file_path} at line #{index}: '{line}'")
            })?;

            let lat = line_parts[1].parse::<f32>().map_err(|_| {
                format!("Invalid `lat` value in file {file_path} at line #{index}: '{line}'")
            })?;

            lons.push(lon);
            lats.push(lat);
        }

        let props = FWICellPropertiesContainer { lats, lons };
        Ok(props)
    }

    pub fn get_output_writer(&self) -> Result<OutputWriter, RISICOError> {
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }

    pub fn should_write_warm_state(&self, time: &DateTime<Utc>) -> bool {
        check_write_warm_state(time, self.warm_state_hour)
    }

    #[allow(non_snake_case)]
    /// Reads the warm state from the file
    /// The warm state is stored in a file with the following structure:
    /// base_warm_file_YYYYmmDDHHMM
    /// where <base_warm_file> is the base name of the file and `YYYYmmDDHHMM` is the date of the warm state
    /// The warm state is stored in a text file with the following structure:
    /// dffm
    pub fn read_warm_state(
        base_warm_file: &str,
        run_date: DateTime<Utc>,
        hour: &i64,
        lag_days: &i64,
    ) -> Option<(Vec<FWIWarmState>, DateTime<Utc>)> {
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
        let source = format!("FWI warm state at {current_date}");
        match read_fwi(io::BufReader::new(file), &source, current_date) {
            Ok(warm_state) => Some((warm_state, current_date)),
            Err(error) => {
                warn!("Could not read legacy FWI warm state: {error}");
                None
            }
        }
    }

    #[allow(non_snake_case)]
    pub fn write_warm_state(
        &self,
        state: &FWIState,
        warm_state_time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        if let Some(directory) = &self.netcdf_warm_state_path {
            let grid = self
                .grid
                .as_ref()
                .ok_or("NetCDF warm state is missing its grid")?;
            write_fwi_snapshot(
                directory,
                state,
                &self.model_version,
                grid,
                &self.cell_indexes,
            )?;
            return Ok(());
        }

        let date_string = warm_state_time.format("%Y%m%d%H%M").to_string();
        let warm_state_path = self
            .warm_state_path
            .as_deref()
            .ok_or("legacy warm-state path is not configured")?;
        let warm_state_name = format!("{}{}", warm_state_path, date_string);
        let mut warm_state_file = File::create(&warm_state_name)
            .map_err(|error| format!("error creating {}, {}", &warm_state_name, error))?;

        let mut warm_state_writer = BufWriter::new(&mut warm_state_file);

        for state in &state.data {
            let dates = state.dates.clone();
            let ffmc = state.ffmc.clone();
            let dmc = state.dmc.clone();
            let dc = state.dc.clone();
            let rain = state.rain.clone();

            let line = format!(
                "{}\t{}\t{}\t{}\t{}",
                dates
                    .iter()
                    .map(|value| format!("{}", value.format("%Y%m%d%H%M")))
                    .collect::<Vec<String>>()
                    .join(","),
                ffmc.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                dmc.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                dc.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(","),
                rain.iter()
                    .map(|value| format!("{}", value))
                    .collect::<Vec<String>>()
                    .join(",")
            );
            writeln!(warm_state_writer, "{}", line)
                .map_err(|error| format!("error writing to {}, {}", &warm_state_name, error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod fwi_warm_state_tests {
    use super::*;

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

    #[test]
    fn deployed_scalar_fwi_state_is_read_in_rain_ffmc_dmc_dc_order() {
        let directory = std::env::temp_dir().join(format!(
            "risico-fwi-legacy-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("valid system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("test directory should be created");
        let base = directory.join("state0FWI_");
        let snapshot = directory.join("state0FWI_202401010000");
        fs::write(&snapshot, "0\t73.2036\t6\t15\n1.25\t80\t7\t16\n")
            .expect("legacy state should be written");
        let run_date = Utc
            .with_ymd_and_hms(2024, 1, 2, 0, 0, 0)
            .single()
            .expect("valid test date");

        let (state, time) =
            FWIConfig::read_warm_state(base.to_str().expect("UTF-8 test path"), run_date, &0, &1)
                .expect("deployed state should load");
        assert_eq!(time, run_date - Duration::days(1));
        assert_eq!(state.len(), 2);
        assert_eq!(state[0].rain, vec![0.0]);
        assert_eq!(state[0].ffmc, vec![73.2036]);
        assert_eq!(state[0].dmc, vec![6.0]);
        assert_eq!(state[0].dc, vec![15.0]);
        fs::remove_dir_all(directory).expect("test directory should be removable");
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
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
//        Ok(OutputWriter::new(
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
        Ok(OutputWriter::new(
            self.output_types_defs.as_slice(),
            &self.run_date,
            &self.palettes,
        ))
    }

    // check for writing output condition
    pub fn should_write_output(&self, time: &DateTime<Utc>) -> bool {
        let time_diff = time.signed_duration_since(self.run_date);
        let hours = time_diff.num_hours();
        hours % self.output_time_resolution as i64 == 0
    }
}
