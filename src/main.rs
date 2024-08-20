#![allow(dead_code)]
// import state from lib
mod library;
use std::env::{args, set_var, var};
use std::path::Path;
use std::process::exit;

use chrono::prelude::*;
use library::io::readers::netcdf::{NetCdfInputConfiguration, NetCdfInputHandler};
use log::{error, info, trace, warn};
use pretty_env_logger;

use crate::library::io::readers::binary::BinaryInputDataHandler;
use crate::library::io::readers::prelude::InputHandler;
use crate::library::{config::models::Config, helpers::get_input, version::GIT_VERSION};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// parse command line arguments: first argument is model date in the form YYYYMMDDHHMM, second is configuration path, third is input path
fn main() {
    if var("RUST_LOG").is_err() {
        set_var("RUST_LOG", "info")
    }
    pretty_env_logger::init();

    if GIT_VERSION == "__COMMIT__" {
        info!("RISICO-2023 v{}", VERSION);
    } else {
        info!("RISICO-2023 v{}-{}", VERSION, GIT_VERSION);
    }

    let args: Vec<String> = args().collect();
    if args.len() != 4 {
        info!("Usage: {} YYYYMMDDHHMM config_path input_path", args[0]);
        return;
    }

    let start_time = Utc::now();

    let date = &args[1];
    let config_path_str = &args[2];
    let input_path_str = &args[3];

    if !Path::new(&config_path_str).is_file() {
        error!("Config file {} is not a file", config_path_str);
        exit(1)
    }

    let date = NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
        .expect(&format!("Could not parse run date '{}'", date));

    let date = DateTime::from_naive_utc_and_offset(date, Utc);

    let config = Config::new(&config_path_str, date).expect("Could not configure model");

    let mut output_writer = config
        .get_output_writer()
        .expect("Could not configure output writer");

    let props = config.get_properties();
    let mut state = config.new_state();

    let (lats, lons) = config.properties.get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());

    let current_time = Utc::now();
    info!("Loading input data from {}", input_path_str);

    // check if input_path is a file or a directory
    let input_path = Path::new(input_path_str);
    let handler: Box<dyn InputHandler> = if input_path.is_file() {
        // if it is a file, we are loading the legacy input.txt file and binary inputs
        Box::new(
            BinaryInputDataHandler::new(input_path_str, lats, lons)
                .expect("Could not load input data"),
        )
    } else if input_path.is_dir() {
        // we should load the netcdfs using the netcdfinputhandler
        let nc_config = if let Some(nc_config) = &config.netcdf_input_configuration {
            nc_config
        } else {
            &NetCdfInputConfiguration::default()
        };
        Box::new(
            NetCdfInputHandler::new(input_path_str, lats, lons, nc_config)
                .expect("Could not load input data"),
        )
    } else {
        error!("Input path {} is not valid", input_path_str);
        exit(1);
    };

    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );

    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler.as_ref(), &time, len);

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
