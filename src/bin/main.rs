mod common;
use std::env::{set_var, var};
use std::error::Error;
use std::path::Path;

use chrono::prelude::*;
use clap::{arg, command, Parser};

use common::config::builder::ConfigBuilder;
use common::helpers::get_input;
use common::io::readers::binary::BinaryInputHandler;
use common::io::readers::netcdf::{NetCdfInputConfiguration, NetCdfInputHandler};
use common::io::readers::prelude::InputHandler;
use log::{info, trace, warn};
use risico::version::LONG_VERSION;

#[derive(Parser, Debug)]
#[command(
    author="Mirko D'Andrea <mirko.dandrea@cimafoundation.org>, Nicolò Perello <nicolo.perello@cimafoundation.org>",
    version,
    long_version=LONG_VERSION,
    about="risico-2023 Wildfire Risk Assessment Model by CIMA Research Foundation", 
    long_about="RISICO  (Rischio Incendi E Coordinamento) is a wildfire risk forecast model written in rust and developed by CIMA Research Foundation. 
It is designed to predict the likelihood and potential impact of wildfires in a given region, given a set of input parameters."
)]
struct Args {
    #[arg(
        required = true,
        help = "Model date in the format YYYYMMDDHHMM",
        index = 1
    )]
    date: String,

    #[arg(required = true, help = "Path to the configuration file", index = 2)]
    config_path: String,

    #[arg(required = true, help = "Path to the input data file", index = 3)]
    input_path: String,
}

/// main function
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let date_str = args.date;
    let config_path_str = args.config_path;
    let input_path_str = args.input_path;

    if var("RUST_LOG").is_err() {
        set_var("RUST_LOG", "info")
    }
    pretty_env_logger::init();

    let start_time = Utc::now();

    if !Path::new(&config_path_str).is_file() {
        return Err(format!("Config file {} is not a file", config_path_str).into());
    }

    let date = NaiveDateTime::parse_from_str(&date_str, "%Y%m%d%H%M")
        .map_err(|_| format!("Could not parse run date '{}'", date_str))?;

    let date = DateTime::from_naive_utc_and_offset(date, Utc);

    let config = ConfigBuilder::from_file(&config_path_str)
        .map_err(|err| format!("Failed to build config: {}", err))?
        .build(&date)
        .map_err(|_| "Could not configure model")?;

    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;

    let props = config.get_properties();
    let mut state = config.new_state();

    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());

    let current_time = Utc::now();

    // check if input_path is a file or a directory
    let input_path = Path::new(&input_path_str);
    let handler: Box<dyn InputHandler> = if input_path.is_file() {
        info!(
            "Loading input data from {} using BinaryInputHandler",
            input_path_str
        );
        // if it is a file, we are loading the legacy input.txt file and binary inputs
        Box::new(
            BinaryInputHandler::new(&input_path_str, lats, lons)
                .map_err(|_| "Could not load input data")?,
        )
    } else if input_path.is_dir() {
        info!(
            "Loading input data from {} using NetCdfInputHandler",
            input_path_str
        );
        // we should load the netcdfs using the netcdfinputhandler
        let nc_config = if let Some(nc_config) = &config.get_netcdf_input_config() {
            nc_config.clone()
        } else {
            NetCdfInputConfiguration::default()
        };

        Box::new(
            NetCdfInputHandler::new(&input_path_str, lats, lons, &nc_config)
                .map_err(|_| "Could not load input data")?,
        )
    } else {
        return Err(format!("Input path {} is not valid", input_path_str).into());
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
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
        }

        if time.hour() == 0 {
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    let elapsed_time = Utc::now() - start_time;
    info!("Elapsed time: {} seconds", elapsed_time.num_seconds());

    Ok(())
}
