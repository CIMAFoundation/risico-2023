use std::{f32::consts::PI, fmt::Display};

use chrono::{DateTime, Utc};
use itertools::izip;

use ndarray::{azip, Array1, Zip};
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
    match src {
        Some(src) => replace(dst, src, fun),
        None => (),
    }
}


/// Get the input data from the input handler and dave in the Input struct
/// If the input data are not in the expected units, the function will convert them
pub fn get_input(handler: &dyn InputHandler, time: &DateTime<Utc>, len: usize) -> Input {
    let mut data: Array1<InputElement> = Array1::default(len);

    // Observed temperature
    let t = handler.get_values(TEMPERATURE_OBS, time);
    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });  // convert to Celsius
        replace(&mut data, &t, |i| &mut i.temperature);
    }

    // Forecasted temperature
    let t = handler.get_values(TEMPERATURE, time);
    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });  // conversion to Celsius
        replace(&mut data, &t, |i| &mut i.temperature);

        // Forecasted dew point temperature
        let r = handler.get_values(TEMP_DEW, time);
        if let Some(r) = r {
            // computation of the relative humidity from the forecasted temperature and dew point temperature
            let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;
            azip!((
                h in &mut h,
                r in &r,
                t in &t
            ){
                if *r > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) {
                    let mut r = *r;
                    if r > 200.0 {
                        r -= 273.15;
                    }
                    *h = 100.0*(f32::exp((17.67 * r)/(r + 243.5))/f32::exp((17.67 * t)/(t + 243.5)));
                }
            });
            replace(&mut data, &h, |i| &mut i.humidity);
            // save also the dew point temperature
            replace(&mut data, &r, |i| &mut i.temp_dew);
        } else {            
            // forecasted surface pressure
            let psfc = handler.get_values(PSFC, time);
            // forecasted specific humidity
            let q = handler.get_values(Q, time);

            if let Some(psfc) = psfc {
                if let Some(q) = q {
                    // compute the relative humidity from the forecasted temperature, surface pressure and specific humidity
                    let mut h: Array1<f32> = Array1::ones(len) * NODATAVAL;
                    azip!((
                        h in &mut h,
                        q in &q,
                        p in &psfc,
                        t in &t
                    ){
                        if *q > (NODATAVAL+1.0) && *t > (NODATAVAL+1.0) && *p > (NODATAVAL+1.0) {
                            // this implements the following cdo formula
                            // T_C=temperature in °C; P_hPa=pressure in hPa; denom=0.622+Q2; e=(Q2*P_hPa/denom); es=6.112*exp((17.67*T_C)/(T_C+243.5)); RH=(e/es)*100;
                            *h = 100.0 * (q * (p/100.0) / (0.622 + q)) / (6.112 * f32::exp((17.67 * t)/(t + 243.5)));
                        }
                    });
                    replace(&mut data, &h, |i| &mut i.humidity);

                    // compute the temperature dew point from the forecasted temperature and relative humidity
                    let mut r: Array1<f32> = Array1::ones(len) * NODATAVAL;
                    azip!((
                        r in &mut r,
                        h in &h,
                        t in &t
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
                    replace(&mut data, &r, |i| &mut i.temp_dew_point);

                    // compute the vapour pressure deficit
                    let mut vpd: Array1<f32> = Array1::ones(len) * NODATAVAL;
                    azip!((
                        vpd in &mut vpd,
                        t in &t,
                        q in &q2,
                        p in &psfc
                    ){
                        if *q > (NODATAVAL+1.0) && *p > (NODATAVAL+1.0) {
                            // difference between saturation vapor pressure and vapor pressure
                            *vpd = (6.112 * f32::exp((17.67 * t)/(t + 243.5))) - (q * (p/100.0) / (0.622 + q));
                        }
                    });
                    replace(&mut data, &vpd, |i| &mut i.vpd);
                }
            }
        }
    }

    // Observed relative humidity
    let h = handler.get_values(HUMIDITY_OBS, time);
    maybe_replace(&mut data, &h, |i| &mut i.humidity);
    
    // Forecasted relative humidity
    let h = handler.get_values(HUMIDITY, time);
    maybe_replace(&mut data, &h, |i| &mut i.humidity);
    

    // wind speed and wind direction
    let ws = handler.get_values(WIND_SPEED, time);  // supposed in m/s
    let wd = handler.get_values(WIND_DIR, time);  // supposed in degree
    if let Some(ws) = ws {
        let ws = ws.mapv(|_ws| {
            if _ws > -9998.0 {
                _ws * 3600.0  // conversion to m/h
            } else {
                NODATAVAL
            }
        });
        // save data
        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }
    if let Some(wd) = wd {
        let wd = wd.mapv(|_wd| {
            let mut _wd = _wd / 180.0 * PI;  // conversion to rad
            if _wd < 0.0 {
                _wd += PI * 2.0;
            }
            _wd
        });
        // save data
        replace(&mut data, &wd, |i| &mut i.wind_dir);
    }

    // U and V components of the wind
    let u = handler.get_values(U, time);  // supposed in m/s
    let v = handler.get_values(V, time);  // supposed in m/s
    if let (Some(u), Some(v)) = (u, v) {
        // compute wind speed
        let ws = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u < (NODATAVAL + 1.0) || *_v < (NODATAVAL + 1.0) {
                    return NODATAVAL;
                }

                f32::sqrt(_u * _u + _v * _v) * 3600.0  // conversion to m/h
            })
            .collect::<Array1<f32>>();
        // compute wind direction
        let wd = izip!(&u, &v)
            .map(|(_u, _v)| {
                if *_u < -9998.0 || *_v < -9998.0 {
                    return NODATAVAL;  // there is no data
                }
                let mut wd = f32::atan2(*_u, *_v);
                if wd < 0.0 {
                    wd += PI * 2.0;
                }
                wd
            })
            .collect::<Array1<f32>>();
        // save data
        replace(&mut data, &wd, |i| &mut i.wind_dir);
        replace(&mut data, &ws, |i| &mut i.wind_speed);
    }

    // Observed precipitation
    let op = handler.get_values(RAIN_OBS, time);  // supposed in mm
    maybe_replace(&mut data, &op, |i| &mut i.rain);

    // Forecast precipitation
    let fp = handler.get_values(RAIN, time);  // supposed in mm
    maybe_replace(&mut data, &fp, |i| &mut i.rain);

    // Forecasted snow cover
    let snow = handler.get_values(SNOW, time);  // supposed in mm
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
