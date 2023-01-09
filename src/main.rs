#![allow(dead_code)]
// import state from lib
mod library;
use std::env::args;

use chrono::prelude::*;

use crate::library::{
    config::models::{Config, InputDataHandler},
    helpers::get_input,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

use git_version::git_version;
const GIT_VERSION: &str = git_version!();

// parse command line arguments: first argument is model date in the form YYYYMMDDHHMM, second is configuration path, third is input path
fn main() {
    println!("RISICO.rs {VERSION}.{GIT_VERSION}");
    let args: Vec<String> = args().collect();
    if args.len() != 4 {
        panic!("Usage: {} YYYYMMDDHHMM config_path input_path", args[0]);
    }

    let start_time = Utc::now();

    let date = &args[1];
    let config_path = &args[2];
    let input_path = &args[3];

    let date = Utc.datetime_from_str(date, "%Y%m%d%H%M")
        .expect("Could not parse date");
    let config = Config::new(&config_path, date)
        .expect("Could not configure model");


    let mut output_writer = config
        .get_output_writer()
        .expect("Could not configure output writer");

    let props = config.get_properties();
    let mut state = config.new_state();

    let lats = config.properties.lats.as_slice()
        .expect("should unwrap");
    let lons = config.properties.lons.as_slice()
        .expect("should unwrap");
    
    let c = Utc::now();
    println!("Loading input data from {}", input_path);
    let handler = InputDataHandler::new(input_path, lats, lons);
    println!("Loading input configuration took {} seconds", Utc::now() - c);
    let len = state.len();

    let timeline = handler.get_timeline();
    for time in timeline {
        let step_time = Utc::now();
        println!("Processing {}", time.format("%Y-%m-%d %H:%M"));
        let input = get_input(&handler, &time, len);
        
        let c = Utc::now();
        state.update(props, &input);
        println!("Updating state took {} seconds", Utc::now() - c);

        if config.should_write_output(&state.time) {
            let c = Utc::now();
            let output = state.output(props, &input);
            println!("Generating output took {} seconds", Utc::now() - c);

            let c = Utc::now();
            match output_writer.write_output(lats, lons, &output) {
                Ok(_) => (),
                Err(err) => println!("Error writing output: {}", err),
            };
            println!("Writing output took {} seconds", Utc::now() - c);
        }

        if time.hour() == 0 {
            let c = Utc::now();
            match config.write_warm_state(&state) {
                Ok(_) => (),
                Err(err) => println!("Error writing warm state: {}", err),
            };
            println!("Writing warm state took {} seconds", Utc::now() - c);
        }
        println!("Step took {} seconds", Utc::now() - step_time);
    }
    let elapsed_time = Utc::now() - start_time;
    println!("Elapsed time: {} seconds", elapsed_time.num_seconds());
}
