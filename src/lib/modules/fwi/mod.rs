//! The Canadian Fire Weather Index System (FFMC, DMC, DC, ISI, BUI, FWI).
//!
//! Call [`models::FWIProperties::compute_standard_offsets`] once before the run:
//! the daily step happens at local standard noon. Then [`models::FWIState::update`]
//! every hour and [`models::FWIState::output`] when an output is wanted.
//! [`config::FWIModelConfig::new`] selects `legacy`, `cffdrs` or `sliding_window`.

pub mod cffdrs;
pub mod constants;
pub mod functions;
pub mod models;
pub mod config;