use std::f32::consts::PI;

use chrono::{DateTime, Utc};
use itertools::izip;
use ndarray::{azip, Array1, Zip};

use super::{
    config::models::InputDataHandler,
    state::{
        constants::NODATAVAL,
        models::{Input, InputElement},
    },
};

fn replace<'a>(
    dst: &'a mut Array1<InputElement>,
    src: &Array1<f32>,
    fun: fn(&'a mut InputElement) -> &'a mut f32,
) {
    Zip::from(dst).and(src).par_for_each(|dst, &src| {
        let dst = fun(dst);
        if *dst <= (NODATAVAL + 1.0) {
            *dst = src;
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

pub fn get_input(handler: &InputDataHandler, time: &DateTime<Utc>, len: usize) -> Input {
    let mut data: Array1<InputElement> = Array1::default(len);

    let snow = handler.get_values("SNOW", &time);

    maybe_replace(&mut data, &snow, |i| &mut i.snow_cover);

    // Observed relative humidity
    let h = handler.get_values("F", &time);
    maybe_replace(&mut data, &h, |i| &mut i.humidity);

    // forecasted relative humidity
    let h = handler.get_values("H", &time);
    maybe_replace(&mut data, &h, |i| &mut i.humidity);

    // Observed temperature
    let t = handler.get_values("K", &time);

    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });
        replace(&mut data, &t, |i| &mut i.temperature);
    }

    // Forecasted temperature
    let t = handler.get_values("T", &time);

    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });
        replace(&mut data, &t, |i| &mut i.temperature);
        // Forecasted dew point temperature
        let r = handler.get_values("R", &time);
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
    let op = handler.get_values("O", &time);
    maybe_replace(&mut data, &op, |i| &mut i.rain);
    // Forecast precipitation
    let fp = handler.get_values("P", &time);
    maybe_replace(&mut data, &fp, |i| &mut i.rain);

    // wind speed
    let ws = handler.get_values("W", &time);
    // wind direction
    let wd = handler.get_values("D", &time);

    let u = handler.get_values("U", &time);
    let v = handler.get_values("V", &time);

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

    let swi = handler.get_values("SWI", &time);
    maybe_replace(&mut data, &swi, |i| &mut i.swi);

    let ndvi = handler.get_values("NDVI", &time);
    maybe_replace(&mut data, &ndvi, |i| &mut i.ndvi);

    let ndwi = handler.get_values("NDWI", &time);
    maybe_replace(&mut data, &ndwi, |i| &mut i.ndwi);

    let msi = handler.get_values("M", &time);
    maybe_replace(&mut data, &msi, |i| &mut i.msi);

    Input {
        time: time.to_owned(),
        data,
    }
}
