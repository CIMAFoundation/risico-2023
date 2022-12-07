#![allow(dead_code)]
// import state from lib
mod library;
use chrono::{prelude::*};
use library::{state::models::State, io::{readers::read_input_from_file, writers::write_netcdf}, config::models::InputDataHandler};
use crate::library::io::{models::grid::GridFunctions, writers::write_to_zbin_file};
use crate::library::{config::{data::read_cells_properties, models::Config}, io::models::grid::{Grid, RegularGrid}};

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
    
    let lats = cells.iter().map(
        |cell| {
            cell.properties.lat as f32
        }
    ).collect();
    let lons = cells.iter().map(|cell| {
        cell.properties.lon as f32
    }).collect();

    let state: State = State::new(cells, Utc::now());

    
    let GRIDNROWS=1148/4;
    let GRIDNCOLS=1497/4;
    let MINLAT=3.415;
    let MAXLAT=14.885;
    let MINLON=33.015;
    let MAXLON=47.975;

    let mut grid = RegularGrid::new(GRIDNROWS as usize, GRIDNCOLS as usize,  MINLAT, MINLON, MAXLAT, MAXLON);

    let mut grid_enum = Grid::Regular(grid.clone());

    for time in timeline {
        print!("{} ", time);
        let start_time = Utc::now();
        let new_state = state.update(&mut handler, &time);
        let state = new_state;
        
        let elapsed = Utc::now().signed_duration_since(start_time).num_milliseconds();
        print!("{} cells updated in {} msec\n", ncells, elapsed);


        let dffms = state.cells.iter().map(|cell| {
            cell.output.as_ref().unwrap().dffm as f32
        }).collect();

        let V = state.cells.iter().map(|cell| {
            cell.output.as_ref().unwrap().V as f32
        }).collect();


        let values = grid.project_to_grid(&lats, &lons, dffms);
        let file = format!("data/output/dffm_{}.zbin", time.format("%Y%m%d%H%M%S"));
        write_to_zbin_file(&file, &mut grid_enum, values).unwrap();


        let values = grid.project_to_grid(&lats, &lons, V);
        let file = format!("data/output/V_{}.zbin", time.format("%Y%m%d%H%M%S"));
        write_to_zbin_file(&file, &mut grid_enum, values).unwrap();

        
    }

    

}


// fn main() {
//     write_netcdf();
// }
