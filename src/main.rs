#![allow(dead_code)]
// import state from lib
mod library;
use chrono::prelude::*;
use itertools::izip;
use library::state::constants::{NODATAVAL, PI};
use ndarray::{azip, Array1};

use crate::library::{
    config::models::{Config, InputDataHandler},
    state::models::Input,
};

fn maybe_replace(dst: &mut Array1<f32>, src: &Option<Array1<f32>>) {   
    match src {
        Some(src) => azip!((
            dst in dst,
            src in src,
        ) {
            if *src > -9998.0 {
                *dst = *src;
            }
        }),
        None => (),
    }
}

fn get_input(
    handler: &InputDataHandler,
    lats: &[f32],
    lons: &[f32],
    time: &DateTime<Utc>,
) -> Input {
    let mut snow_cover: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut precipitation: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut temperature: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut wind_speed: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut wind_dir: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut humidity: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut ndvi: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut ndwi: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut ndsi: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut swi: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
    let mut msi: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;

    let maybe_snow = handler.get_values("SNOW", &time, lats, lons);
    
    maybe_replace(&mut snow_cover, &maybe_snow);

    // Observed relative humidity
    let h = handler.get_values("F", &time, lats, lons);
    maybe_replace(&mut humidity, &h);

    // forecasted relative humidity
    let h = handler.get_values("H", &time, lats, lons);
    maybe_replace(&mut humidity, &h);


    // Observed temperature
    let t = handler.get_values("K", &time, lats, lons);
    maybe_replace(&mut temperature, &t);

    // Forecasted temperature
    let t = handler.get_values("T", &time, lats, lons);
    maybe_replace(&mut temperature, &t);


    // wind speed
    let mut ws = handler.get_values("W", &time, lats, lons);
    // wind direction
    let wd = handler.get_values("D", &time, lats, lons);

    let u = handler.get_values("U", &time, lats, lons);
    let v = handler.get_values("V", &time, lats, lons);

    // Observed precipitation
    let op = handler.get_values("O", &time, lats, lons);
    maybe_replace(&mut precipitation, &op);
    // Forecast precipitation
    let fp = handler.get_values("P", &time, lats, lons);
    maybe_replace(&mut precipitation, &fp);

    let wind_speed = match ws {
        Some(ws) => ws.mapv(|_ws| {
            if _ws > -9998.0 {
                _ws * 3600.0
            } else {
                NODATAVAL
            }
        }),
        None => {
            let u = u.clone().unwrap();
            let v = v.clone().unwrap();
            (u.mapv(|_u| _u.powi(2)) + v.mapv(|_v| _v.powi(2))).mapv(f32::sqrt) * 3600.0
        }
    };
    let wind_dir = match wd {
        Some(wd) => wd.mapv(|_wd| {
            let mut _wd = _wd / 180.0 * PI;
            if _wd < 0.0 {
                _wd += PI * 2.0;
            }
            _wd
        }),
        None => {
            let u = u.unwrap();
            let v = v.unwrap();

            izip!(u, v)
                .map(|(_u, _v)| {
                    if _u < -9998.0 || _v < -9998.0 {
                        return NODATAVAL;
                    }
                    let mut wd = f32::atan2(_u, _v);
                    if wd < 0.0 {
                        wd = wd + PI * 2.0;
                    }
                    wd
                })
                .collect::<Array1<f32>>()
        }
    };

    let _ndsi = handler.get_values("N", &time, lats, lons);
    maybe_replace(&mut ndsi, &_ndsi);

    let _ndvi = handler.get_values("NDVI", &time, lats, lons);
    maybe_replace(&mut ndvi, &_ndvi);

    let _ndwi = handler.get_values("NDWI", &time, lats, lons);
    maybe_replace(&mut ndwi, &_ndwi);

    let _msi = handler.get_values("M", &time, lats, lons);
    maybe_replace(&mut msi, &_msi);
    

    Input {
        time: time.to_owned(),
        temperature: temperature,
        wind_speed: wind_speed,
        wind_dir: wind_dir,
        humidity: humidity,
        rain: precipitation,
        snow_cover: snow_cover,
        ndvi: ndvi,
        ndwi: ndwi,
        //[TODO] implement this
        ndsi: ndsi,
        swi: swi,
        msi: msi,
    }
}

fn main() {
    let start_time = Utc::now();

    let date = Utc.datetime_from_str("202102010000", "%Y%m%d%H%M").unwrap();
    let config = Config::new("/opt/risico/RISICOETHIOPIA/configuration.txt", date).unwrap();
    let props = config.get_properties();
    let mut state = config.new_state();
    let input_path = "/opt/risico/RISICOETHIOPIA/INPUT/input.txt";

    let elapsed = Utc::now()
        .signed_duration_since(start_time)
        .num_milliseconds();
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

    let lats = config.properties.lats.as_slice().unwrap();
    let lons = config.properties.lons.as_slice().unwrap();

    for time in timeline {
        print!("{} ", time);
        let start_time = Utc::now();
        handler.load_data(&time, lats, lons);

        let input = get_input(&handler, lats, lons, &time);

        state.update(props, &input, &time);

        let output = state.output(props, &input);

        let elapsed = Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds();
        println!("state updated in {} msec\n", elapsed);

        match config.write_output(&output, lats, lons) {
            Ok(_) => (),
            Err(err) => println!("Error writing output: {}", err),
        };

        if time.hour() == 0 {
            match config.write_warm_state(&state) {
                Ok(_) => (),
                Err(err) => println!("Error writing warm state: {}", err),
            };
        }
    }
}
