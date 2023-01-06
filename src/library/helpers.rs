use std::f32::consts::PI;

use chrono::{DateTime, Utc};
use itertools::izip;
use ndarray::{Array1, azip};

use super::{config::models::InputDataHandler, state::{constants::NODATAVAL, models::Input}};

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

pub fn get_input(
    handler: &InputDataHandler,
    time: &DateTime<Utc>,
    len: usize,
) -> Input {
    let mut snow_cover: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut precipitation: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut temperature: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut wind_speed: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut wind_dir: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut humidity: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut ndvi: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut ndwi: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut ndsi: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut swi: Array1<f32> = Array1::ones(len) * NODATAVAL;
    let mut msi: Array1<f32> = Array1::ones(len) * NODATAVAL;

    let snow = handler.get_values("SNOW", &time);

    maybe_replace(&mut snow_cover, &snow);

    // Observed relative humidity
    let h = handler.get_values("F", &time);
    maybe_replace(&mut humidity, &h);

    // forecasted relative humidity
    let h = handler.get_values("H", &time);
    maybe_replace(&mut humidity, &h);

    // Observed temperature
    let t = handler.get_values("K", &time);

    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });
        replace(&mut temperature, &t);
    }

    // Forecasted temperature
    let t = handler.get_values("T", &time);

    if let Some(t) = t {
        let t = t.mapv(|_t| if _t > 200.0 { _t - 273.15 } else { _t });
        replace(&mut temperature, &t);
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
            replace(&mut humidity, &h);
        }
    }

    // Observed precipitation
    let op = handler.get_values("O", &time);
    maybe_replace(&mut precipitation, &op);
    // Forecast precipitation
    let fp = handler.get_values("P", &time);
    maybe_replace(&mut precipitation, &fp);

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
        replace(&mut wind_speed, &ws);
    }

    if let Some(wd) = wd {
        let wd = wd.mapv(|_wd| {
            let mut _wd = _wd / 180.0 * PI;
            if _wd < 0.0 {
                _wd += PI * 2.0;
            }
            _wd
        });
        replace(&mut wind_dir, &wd);
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

        replace(&mut wind_dir, &wd);
        replace(&mut wind_speed, &ws);
    }

    let _swi = handler.get_values("SWI", &time);
    maybe_replace(&mut swi, &_swi);

    let _ndsi = handler.get_values("N", &time);
    maybe_replace(&mut ndsi, &_ndsi);

    let _ndvi = handler.get_values("NDVI", &time);
    maybe_replace(&mut ndvi, &_ndvi);

    let _ndwi = handler.get_values("NDWI", &time);
    maybe_replace(&mut ndwi, &_ndwi);

    let _msi = handler.get_values("M", &time);
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
        ndsi: ndsi,
        swi: swi,
        msi: msi,
    }
}