//! RISICO: hourly dead fine fuel moisture, rate of spread and fireline intensity.
//!
//! Build [`models::RISICOProperties`] and a [`models::RISICOState`], then for
//! each timestamp call [`models::RISICOState::update`] and, when an output is
//! wanted, [`models::RISICOState::output`]. [`config::RISICOModelConfig::new`]
//! selects the formulation: `legacy`, `v2023` or `v2025`.

pub mod config;
pub mod constants;
pub mod functions;
pub mod models;
