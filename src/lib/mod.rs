//! RISICO and the fire danger indices of CIMA Research Foundation, as a library.
//!
//! This crate holds the models themselves: every index is a pure computation on
//! arrays of cells, with no file I/O. The `risico-2023` binary adds the readers,
//! writers, warm-state snapshots and the tiled runner around them.
//!
//! # Concepts
//!
//! Every model in [`modules`] is made of the same pieces:
//!
//! - **Properties** (`*Properties`): what does not change during a run, one
//!   element per cell. Coordinates for every model; slope, aspect, vegetation
//!   and PPF for RISICO; the local standard-time offset for FWI.
//! - **Warm state** (`*WarmState`): the per-cell memory a run starts from and
//!   saves at the end, such as the fine fuel moisture of RISICO or the moisture
//!   codes of FWI. Models without memory have none.
//! - **State** (`*State`): the running state, built from the warm state and a
//!   start time, advanced one [`Input`](models::input::Input) at a time.
//! - **Input** ([`models::input`]): the weather of one timestamp on every cell.
//! - **Output** ([`models::output`]): the variables a model produces at one
//!   timestamp, read by [`OutputVariableName`](models::output::OutputVariableName).
//!
//! Inputs, properties and states are aligned arrays: element `i` of each is
//! the same cell.
//!
//! # Units
//!
//! The library uses its own units, which the binary converts to on reading:
//! temperature in °C, relative humidity in %, **wind speed in m/h**, **wind
//! direction in radians** (meteorological, the direction the wind comes from),
//! rain in mm, snow cover in cm, **slope and aspect in radians**. Missing values
//! are [`constants::NODATAVAL`].
//!
//! # Example
//!
//! RISICO on one cell, one hour of weather:
//!
//! ```
//! use std::{collections::HashMap, sync::Arc};
//! use chrono::{Duration, TimeZone, Utc};
//! use risico::models::{input::{Input, InputElement}, output::OutputVariableName};
//! use risico::modules::risico::{
//!     config::RISICOModelConfig,
//!     models::{RISICOCellPropertiesContainer, RISICOProperties, RISICOState,
//!              RISICOVegetation, RISICOWarmState},
//! };
//!
//! let vegetation = Arc::new(RISICOVegetation::default());
//! let properties = RISICOProperties::new(
//!     RISICOCellPropertiesContainer {
//!         lons: vec![8.86],
//!         lats: vec![44.45],
//!         slopes: vec![0.1],   // radians
//!         aspects: vec![2.8],  // radians
//!         vegetations: vec![vegetation.clone()],
//!     },
//!     HashMap::from([(vegetation.id.clone(), vegetation)]),
//!     vec![1.0], // summer PPF
//!     vec![1.0], // winter PPF
//! );
//!
//! let start = Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap();
//! let mut state = RISICOState::new(
//!     &[RISICOWarmState::default()], &start, RISICOModelConfig::new("v2025"));
//!
//! let input = Input {
//!     time: start + Duration::hours(1),
//!     data: vec![InputElement {
//!         temperature: 30.0,
//!         humidity: 25.0,
//!         wind_speed: 5.0 * 3600.0, // m/h
//!         wind_dir: 0.0,
//!         rain: 0.0,
//!         snow_cover: 0.0,
//!         ..InputElement::default()
//!     }].into(),
//! };
//! state.update(&properties, &input);
//! let output = state.output(&properties, &input);
//! let dffm = output.get(&OutputVariableName::dffm).unwrap()[0];
//! assert!(dffm < 40.0); // the fuel dried out from the default 40 %
//! ```
//!
//! More complete programs are in the `examples/` directory of the repository
//! (`cargo run --example risico_single_cell`, `cargo run --example fwi_single_cell`).

#[cfg(feature = "config")]
pub mod config;
pub mod constants;
pub mod models;
pub mod modules;
pub mod version;
