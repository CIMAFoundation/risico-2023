#![allow(dead_code)]
// import state from lib
mod library;
use chrono::{prelude::*};

use crate::library::config::models::{Config, InputDataHandler};

fn main() {
    let start_time = Utc::now();

    let date = Utc.datetime_from_str("202102010000", "%Y%m%d%H%M").unwrap();
    let config = Config::new("/opt/risico/RISICOETHIOPIA/configuration.txt", Utc::now()).unwrap();
    let state = config.new_state(date);
    let input_path = "/opt/risico/RISICOETHIOPIA/INPUT/input.txt";

    let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
    println!("state created in {} msec\n", elapsed);

    
    let mut handler = InputDataHandler::new(&input_path);

    for time in handler.get_timeline() {
        print!("Time: {}\n", time);        
        let variables = handler.get_variables(&time);
        for name in variables {
            print!("Variable: {}\n", name);
        }
    }


    let timeline = handler.get_timeline();

    let (lats, lons) = state.coords();
    for time in timeline {
        print!("{} ", time);
        let start_time = Utc::now();
        handler.load_data(&time, &lats, &lons);

        let new_state = state.update(&handler, &time);
        let state = new_state;
        
        let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
        println!("state updated in {} msec\n", elapsed);

        match config.write_output(&state) {
            Ok(_) => (),
            Err(err) => println!("Error writing output: {}", err)
        };
        
        if time.hour() == 0 {
            match config.write_warm_state(&state) {
                Ok(_) => (),
                Err(err) => println!("Error writing warm state: {}", err)
            };                
        }
    }
}


