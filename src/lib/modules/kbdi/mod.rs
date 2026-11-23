//! Keetch–Byram Drought Index.
//!
//! Hourly inputs are accumulated with `store`; `update` advances the daily
//! index (the runner calls it once a day, at the warm-state hour) and `output`
//! reads it. Each cell needs its mean annual rainfall.

pub mod constants;
pub mod functions;
pub mod models;
pub mod config;