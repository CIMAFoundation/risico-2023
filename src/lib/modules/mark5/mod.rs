//! McArthur Forest Fire Danger Index, Mark 5, with its drought factor.
//!
//! Hourly inputs are accumulated with `store`; `output` gives the daily
//! drought factor and FFDI. Each cell needs its mean annual rainfall.

pub mod constants;
pub mod functions;
pub mod models;
pub mod config;