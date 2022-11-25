#![allow(dead_code)]
// import state from lib
mod library;
use chrono::{prelude::*};
use library::{state::models::State, io::{readers::read_input_from_file, writers::write_netcdf}, config::models::InputDataHandler};

use crate::library::config::{data::read_cells_properties, models::Config};

//use library::io::writers::write_netcdf;

fn main() {
    let cells_path = "data/ethiopia.txt";
    let veg_path = "data/pveg_ethiopia.txt";
    let input_path = "data/input/input.txt";
    

    let start_time = Utc::now();
    let config = Config::new(&cells_path, &veg_path);
    let ncells = &config.cells.len();
    
    let cells = config.init_state();
    
    

    let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
    print!("{} cells created in {} msec\n", ncells, elapsed);

    
    let mut handler = InputDataHandler::new(&input_path);

    for time in handler.get_timeline() {
        print!("Time: {}\n", time);        
        let variables = handler.get_variables(&time);
        for name in variables {
            print!("Variable: {}\n", name);
        }

        

    }


    let timeline = handler.get_timeline();
    let state: State = State::new(cells, Utc::now());
    for time in timeline {
        print!("{} ", time);
        let start_time = Utc::now();
        let new_state = state.update(&mut handler, &time);
        let state = new_state;
        
        let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
        print!("{} cells updated in {} msec\n", ncells, elapsed);
    }

    

}


// fn main() {
//     write_netcdf();
// }
