use std::{f32::consts::PI, fmt::Display};

use chrono::{DateTime, Utc};
use itertools::izip;

use ndarray::{azip, Array1, Zip};
// use png::text_metadata;  // REMOVED
use risico::{
    constants::NODATAVAL,
    models::input::{Input, InputElement, InputVariableName::*},
};

use crate::common::io::readers::prelude::InputHandler;

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
    if let Some(src) = src {
        replace(dst, src, fun)
    }
}

/// Get the input data from the input handler and dave in the Input struct
/// If the input data are not in the expected units, the function will convert them
pub fn get_input(handler: &dyn InputHandler, time: &DateTime<Utc>, len: usize) -> Input {
    let mut data: Array1<InputElement> = Array1::default(len);

    // Observed temperature
    let temperature_obs = handler.get_values(K, time);
    if let Some(mut t) = temperature_obs {
        t.mapv_inplace(|_t| if _t > 200.0 { _t - 273.15 } else { _t }); // conversion to Celsius
        replace(&mut data, &t, |i| &mut i.temperature); // save observed temperature
    }

    // Observed relative humidity
    let humidity_obs = handler.get_values(F, time); // supposed in %
    maybe_replace(&mut data, &humidity_obs, |i| &mut i.humidity); // save observed relative humidity if any

    // Forecasted relative humidity
    let humidity = handler.get_values(H, time); // supposed in %
    maybe_replace(&mut data, &humidity, |i| &mut i.humidity); // save forecasted relative humidity if any

    // Forecasted temperature
    let temperature = handler.get_values(T, time);
    if let Some(mut t) = temperature {
        t.mapv_inplace(|_t| if _t > 200.0 { _t - 273.15 } else { _t }); // conversion to Celsius
        replace(&mut data, &t, |i| &mut i.temperature); // save forecasted temperature

        // Forecasted dew point temperature
        let temp_dew = handler.get_values(R, time);
        if let Some(mut td) = temp_dew {
            // if the dew point temperature is available
            td.mapv_inplace(|_t| if _t > 200.0 { _t - 273.15 } else { _t }); // conversion to Celsius
            replace(&mut data, &td, |i| &mut i.temp_dew);

            // computation of the relative humidity from the forecasted temperature and dew point temperature
            let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                h in &mut h,  // %
                r in &td,  // °C
                t in &t  // °C
            ){
                if *r > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                    *h = 100.0*(f32::exp((17.67 * r)/(r + 243.5))/f32::exp((17.67 * t)/(t + 243.5)));
                }
            });
            replace(&mut data, &h, |i| &mut i.humidity); // replace the humidity values

            // compute the vapor pressure deficit from temperature and relative humidity computed now
            let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                vpd in &mut vpd,
                h in &h,  // %
                t in &t  // °C
            ){
                if *h > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                    // difference between saturation vapor pressure and actual vapor pressure [hPa]
                    *vpd = (6.112 * f32::exp((17.67 * t)/(t + 243.5))) - (h/100.0 * 6.112 * f32::exp((17.67 * t)/(t + 243.5)));
                }
            });
            replace(&mut data, &vpd, |i| &mut i.vpd);
        } else {
            // if the dew point temperature is not available
            // compute the temperature dew point from the forecasted temperature and relative humidity
            if let Some(h) = humidity {
                // you need the relative humidity
                let mut r: Array1<f32> = Array1::ones(len) * NODATAVAL;
                azip!((
                    r in &mut r,
                    h in &h,  // %
                    t in &t  // °C
                ){
                    if *h > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                        let mut h = *h;
                        if h > 100.0 {
                            h = 100.0;
                        }
                        // Magnus formula (https://en.wikipedia.org/wiki/Dew_point)
                        let gamma = f32::ln(h / 100.0) + ((17.625 * t) / (t + 243.04));
                        *r = (243.04 * gamma) / (17.625 - gamma);
                    }
                });
                replace(&mut data, &r, |i| &mut i.temp_dew);

                // compute the vapor pressure deficit from temperature and relative humidity
                let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;
                azip!((
                    vpd in &mut vpd,
                    h in &h,  // %
                    t in &t  // °C
                ){
                    if *h > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                        let mut h = *h;
                        if h > 100.0 {
                            h = 100.0;
                        }
                        // difference between saturation vapor pressure and actual vapor pressure [hPa]
                        *vpd = (6.112 * f32::exp((17.67 * t)/(t + 243.5))) - (h/100.0 * 6.112 * f32::exp((17.67 * t)/(t + 243.5)));
                    }
                });
                replace(&mut data, &vpd, |i| &mut i.vpd);
            }
        }

        // compute the relative humidity from specific humidity and surface pressure
        // forecasted surface pressure
        let psfc = handler.get_values(PSFC, time); // supposed in Pa
                                                   // forecasted specific humidity
        let q = handler.get_values(Q, time); // supposed in kg/kg
        if let (Some(psfc), Some(q)) = (psfc, q) {
            // compute the relative humidity from the forecasted temperature, surface pressure and specific humidity
            let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                h in &mut h,
                q in &q, // kg/kg
                p in &psfc, // Pa
                t in &t // °C
            ){
                if *q > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) && *p > (NODATAVAL+1.0) {
                    // this implements the following cdo formula
                    // T_C=temperature in °C; P_hPa=pressure in hPa; Q2=specific humidity at 2m
                    // e=(Q2*P_hPa/(0.622+Q2)); es=6.112*exp((17.67*T_C)/(T_C+243.5)); RH=(e/es)*100;
                    *h = 100.0 * (q * (p/100.0) / (0.622 + q)) / (6.112 * f32::exp((17.67 * t)/(t + 243.5)));
                }
            });
            replace(&mut data, &h, |i| &mut i.humidity);

            // compute the dew point temperature wiht the new computed relative humidity
            let mut td: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                td in &mut td,
                h in &h, // %
                t in &t // °C
            ){
                if *h > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                    let mut h = *h;
                    if h > 100.0 {
                        h = 100.0;
                    }
                    // Magnus formula (https://en.wikipedia.org/wiki/Dew_point)
                    let gamma = f32::ln(h / 100.0) + ((17.625 * t) / (t + 243.04));
                    *td = (243.04 * gamma) / (17.625 - gamma);
                }
            });
            replace(&mut data, &td, |i| &mut i.temp_dew);

            // compute the vapor pressure deficit
            let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                vpd in &mut vpd,
                t in &t, // °C
                q in &q,  // kg/kg
                p in &psfc  // Pa
            ){
                if *q > (NODATAVAL+1.0) && *p > (NODATAVAL+1.0) {
                    // difference between saturation vapor pressure and actual vapor pressure [hPa]
                    *vpd = (6.112 * f32::exp((17.67 * t)/(t + 243.5))) - (q * (p/100.0) / (0.622 + q));
                }
            });
            replace(&mut data, &vpd, |i| &mut i.vpd);
        }
    }

    // wind speed and wind direction
    let ws = handler.get_values(W, time); // supposed in m/s
    let wd = handler.get_values(D, time); // supposed in degree
    if let Some(ws) = ws {
        let ws = ws.mapv(|_ws| {
            if _ws > -9998.0 {
                _ws * 3600.0 // conversion to m/h
            } else {
                NODATAVAL
            }
        });
        // save data
        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }
    if let Some(wd) = wd {
        let wd = wd.mapv(|_wd| {
            let mut _wd = _wd / 180.0 * PI; // conversion to rad
            if _wd < 0.0 {
                _wd += PI * 2.0;
            }
            _wd
        });
        // save data
        replace(&mut data, &wd, |i| &mut i.wind_dir);
    }

    // U and V components of the wind
    let u = handler.get_values(U, time); // supposed in m/s
    let v = handler.get_values(V, time); // supposed in m/s
    if let (Some(u), Some(v)) = (u, v) {
        // compute wind speed
        let ws = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u < (NODATAVAL + 1.0) || *_v < (NODATAVAL + 1.0) {
                    return NODATAVAL;
                }

                f32::sqrt(_u * _u + _v * _v) * 3600.0 // conversion to m/h
            })
            .collect::<Array1<f32>>();
        // compute wind direction
        let wd = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u < -9998.0 || *_v < -9998.0 {
                    return NODATAVAL; // there is no data
                }
                // from https://confluence.ecmwf.int/pages/viewpage.action?pageId=133262398
                (PI + f32::atan2(*_u, *_v)) % (PI * 2.0)
            })
            .collect::<Array1<f32>>();
        // save data
        replace(&mut data, &wd, |i| &mut i.wind_dir);
        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }

    // Observed precipitation
    let op = handler.get_values(O, time); // supposed in mm
    maybe_replace(&mut data, &op, |i| &mut i.rain);

    // Forecast precipitation
    let fp = handler.get_values(P, time); // supposed in mm
    maybe_replace(&mut data, &fp, |i| &mut i.rain);

    // Forecasted snow cover
    let snow = handler.get_values(SNOW, time); // supposed in mm
    maybe_replace(&mut data, &snow, |i| &mut i.snow_cover);

    // SATELLITE VARIABLES

    let swi = handler.get_values(SWI, time);
    maybe_replace(&mut data, &swi, |i| &mut i.swi);

    let ndvi = handler.get_values(NDVI, time);
    maybe_replace(&mut data, &ndvi, |i| &mut i.ndvi);

    let ndwi = handler.get_values(NDWI, time);
    maybe_replace(&mut data, &ndwi, |i| &mut i.ndwi);

    let msi = handler.get_values(M, time);
    maybe_replace(&mut data, &msi, |i| &mut i.msi);

    Input {
        time: time.to_owned(),
        data,
    }
}

#[derive(Debug)]
pub struct RISICOError {
    msg: String,
}

impl From<String> for RISICOError {
    fn from(msg: String) -> Self {
        RISICOError { msg }
    }
}

impl From<RISICOError> for String {
    fn from(value: RISICOError) -> String {
        value.msg
    }
}

impl From<&str> for RISICOError {
    fn from(msg: &str) -> Self {
        RISICOError { msg: msg.into() }
    }
}

impl Display for RISICOError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}
