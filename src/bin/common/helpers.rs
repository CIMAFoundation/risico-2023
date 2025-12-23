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
    let temperature_obs = handler.get_values(K, time);  // supposed in K or °C
    if let Some(mut t) = temperature_obs {
        t.mapv_inplace(|_t| if _t > 200.0 { _t - 273.15 } else { _t }); // conversion to Celsius
        replace(&mut data, &t, |i| &mut i.temperature); // save observed temperature [°C]
    }

    // Observed relative humidity
    let humidity_obs = handler.get_values(F, time); // supposed in %
    maybe_replace(&mut data, &humidity_obs, |i| &mut i.humidity); // save observed relative humidity if any [%]

    // Forecasted relative humidity
    let humidity = handler.get_values(H, time); // supposed in %
    maybe_replace(&mut data, &humidity, |i| &mut i.humidity); // save forecasted relative humidity if any [%]

    // Forecasted temperature
    let temperature = handler.get_values(T, time);  // supposed in K or °C
    if let Some(mut t) = temperature {
        t.mapv_inplace(|_t| if _t > 200.0 { _t - 273.15 } else { _t }); // conversion to Celsius
        replace(&mut data, &t, |i| &mut i.temperature); // save forecasted temperature [°C]

        // Forecasted dew point temperature
        let temp_dew = handler.get_values(R, time);  // supposed in K or °C
        if let Some(mut td) = temp_dew {
            // if the dew point temperature is available
            td.mapv_inplace(|_t| if _t > 200.0 { _t - 273.15 } else { _t }); // conversion to Celsius
            replace(&mut data, &td, |i| &mut i.temp_dew);  // save dew point temperature [°C]

            // computation of the relative humidity and VPD from the forecasted temperature and dew point temperature
            let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;  // [%]
            let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;  // [hPa]
            azip!((
                h in &mut h,  // %
                v in &mut vpd,  // hPa
                r in &td,  // °C
                t in &t  // °C
            ){
                if *r > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                    // compute the relative humidity > https://cran.r-project.org/web/packages/humidity/vignettes/humidity-measures.html
                    // August–Roche–Magnus formula > https://en.wikipedia.org/wiki/Dew_point
                    let es = 6.1094 * f32::exp((17.625 * t)/(t + 243.04));  // saturation vapor pressure [hPa]
                    let e = 6.1094 * f32::exp((17.625 * r)/(r + 243.04));  // vapor pressure [hPa] > computed substituting the dew point temperature
                    *h = 100.0 * (e / es);  // relative humidity [%]
                    if *h > 100.0 {  // clip to 100%
                        *h = 100.0;
                    }
                    // compute the vapor pressure deficit
                    *v = es - e;  // difference between saturation vapor pressure and actual vapor pressure [hPa]
                    if *v < 0.0 {  // clip to 0
                        *v = 0.0;
                    }
                }
            });
            replace(&mut data, &h, |i| &mut i.humidity);  // replace the humidity values [%]
            replace(&mut data, &vpd, |i| &mut i.vpd);  // save vapor pressure deficit [hPa]
        
        } else {
            // if the dew point temperature is not available, you need the relative humidity
            // or you need to compute it from specific humidity and surface pressure

            if let Some(h) = humidity {
                // there is the relative humidity data

                // compute the temperature dew point from the temperature and relative humidity
                let mut td: Array1<f32> = Array1::ones(len) * NODATAVAL;  // °C
                let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;  // hPa
                azip!((
                    r in &mut td,  // °C
                    v in &mut vpd,  // hPa
                    h in &h,  // %
                    t in &t  // °C
                ){
                    if *h > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                        let mut h = *h;
                        if h > 100.0 {  // clip to 100%
                            h = 100.0;
                        }
                        // compute dew point temperature from Magnus formula (https://en.wikipedia.org/wiki/Dew_point)
                        let gamma = f32::ln(h / 100.0) + ((17.625 * t) / (t + 243.04));
                        *r = (243.04 * gamma) / (17.625 - gamma);
                        // compute the vapor pressure deficit [hPa]
                        // August–Roche–Magnus formula > https://en.wikipedia.org/wiki/Clausius%E2%80%93Clapeyron_relation#August%E2%80%93Roche%E2%80%93Magnus_approximation
                        let es = 6.1094 * f32::exp((17.625 * t)/(t + 243.04));  // saturation vapor pressure [hPa]
                        // compute vapor pressure from relative humidity
                        let e = (h / 100.0) * es;  // vapor pressure [hPa]
                        // difference between saturation vapor pressure and actual vapor pressure [hPa]
                        *v = es - e;
                        if *v < 0.0 {  // clip to 0
                            *v = 0.0;
                        }
                    }
                });
                replace(&mut data, &td, |i| &mut i.temp_dew);
                replace(&mut data, &vpd, |i| &mut i.vpd);
              
            } else {
                // compute the relative humidity from specific humidity and surface pressure forecasted surface pressure

                // forecasted surface pressure
                let psfc = handler.get_values(PSFC, time); // supposed in Pa
                // forecasted specific humidity
                let q = handler.get_values(Q, time); // supposed in kg/kg

                if let (Some(psfc), Some(q)) = (psfc, q) {
                    // compute the relative humidity from the forecasted temperature, surface pressure and specific humidity
                    let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;  // %
                    let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;  // hPa
                    azip!((
                        h in &mut h,  // %
                        v in &mut vpd,  // hPa
                        q in &q, // kg/kg
                        p in &psfc, // Pa
                        t in &t // °C
                    ){
                        if *q > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) && *p > (NODATAVAL+1.0) {
                            // T_C=temperature in °C; P_hPa=pressure in hPa; Q2=specific humidity at 2m
                            // vapor pressure: e=(Q2*P_hPa/(0.622+0.378*Q2)) > https://cran.r-project.org/web/packages/humidity/vignettes/humidity-measures.html
                            // saturation vapor pressure: es=6.1094*exp((17.625*T_C)/(T_C+243.04)) > August–Roche–Magnus formula > https://en.wikipedia.org/wiki/Clausius%E2%80%93Clapeyron_relation#August%E2%80%93Roche%E2%80%93Magnus_approximation
                            // RH=(e/es)*100;
                            let e = q * (p/100.0) / (0.622 + 0.378*q);  // vapor pressure [hPa]
                            let es = 6.1094 * f32::exp((17.625 * t)/(t + 243.04));  // saturation vapor pressure [hPa]
                            *h = 100.0 * e / es;
                            if *h > 100.0 {
                                *h = 100.0;
                            }
                            // compute the vapor pressure deficit
                            *v = es - e;  // difference between saturation vapor pressure and actual vapor pressure [hPa]
                            if *v < 0.0 {  // clip to 0
                                *v = 0.0;
                            }
                        }
                    });
                    replace(&mut data, &h, |i| &mut i.humidity);
                    replace(&mut data, &vpd, |i| &mut i.vpd);
                
                    // compute the dew point temperature wiht the new computed relative humidity
                    let mut td: Array1<f32> = Array1::ones(len) * NODATAVAL;  // [°C]
                    azip!((
                        r in &mut td,  // °C
                        h in &h, // %
                        t in &t // °C
                    ){
                        if *h > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                            let mut h = *h;
                            if h > 100.0 {
                                h = 100.0;
                            }
                            // computed from Magnus formula (https://en.wikipedia.org/wiki/Dew_point)
                            let gamma = f32::ln(h / 100.0) + ((17.625 * t) / (t + 243.04));
                            *r = (243.04 * gamma) / (17.625 - gamma);
                        }
                    });
                    replace(&mut data, &td, |i| &mut i.temp_dew);           
                }
            }
        }
    }

    // wind speed and wind direction
    let ws = handler.get_values(W, time); // supposed in m/s
    let wd = handler.get_values(D, time); // supposed in degree with meteorological convenction (wind from, 0=from North)
    if let Some(ws) = ws {
        let ws = ws.mapv(|_ws| {
            if _ws <= (NODATAVAL + 1.0) {
                return NODATAVAL;
            } else {
                _ws * 3600.0 // conversion to m/h
            }
        });
        // save data
        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }
    if let Some(wd) = wd {
        let wd = wd.mapv(|_wd| {
            if _wd <= (NODATAVAL + 1.0) {
                return NODATAVAL;
            } else {
                _wd.to_radians().rem_euclid(2.0 * PI)  // conversion to rad, remap to [0, 2PI]
            }
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
                if *_u <= (NODATAVAL + 1.0) || *_v <= (NODATAVAL + 1.0) {
                    return NODATAVAL;
                }
                // compute wind speed
                f32::sqrt(_u * _u + _v * _v) * 3600.0 // conversion to m/h
            })
            .collect::<Array1<f32>>();
        // compute wind direction
        let wd = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u <= (NODATAVAL + 1.0) || *_v <= (NODATAVAL + 1.0) {
                    return NODATAVAL; // there is no data
                }
                // from https://confluence.ecmwf.int/pages/viewpage.action?pageId=133262398
                (PI + f32::atan2(*_u, *_v)).rem_euclid(2.0 * PI)  // rad
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

    // Forecasted snow cover depth
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
