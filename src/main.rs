// import state from lib
mod library;

use chrono::{prelude::*};
use library::state::models::State;

use crate::library::config::data::read_cells_properties;


fn main() {
    let file_path = "/Users/mirko/development/risico/convert/data/world.txt";
    let cells_properties = read_cells_properties(file_path).unwrap();
    let ncells = cells_properties.len();

    let start_time = Utc::now();
    let state: State = State::new(&cells_properties);
    

    let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
    print!("{} cells created in {} msec\n", ncells, elapsed);
    

    for i in 0..10 {
        print!("{} ", i);
        let start_time = Utc::now();
        let _ = state.update();
        let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
        print!("{} cells updated in {} msec\n", ncells, elapsed);
    }

    

}
