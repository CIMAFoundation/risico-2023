// import state from lib
mod library;

use chrono::{prelude::*};
use library::{state::models::State, io::readers::read_input_from_file, config::models::InputDataHandler};

use crate::library::config::{data::read_cells_properties, models::Config};


// fn main() {
//     let cells_path = "data/ethiopia.txt";
//     let veg_path = "data/pveg_ethiopia.txt";
    
    

//     let start_time = Utc::now();
//     let config = Config::new(&cells_path, &veg_path);
//     let ncells = &config.cells.len();
    
//     let cells = config.init_state();
//     let state: State = State::new(cells, Utc::now());
    

//     let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
//     print!("{} cells created in {} msec\n", ncells, elapsed);
    

//     for i in 0..10 {
//         print!("{} ", i);
//         let start_time = Utc::now();
//         let _ = state.update();
//         let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
//         print!("{} cells updated in {} msec\n", ncells, elapsed);
//     }

    

// }


fn main() {
    let input_path = "data/input/input.txt";
    let handler = InputDataHandler::new(&input_path);
    for grid_name in handler.grid_registry.grids.keys() {
        println!("GRID SAVED: {}", grid_name);
    }

    for (date, data_map) in handler.data_map.iter() {
        println!(" .   DATE: {}", date);
        for (var_name, _) in data_map.iter() {
            println!("         VARIABLE: {}", var_name);
        }
    }

}