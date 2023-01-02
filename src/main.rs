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

fn replace(dst: &mut Array1<f32>, src: &Array1<f32>) {   
    azip!((
            dst in dst,
            src in src,
        ) {
            if *dst <= (NODATAVAL+1.0) {
                *dst = *src;
            }
        })
}

fn maybe_replace(dst: &mut Array1<f32>, src: &Option<Array1<f32>>) {   
    match src {
        Some(src) => replace(dst, src),
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

    let snow = handler.get_values("SNOW", &time, lats, lons);
    
    maybe_replace(&mut snow_cover, &snow);

    // Observed relative humidity
    let h = handler.get_values("F", &time, lats, lons);
    maybe_replace(&mut humidity, &h);

    // forecasted relative humidity
    let h = handler.get_values("H", &time, lats, lons);
    maybe_replace(&mut humidity, &h);


    // Observed temperature
    let t = handler.get_values("K", &time, lats, lons);
    
    if let Some(t) = t  { 
        let t = t.mapv(|_t| if _t > 200.0 {_t - 273.15} else { _t });
        replace(&mut temperature, &t);
    }


    // Forecasted temperature
    let t = handler.get_values("T", &time, lats, lons);
    
    
    if let Some(t) = t  { 
        let t = t.mapv(|_t| if _t > 200.0 {_t - 273.15} else { _t });
        replace(&mut temperature, &t);
        // Forecasted dew point temperature
        let r = handler.get_values("R", &time, lats, lons);
        if let Some(r) = r {
            let mut h: Array1<f32> = Array1::ones(lats.len()) * NODATAVAL;
            azip!((
                h in &mut h,
                r in &r, 
                t in &t
            ){
                if *r > NODATAVAL && *t > NODATAVAL {
                    let mut r = *r;
                    if r > 200.0 {
                        r = r - 273.15;
                    }
                    *h = 100.0*(f32::exp((17.67 * r)/(r + 243.5))/f32::exp((17.67 * t)/(t + 243.5)));
                }
            });
            replace(&mut humidity, &h);
        }
    }
    

    

    // Observed precipitation
    let op = handler.get_values("O", &time, lats, lons);
    maybe_replace(&mut precipitation, &op);
    // Forecast precipitation
    let fp = handler.get_values("P", &time, lats, lons);
    maybe_replace(&mut precipitation, &fp);

    // wind speed
    let ws = handler.get_values("W", &time, lats, lons);
    // wind direction
    let wd = handler.get_values("D", &time, lats, lons);

    let u = handler.get_values("U", &time, lats, lons);
    let v = handler.get_values("V", &time, lats, lons);

    if let Some(ws) = ws{
        let ws = ws
            .mapv(|_ws| if _ws > -9998.0 {_ws * 3600.0} else {NODATAVAL});
        replace(&mut wind_speed, &ws);
    }

    if let Some(wd) = wd {
        let wd = wd 
            .mapv(|_wd| {
            let mut _wd = _wd / 180.0 * PI;
            if _wd < 0.0 {
                _wd += PI * 2.0;
            }
            _wd
        });
        replace(&mut wind_dir, &wd);
    }
    
    if let (Some(u), Some(v)) =  (u, v) { 
        let wd = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u < -9998.0 || *_v < -9998.0 {
                    return NODATAVAL;
                }
                let mut wd = f32::atan2(*_u, *_v);
                
                if wd < 0.0 {
                    wd = wd + PI * 2.0;
                }
                wd
            })
            .collect::<Array1<f32>>();

        let ws = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u < -9998.0 || *_v < -9998.0 {
                    return NODATAVAL;
                }
                
                let ws = f32::sqrt(_u * _u + _v * _v) * 3600.0;
                ws
            })
            .collect::<Array1<f32>>();

        replace(&mut wind_dir, &wd);
        replace(&mut wind_speed, &ws);
    
    }

    let _swi = handler.get_values("SWI", &time, lats, lons);
    maybe_replace(&mut swi, &_swi);    

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
    let the_start_time = Utc::now();

    let date = Utc.datetime_from_str("202301020000", "%Y%m%d%H%M").unwrap();
    let config = Config::new("/opt/risico/RISICO2015/configuration.txt", date).unwrap();
    
    let mut output_writer = config.get_output_writer()
        .expect("Could not configure output writer");

    let props = config.get_properties();
    let mut state = config.new_state();
    let input_path = "/opt/risico/RISICO2015/INPUT/202301020842/input.txt";

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

        state.update(props, &input);

        let output = state.output(props, &input);


        match output_writer.write_output(&output, lats, lons) {
            Ok(_) => (),
            Err(err) => println!("Error writing output: {}", err),
        };

        if time.hour() == 0 {
            match config.write_warm_state(&state) {
                Ok(_) => (),
                Err(err) => println!("Error writing warm state: {}", err),
            };
        }

        let elapsed = Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds();
        println!("step elapsed time: {} msec\n", elapsed);

    }
    let elapsed = Utc::now()
        .signed_duration_since(the_start_time)
        .num_milliseconds();
    println!("total elapsed time: {} msec\n", elapsed);

    
}
