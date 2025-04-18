mod common;
use std::env::{set_var, var};
use std::error::Error;
use std::path::Path;

use chrono::prelude::*;
use clap::{arg, command, Parser};

use common::config::builder::{
    AngstromConfigBuilder,
    ConfigBuilderType,
    ConfigContainer,
    FWIConfigBuilder,
    FosbergConfigBuilder,
    //    PortugueseConfigBuilder,
    HdwConfigBuilder,
    KbdiConfigBuilder,
    Mark5ConfigBuilder,
    NesterovConfigBuilder,
    OrieuxConfigBuilder,
    PaletteMap,
    RISICOConfigBuilder,
    SharplesConfigBuilder,
};
use common::helpers::{get_input, RISICOError};
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

fn run_risico(
    model_config: &RISICOConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    // run risico
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;

    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;

    let props = config.get_properties();
    let mut state = config.new_state();

    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());

    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");

    let current_time = Utc::now();
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );

    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);

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
        if config.should_write_warm_state(&state.time) {
            info!("Writing warm state");
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state, state.time) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

fn run_fwi(
    model_config: &FWIConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    // run risico
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;

    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;

    let props = config.get_properties();
    let mut state = config.new_state();

    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());

    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");

    let current_time = Utc::now();
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );

    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);

        let c = Utc::now();
        state.update(props, &input);
        trace!("Updating state took {} seconds", Utc::now() - c);

        if config.should_write_output(&state.time) {
            let c = Utc::now();
            let output = state.output(&input);
            trace!("Generating output took {} seconds", Utc::now() - c);

            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
        }

        if config.should_write_warm_state(&state.time) {
            info!("Writing warm state");
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state, state.time) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

fn run_mark5(
    model_config: &Mark5ConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    // run risico
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;

    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;

    let props = config.get_properties();
    let mut state = config.new_state();

    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());

    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");

    let current_time = Utc::now();
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );

    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);

        // store the input of the day
        state.store(&input, props);

        if config.should_write_warm_state(&state.time) {
            // update the state with the input of the day and compute output
            let c = Utc::now();
            let output = state.output(props);
            trace!("Generating output took {} seconds", Utc::now() - c);

            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);

            info!("Writing warm state");
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state, state.time) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

fn run_kbdi(
    model_config: &KbdiConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // build configuration
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let props = config.get_properties(); // get properties
    let mut state = config.new_state(); // get state
                                        // set coordinates for the input handlerq
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);
        // store the input of the day
        state.store(&input);
        // check if we should write the output
        if config.should_write_warm_state(&state.time) {
            // update the state with the input of the day
            let c = Utc::now();
            state.update(props);
            trace!("updating state took {} seconds", Utc::now() - c);
            // compute output
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            // write the output
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
            // write the warm state
            info!("Writing warm state");
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state, state.time) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

