#![allow(dead_code)]
// import state from lib
mod library;
use chrono::{prelude::*};
use itertools::izip;
use ndarray::Array1;

use crate::library::{config::models::{Config, InputDataHandler}, state::models::Input};

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

    let coords = state.coords();
    let lats = coords.0;
    let lons = coords.1;

    for time in timeline {
        print!("{} ", time);
        let start_time = Utc::now();
        handler.load_data(&time, lats, lons);
        
        let t = handler.get_values("T", &time, lats, lons) -273.15;
        let u = handler.get_values("U", &time, lats, lons);
        let v = handler.get_values("V", &time, lats, lons);
        let p = handler.get_values("P", &time, lats, lons);
        let h = handler.get_values("H", &time, lats, lons);

        let wind_speed = 
            (u.mapv(|_u| _u.powi(2)) + v.mapv(|_v| _v.powi(2)))
            .mapv(f32::sqrt) * 3600.0;
        
        let wind_dir = izip!(u,v).map(|(_u, _v)| f32::atan2(_u, _v)).collect::<Array1<f32>>();

        let input = Input {
            time: time,
            temperature: t,
            wind_speed: wind_speed,
            wind_dir: wind_dir,
            humidity: h,
            rain: p,
            snow_cover: Array1::from_elem(lats.len(), 0.0),
            ndvi: Array1::from_elem(lats.len(), 1.0),
            ndwi: Array1::from_elem(lats.len(), 1.0),
        };

        let new_state = state.update(&input, &time);
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


