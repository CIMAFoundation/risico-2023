// import state from lib
mod library;

use chrono::{prelude::*};
use library::{state::models::State, config::models::read_input_from_file};

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
    read_input_from_file("data/input/202101310300_GFS025-6473ffdb_H.zbin");
}