/// Run Angstrom index
fn run_angstrom(
    model_config: &AngstromConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // configure the model
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let mut state = config.new_state(); // inizialized the state
                                        // set coordinates for the input handler
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    // explore the timeline
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);
        // store the input
        state.store(&input);
        // check if we should write the output
        if config.should_write_output(&state.time) {
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

/// Run Fosberg index
fn run_fosberg(
    model_config: &FosbergConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // configure the model
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let mut state = config.new_state(); // initialize the state
                                        // set coordinates for the input handler
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    // explore the timeline
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);
        // store the input data
        state.store(&input);
        // check if we should write the output
        if config.should_write_output(&state.time) {
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

/// Run Nesterov index
fn run_nesterov(
    model_config: &NesterovConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // configure the model
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let props = config.get_properties(); // get properties
    let mut state = config.new_state(); // initialize the state
                                        // set coordinates for the input handler
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    // explore the timeline
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);
        // store the input of the day
        state.store(&input, props);
        // check if we should write the output
        if config.should_write_warm_state(&state.time) {
            // update the state with the input of the day
            let c = Utc::now();
            state.update();
            trace!("Generating update took {} seconds", Utc::now() - c);
            // compute output
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            // write the output
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
            // write the warm state
            info!("Writing warm state");
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state, state.time) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

/// Run Sharples index
fn run_sharples(
    model_config: &SharplesConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // configure the model
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let mut state = config.new_state(); // initialize the state
                                        // set coordinates for the input handler
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    // explore the timeline
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        // store the input
        let input = get_input(handler, &time, len);
        state.store(&input);
        if config.should_write_output(&state.time) {
            // compute the output
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            // write the output
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

// Run Orieux index
fn run_orieux(
    model_config: &OrieuxConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // configure the model
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let props = config.get_properties(); // get properties
    let mut state = config.new_state(); // initialize the state
                                        // set coordinates for the input handler
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    // explore the timeline
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);
        // store the input of the day
        state.store(&input);
        if config.should_write_warm_state(&state.time) {
            // update the state with the input of the day
            let c = Utc::now();
            state.update(props);
            trace!("Generating update took {} seconds", Utc::now() - c);
            // compute output
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            // write the output
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
            // write the warm state
            info!("Writing warm state");
            let c = Utc::now();
            if let Err(err) = config.write_warm_state(&state, state.time) {
                warn!("Error writing warm state: {}", err);
            }
            trace!("Writing warm state took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

// Run Portuguese index
// fn run_portuguese(
//     model_config: &PortugueseConfigBuilder,
//     date: &DateTime<Utc>,
//     handler: &mut dyn InputHandler,
//     palettes: &PaletteMap,
// ) -> Result<(), RISICOError> {
//     let current_time = Utc::now();
//     // configuration of the model
//     let config = model_config
//         .build(date, palettes)
//         .maerrp_err(|_| format!("Could not configure model {err}"))?;
//     let mut output_writer = config
//         .get_output_writer()
//         .map_err(|_| "Could not configure output writer")?;
//     let props = config.get_properties();  // get the properties
//     let mut state = config.new_state();  // initialize the state
//     // set coordinates for the input handler
//     let (lats, lons) = config.get_properties().get_coords();
//     let (lats, lons) = (lats.as_slice(), lons.as_slice());
//     handler.set_coordinates(lats, lons).expect("Should set coordinates");
//     trace!(
//         "Loading input configuration took {} seconds",
//         Utc::now() - current_time
//     );
//     // explore the timeline
//     let len = state.len();
//     let timeline = handler.get_timeline();
//     for time in timeline {
//         let step_time = Utc::now();
//         info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
//         let input = get_input(handler, &time, len);
//         // store the input of the day
//         state.store(&input, &props);
//         // check if we should write the output
//         let (should_write_warm, warm_state_time) = config.should_write_warm_state(&time);
//         if  should_write_warm{
//             // update the state with the input of the day
//             let c = Utc::now();
//             state.update();
//             trace!("Generating update took {} seconds", Utc::now() - c);
//             // compute output
//             let c = Utc::now();
//             let output = state.output();
//             trace!("Generating output took {} seconds", Utc::now() - c);
//             // write the outut
//             let c = Utc::now();
//             if let Err(err) = output_writer.write_output(lats, lons, &output) {
//                 warn!("Error writing output: {}", err);
//             }
//             trace!("Writing output took {} seconds", Utc::now() - c);
//             // write the warm state
//             info!("Writing warm state");
//             let c = Utc::now();
//             if let Err(err) = config.write_warm_state(&state, warm_state_time) {
//                 warn!("Error writing warm state: {}", err);
//             }
//             trace!("Writing warm state took {} seconds", Utc::now() - c);
//         }
//         trace!("Step took {} seconds", Utc::now() - step_time);
//     }
//     Ok(())
// }

// Run Hot-Dry-Wind index
fn run_hdw(
    model_config: &HdwConfigBuilder,
    date: &DateTime<Utc>,
    handler: &mut dyn InputHandler,
    palettes: &PaletteMap,
) -> Result<(), RISICOError> {
    let current_time = Utc::now();
    // configuration of the model
    let config = model_config
        .build(date, palettes)
        .map_err(|err| format!("Could not configure model {err}"))?;
    let mut output_writer = config
        .get_output_writer()
        .map_err(|_| "Could not configure output writer")?;
    let mut state = config.new_state(); // initialize the state
                                        // set coordinates for the input handler
    let (lats, lons) = config.get_properties().get_coords();
    let (lats, lons) = (lats.as_slice(), lons.as_slice());
    handler
        .set_coordinates(lats, lons)
        .expect("Should set coordinates");
    trace!(
        "Loading input configuration took {} seconds",
        Utc::now() - current_time
    );
    // explore the timeline
    let len = state.len();
    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        info!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(handler, &time, len);
        // store the input
        state.store(&input);
        if config.should_write_output(&state.time) {
            // compute the output
            let c = Utc::now();
            let output = state.output();
            trace!("Generating output took {} seconds", Utc::now() - c);
            // write the output
            let c = Utc::now();
            if let Err(err) = output_writer.write_output(lats, lons, &output) {
                warn!("Error writing output: {}", err);
            }
            trace!("Writing output took {} seconds", Utc::now() - c);
        }
        trace!("Step took {} seconds", Utc::now() - step_time);
    }
    Ok(())
}

fn get_input_handler(
    input_path_str: &str,
    configs: &ConfigContainer,
) -> Result<Box<dyn InputHandler>, Box<dyn Error>> {
    // check if input_path is a file or a directory
    let input_path = Path::new(&input_path_str);
    let handler: Box<dyn InputHandler> = if input_path.is_file() {
        info!(
            "Loading input data from {} using BinaryInputHandler",
            input_path_str
        );
        // if it is a file, we are loading the legacy input.txt file and binary inputs
        Box::new(BinaryInputHandler::new(input_path_str).map_err(|_| "Could not load input data")?)
    } else if input_path.is_dir() {
        info!(
            "Loading input data from {} using NetCdfInputHandler",
            input_path_str
        );
        // we should load the netcdfs using the netcdfinputhandler
        let nc_config = if let Some(nc_config) = configs.get_netcdf_input_config() {
            nc_config.clone()
        } else {
            NetCdfInputConfiguration::default()
        };

        Box::new(
            NetCdfInputHandler::new(input_path_str, &nc_config)
                .map_err(|_| "Could not load input data")?,
        )
    } else {
        return Err(format!("Input path {} is not valid", input_path_str).into());
    };

    Ok(handler)
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

    if !Path::new(&config_path_str).is_file() {
        return Err(format!("Config file {} is not a file", config_path_str).into());
    }

    let date = NaiveDateTime::parse_from_str(&date_str, "%Y%m%d%H%M")
        .map_err(|_| format!("Could not parse run date '{}'", date_str))?;

    let date = DateTime::from_naive_utc_and_offset(date, Utc);

    let configs = ConfigContainer::from_file(&config_path_str)
        .map_err(|err| format!("Failed to load config: {}", err))?;

    // check if input_path is a file or a directory
    let mut input_handler = get_input_handler(&input_path_str, &configs)?;
    info!("Input files:\n{}", input_handler.info_input());

    for model_config in &configs.models {
        info!("Running model: {:?}", model_config.get_model_name());
        let start_time = Utc::now();

        let model_run = match model_config {
            ConfigBuilderType::FWI(model_config) => run_fwi(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::RISICO(model_config) => run_risico(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::Mark5(model_config) => run_mark5(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::KBDI(model_config) => run_kbdi(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::Angstrom(model_config) => run_angstrom(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::Fosberg(model_config) => run_fosberg(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::Nesterov(model_config) => run_nesterov(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::Sharples(model_config) => run_sharples(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            ConfigBuilderType::Orieux(model_config) => run_orieux(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
            // ConfigBuilderType::Portuguese(model_config) => run_portuguese(
            //     model_config,
            //     &date,
            //     input_handler.as_mut(),
            //     &configs.palettes,
            // ),
            ConfigBuilderType::Hdw(model_config) => run_hdw(
                model_config,
                &date,
                input_handler.as_mut(),
                &configs.palettes,
            ),
        };

        if let Err(err) = model_run {
            warn!("Error running model: {}", err);
        }

        let elapsed_time = Utc::now() - start_time;
        info!("Elapsed time: {} seconds", elapsed_time.num_seconds());
    }

    Ok(())
}
