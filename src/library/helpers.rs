use std::f32::consts::PI;

use chrono::{DateTime, Utc};
use itertools::izip;

use ndarray::{azip, Array1, Zip};

use strum_macros::EnumString;

use super::modules::risico::{
    constants::NODATAVAL,
    models::{Input, InputElement},
};
use crate::library::io::readers::prelude::InputHandler;

fn replace<'a>(
    dst: &'a mut Array1<InputElement>,
    src: &Array1<f32>,
    fun: fn(&'a mut InputElement) -> &'a mut f32,
) {
    Zip::from(dst).and(src).par_for_each(|d, s| {
        let result = fun(d);
        if *result <= (NODATAVAL + 1.0) {
            *result = *s;
        }
    });
}

fn maybe_replace<'a>(
    dst: &'a mut Array1<InputElement>,
    src: &Option<Array1<f32>>,
    fun: fn(&'a mut InputElement) -> &'a mut f32,
) {
    match src {
        Some(src) => replace(dst, src, fun),
        None => (),
    }
}

pub fn get_input(handler: &dyn InputHandler, time: &DateTime<Utc>, len: usize) -> Input {
    let mut data: Array1<InputElement> = Array1::default(len);

    let snow = handler.get_values(&InputVariableName::SNOW, &time);

    maybe_replace(&mut data, &snow, |i| &mut i.snow_cover);

    // Observed relative humidity
    let h = handler.get_values(&InputVariableName::F, &time);
    maybe_replace(&mut data, &h, |i| &mut i.humidity);

    // forecasted relative humidity
    let h = handler.get_values(&InputVariableName::H, &time);
    maybe_replace(&mut data, &h, |i| &mut i.humidity);

    // Observed temperature
    let t = handler.get_values(&InputVariableName::K, &time);

    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });
        replace(&mut data, &t, |i| &mut i.temperature);
    }

    // Forecasted temperature
    let t = handler.get_values(&InputVariableName::T, &time);

    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });
        replace(&mut data, &t, |i| &mut i.temperature);

        // Forecasted dew point temperature
        let r = handler.get_values(&InputVariableName::R, &time);
        if let Some(r) = r {
            let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                h in &mut h,
                r in &r,
                t in &t
            ){
                if *r > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                    let mut r = *r;
                    if r > 200.0 {
                        r = r - 273.15;
                    }
                    *h = 100.0*(f32::exp((17.67 * r)/(r + 243.5))/f32::exp((17.67 * t)/(t + 243.5)));
                }
            });
            replace(&mut data, &h, |i| &mut i.humidity);
        }
    }

    // Observed precipitation
    let op = handler.get_values(&InputVariableName::O, &time);
    maybe_replace(&mut data, &op, |i| &mut i.rain);
    // Forecast precipitation
    let fp = handler.get_values(&InputVariableName::P, &time);
    maybe_replace(&mut data, &fp, |i| &mut i.rain);

    // wind speed
    let ws = handler.get_values(&InputVariableName::W, &time);
    // wind direction
    let wd = handler.get_values(&InputVariableName::D, &time);

    let u = handler.get_values(&InputVariableName::U, &time);
    let v = handler.get_values(&InputVariableName::V, &time);

    if let Some(ws) = ws {
        let ws = ws.mapv(|_ws| {
            if _ws > -9998.0 {
                _ws * 3600.0
            } else {
                NODATAVAL
            }
        });
        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }

    if let Some(wd) = wd {
        let wd = wd.mapv(|_wd| {
            let mut _wd = _wd / 180.0 * PI;
            if _wd < 0.0 {
                _wd += PI * 2.0;
            }
            _wd
        });
        replace(&mut data, &wd, |i| &mut i.wind_dir);
    }

    if let (Some(u), Some(v)) = (u, v) {
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
                if *_u < (NODATAVAL + 1.0) || *_v < (NODATAVAL + 1.0) {
                    return NODATAVAL;
                }

                let ws = f32::sqrt(_u * _u + _v * _v) * 3600.0;
                ws
            })
            .collect::<Array1<f32>>();
        replace(&mut data, &wd, |i| &mut i.wind_dir);

        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }

    let swi = handler.get_values(&InputVariableName::SWI, &time);
    maybe_replace(&mut data, &swi, |i| &mut i.swi);

    let ndvi = handler.get_values(&InputVariableName::NDVI, &time);
    maybe_replace(&mut data, &ndvi, |i| &mut i.ndvi);

    let ndwi = handler.get_values(&InputVariableName::NDWI, &time);
    maybe_replace(&mut data, &ndwi, |i| &mut i.ndwi);

    let msi = handler.get_values(&InputVariableName::M, &time);
    maybe_replace(&mut data, &msi, |i| &mut i.msi);

    Input {
        time: time.to_owned(),
        data,
    }
}

#[derive(Debug, PartialEq, Eq, EnumString, Hash, Copy, Clone)]
pub enum InputVariableName {
    /// Air Humidity
    H,
    /// Observed Temperature
    K,
    /// Forecasted Temperature
    T,
    /// SNOW Cover
    SNOW,
    /// Observed Air Humidity
    F,
    // Forecasted dew point temperature
    R,
    /// Observed Precipitation
    O,
    /// Forecasted Precipitation
    P,
    /// Wind Speed
    W,
    /// Wind Direction
    D,
    /// NDWI Value
    NDWI,
    /// NDVI Value
    NDVI,
    /// MSI Value
    M,
    /// U component of the wind
    U,
    /// V value of the wind
    V,
    /// SWI Value
    SWI,
}

// // Implement FromStr for InputVariable
// impl FromStr for InputVariableName {
//     type Err = ();

//     fn from_str(input: &str) -> Result<InputVariableName, Self::Err> {
//         match input {
//             "H" => Ok(InputVariableName::H),
//             "K" => Ok(InputVariableName::K),
//             "T" => Ok(InputVariableName::T),
//             "SNOW" => Ok(InputVariableName::SNOW),
//             "F" => Ok(InputVariableName::F),
//             "R" => Ok(InputVariableName::R),
//             "O" => Ok(InputVariableName::O),
//             "P" => Ok(InputVariableName::P),
//             "W" => Ok(InputVariableName::W),
//             "NDWI" => Ok(InputVariableName::NDWI),
//             "NDVI" => Ok(InputVariableName::NDVI),
//             "M" => Ok(InputVariableName::M),
//             "U" => Ok(InputVariableName::U),
//             "V" => Ok(InputVariableName::V),
//             "SWI" => Ok(InputVariableName::SWI),
//             _ => Err(()),
//         }
//     }
// }

// // Implement Display for InputVariableName
// impl fmt::Display for InputVariableName {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         let value = match self {
//             InputVariableName::D => "D",
//             InputVariableName::H => "H",
//             InputVariableName::K => "K",
//             InputVariableName::T => "T",
//             InputVariableName::SNOW => "SNOW",
//             InputVariableName::F => "F",
//             InputVariableName::R => "R",
//             InputVariableName::O => "O",
//             InputVariableName::P => "P",
//             InputVariableName::W => "W",
//             InputVariableName::NDWI => "NDWI",
//             InputVariableName::NDVI => "NDVI",
//             InputVariableName::M => "M",
//             InputVariableName::U => "U",
//             InputVariableName::V => "V",
//             InputVariableName::SWI => "SWI",
//         };
//         write!(f, "{}", value)
//     }
// }
