//! Nesterov ignition index: a cumulative dryness index reset by rain.
//!
//! `store` hourly inputs, `update` once a day, read `output`.

pub mod constants;
pub mod functions;
pub mod models;
