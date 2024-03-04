#![allow(dead_code)]
// import state from lib
mod library;
use std::env::{args, set_var, var};

use chrono::prelude::*;
use log::{info, trace, warn};
use pretty_env_logger;

use crate::library::{
    config::models::{Config, InputDataHandler},
    helpers::get_input,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

use git_version::git_version;
const GIT_VERSION: &str = git_version!();

// parse command line arguments: first argument is model date in the form YYYYMMDDHHMM, second is configuration path, third is input path
fn main() {
    if var("RUST_LOG").is_err() {
        set_var("RUST_LOG", "info")
    }
    pretty_env_logger::init();

    info!("RISICO.rs {VERSION}.{GIT_VERSION}");
    let args: Vec<String> = args().collect();
    if args.len() != 4 {
        info!("Usage: {} YYYYMMDDHHMM config_path input_path", args[0]);
        return;
    }

    let start_time = Utc::now();

    let date = &args[1];
    let config_path = &args[2];
    let input_path = &args[3];

    let date = NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M").expect("Could not parse date");
    let date = DateTime::from_naive_utc_and_offset(date, Utc);

    let config = Config::new(&config_path, date).expect("Could not configure model");

    let mut output_writer = config
        .get_output_writer()
        .expect("Could not configure output writer");

    let props = config.get_properties();
    let mut state = config.new_state();

    let lats = config.properties.lats.as_slice().expect("should unwrap");
    let lons = config.properties.lons.as_slice().expect("should unwrap");

    let c = Utc::now();
    info!("Loading input data from {}", input_path);
    let handler = InputDataHandler::new(input_path, lats, lons);
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - c
    );
    let len = state.len();

    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(&handler, &time, len);

        let c = Utc::now();
        state.update(props, &input);
        trace!("Updating state took {} seconds", Utc::now() - c);

        if config.should_write_output(&state.time) {
            let c = Utc::now();
            let output = state.output(props, &input);
            trace!("Generating output took {} seconds", Utc::now() - c);

            let c = Utc::now();
            match output_writer.write_output(lats, lons, &output) {
                Ok(_) => (),
                Err(err) => warn!("Error writing output: {}", err),
            };
            trace!("Writing output took {} seconds", Utc::now() - c);
        }

        if time.hour() == 0 {
            let c = Utc::now();
            match config.write_warm_state(&state) {
                Ok(_) => (),
                Err(err) => warn!("Error writing warm state: {}", err),
            };
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    let elapsed_time = Utc::now() - start_time;
    info!("Elapsed time: {} seconds", elapsed_time.num_seconds());
}
