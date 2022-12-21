#![allow(dead_code)]



use chrono::{DateTime, Utc};
use library::{config::models::{Config}, io::models::grid::{ClusterMode, RegularGrid, GridFunctions}, state::models::{State, CellOutput}};
mod library;


fn main(){
    let date = Utc::now();
    let config = Config::new("data/config.txt", date)
        .unwrap();

    print!("{:#?}", config)
}
