#![allow(dead_code)]
// import state from lib
mod library;
use std::env::args;

use chrono::prelude::*;

use crate::library::{
    config::models::{Config, InputDataHandler},
    helpers::get_input,
};

use git_version::git_version;
const GIT_VERSION: &str = git_version!();

// parse command line arguments: first argument is model date in the form YYYYMMDDHHMM, second is configuration path, third is input path
fn main() {
    println!("RISICO.rs version 1.0.0v{}", GIT_VERSION);
    let args: Vec<String> = args().collect();
    if args.len() != 4 {
        panic!("Usage: {} YYYYMMDDHHMM config_path input_path", args[0]);
    }
    
    let start_time = Utc::now();

    let date = &args[1];
    let config_path = &args[2];
    let input_path = &args[3];

    let date = Utc.datetime_from_str(date, "%Y%m%d%H%M").unwrap();
    let config = Config::new(&config_path, date).unwrap();
    let mut handler = InputDataHandler::new(input_path);

    let mut output_writer = config
        .get_output_writer()
        .expect("Could not configure output writer");

    let props = config.get_properties();
    let mut state = config.new_state();    

    let timeline = handler.get_timeline();
    let lats = config.properties.lats.as_slice().unwrap();
    let lons = config.properties.lons.as_slice().unwrap();

    for time in timeline {
        println!("{}", time.format("%Y%m%d%H%M"));

        handler.load_data(&time, lats, lons);

        let input = get_input(&handler, lats, lons, &time);

        state.update(props, &input);

        let output = state.output(props, &input);

        match output_writer.write_output(lats, lons, &output) {
            Ok(_) => (),
            Err(err) => println!("Error writing output: {}", err),
        };

        if time.hour() == 0 {
            let time = Utc::now();
            match config.write_warm_state(&state) {
                Ok(_) => (),
                Err(err) => println!("Error writing warm state: {}", err),
            };
            println!("Warm state written in {} ms", (Utc::now() - time).num_milliseconds());
        }
    }
    let elapsed_time = Utc::now() - start_time;
    println!("Elapsed time: {} seconds", elapsed_time.num_seconds());
}
