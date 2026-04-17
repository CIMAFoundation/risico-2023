use chrono::{DateTime, Datelike, LocalResult, NaiveDate, Utc, TimeZone};
use itertools::izip;
use chrono_tz::Tz;
use lazy_static::lazy_static;
use tzf_rs::DefaultFinder;

use crate::models::{input::InputElement, output::OutputElement};

use super::{
    config::FWIModelConfig,
    constants::*,
    models::{FWIPropertiesElement, FWIStateElement},
};

lazy_static! {
    static ref TZ_FINDER: DefaultFinder = DefaultFinder::new();
}

// HELPER FUNCTIONS

// Lawson & Armitage latitude bands: (-90,-30], (-30,-10], (-10,10], (10,30], (30,90]
fn lat_band_la(latitude: f32) -> u8 {
    if latitude < -30.0 {
        1
    } else if latitude < -10.0 {
        2
    } else if latitude < 10.0 {
        3
    } else if latitude < 30.0 {
        4
    } else {
        5  // standard values -> Canada (Van Wagner 1987)
    }
}

// FWI INDICES MODULES

// FFMC MODULE
pub fn from_ffmc_to_moisture(ffmc: f32) -> f32 {
    147.2 * (101.0 - ffmc) / (59.4688 + ffmc)
}

pub fn from_moisture_to_ffmc(moisture: f32) -> f32 {
    59.5 * (250.0 - moisture) / (147.2 + moisture)
}

pub fn moisture_rain_effect(moisture: f32, rain24: f32) -> f32 {
    let rain_eff: f32 = rain24 - 0.5;
    let mut moisture_new: f32 = moisture
        + 42.5
            * (rain_eff
                * f32::exp(-100.0 / (251.0 - moisture))
                * (1.0 - f32::exp(-6.93 / rain_eff)));
    // sovra-saturtion conditions
    if moisture > 150.0 {
        moisture_new += 0.0015 * f32::powf(moisture - 150.0, 2.0) * f32::powf(rain_eff, 0.5);
    }
    // limit moisture to [0, 250]
    moisture_new.clamp(0.0, 250.0)
}

pub fn update_moisture(moisture: f32, rain24: f32, hum: f32, temp: f32, w_speed: f32) -> f32 {
    // conversion from m/h into km/h - required by the FFMC formula
    let ws: f32 = w_speed / 1000.0;
    let mut moisture_new: f32 = moisture;
    if rain24 > 0.5 {
        // rain24 effect
        moisture_new = moisture_rain_effect(moisture, rain24);
    }
    // no-rain conditions
    let emc_dry: f32 = 0.942 * f32::powf(hum, 0.679)
        + 11.0 * f32::exp((hum - 100.0) / 10.0)
        + 0.18 * (21.1 - temp) * (1.0 - f32::exp(-0.115 * hum));
    let emc_wet: f32 = 0.618 * f32::powf(hum, 0.753)
        + 10.0 * f32::exp((hum - 100.0) / 10.0)
        + 0.18 * (21.1 - temp) * (1.0 - f32::exp(-0.115 * hum));
    // EMC_dry > EMC_wet
    if moisture_new > emc_dry {
        // drying process
        let k0_dry: f32 = 0.424 * (1.0 - f32::powf(hum / 100.0, 1.7))
            + 0.0694 * f32::powf(ws, 0.5) * (1.0 - f32::powf(hum / 100.0, 8.0));
        let k_dry: f32 = 0.581 * k0_dry * f32::exp(0.0365 * temp);
        moisture_new = emc_dry + (moisture_new - emc_dry) * f32::powf(10.0, -k_dry);
    } else if moisture_new < emc_wet {
        // wetting process
        let k0_wet: f32 = 0.424 * (1.0 - f32::powf((100.0 - hum) / 100.0, 1.7))
            + 0.0694 * f32::powf(ws, 0.5) * (1.0 - f32::powf((100.0 - hum) / 100.0, 8.0));
        let k_wet: f32 = 0.581 * k0_wet * f32::exp(0.0365 * temp);
        moisture_new = emc_wet - (emc_wet - moisture_new) * f32::powf(10.0, -k_wet);
    }
    // limit moisture to [0, 250]
    moisture_new.clamp(0.0, 250.0)
}

// DMC MODULE

// Le (monthly day-length adjustment) from:
// Lawson, B.D. & Armitage, O.B., 2008. Weather guide for the Canadian Forest Fire Danger Rating System. Northern Forestry Centre, Edmonton (Canada).
// Defaults to latitude=46 if caller passes NaN > reference of Van Wagner 1987.
pub fn get_dmc_param(date: &DateTime<Utc>, latitude: f32) -> f32 {
    let lat = if latitude.is_nan() { 46.0 } else { latitude };
    let band = lat_band_la(lat);
    match band {
        1 => match date.month() {
            1 => 11.5,
            2 => 10.5,
            3 => 9.2, 
            4 => 7.9,
            5 => 6.8,
            6 => 6.2,
            7 => 6.5,
            8 => 7.4,
            9 => 8.7,
            10 => 10.0,
            11 => 11.2,
            12 => 11.8,
            _ => 0.0,
        },
        2 => match date.month() {
            1 => 10.1,
            2 => 9.6,
            3 => 9.1,
            4 => 8.5,
            5 => 8.1,
            6 => 7.8,
            7 => 7.9,
            8 => 8.3,
            9 => 8.9,
            10 => 9.4,
            11 => 9.9,
            12 => 10.2,
            _ => 0.0,
        },
        3 => 9.0,
        4 => match date.month() {
            1 => 7.9, 
            2 => 8.4,
            3 => 8.9,
            4 => 9.5,
            5 => 9.9,
            6 => 10.2,
            7 => 10.1,
            8 => 9.7,
            9 => 9.1,
            10 => 8.6,
            11 => 8.1,
            12 => 7.8,
            _ => 0.0,
        },
        _ => match date.month() {  // 5 and default
            1 => 6.5,
            2 => 7.5,
            3 => 9.0,
            4 => 12.8,
            5 => 13.9,
            6 => 13.9,
            7 => 12.4,
            8 => 10.9,
            9 => 9.4,
            10 => 8.0,
            11 => 7.0,
            12 => 6.0,
            _ => 0.0,
        },
    }
}

pub fn dmc_rain_effect(dmc: f32, rain24: f32) -> f32 {
    let re: f32 = 0.92 * rain24 - 1.27;
    let b: f32 = if dmc <= 33.0 {
        100.0 / (0.5 + 0.3 * dmc)
    } else if dmc > 65.0 {
        6.2 * f32::ln(dmc) - 17.2
    } else {
        //in between
        14.0 - 1.3 * f32::ln(dmc)
    };
    let m0: f32 = 20.0 + f32::exp(-(dmc - 244.72) / 43.43);
    let mr: f32 = m0 + 1000.0 * re / (48.77 + b * re);
    let mut dmc_new: f32 = 244.72 - 43.43 * f32::ln(mr - 20.0);
    // clip to positive values
    if dmc_new < 0.0 {
        dmc_new = 0.0;
    }
    dmc_new
}

pub fn update_dmc(dmc: f32, rain24: f32, temp: f32, hum: f32, l_e: f32) -> f32 {
    let mut dmc_new: f32 = dmc;
    if rain24 > 1.5 {
        // rain effect
        dmc_new = dmc_rain_effect(dmc, rain24);
    }
    if temp >= -1.1 {
        // temperature effect
        let k: f32 = 1.894 * (temp + 1.1) * (100.0 - hum) * l_e * 1e-6;
        dmc_new += 100.0 * k;
    }
    // clip to positive values
    if dmc_new < 0.0 {
        dmc_new = 0.0;
    }
    dmc_new
}

// DC MODULE

// Lf factor (monthly correction) from L&A tables.
// Lawson, B.D. & Armitage, O.B., 2008. Weather guide for the Canadian Forest Fire Danger Rating System. Northern Forestry Centre, Edmonton (Canada).
// Defaults to latitude=46 if caller passes NaN > reference of Van Wagner 1987.
pub fn get_dc_param(date: &DateTime<Utc>, latitude: f32) -> f32 {
    let lat = if latitude.is_nan() { 46.0 } else { latitude };
    let band = lat_band_la(lat);
    match band {
        1 | 2 => match date.month() {  // southern emisphere
            1 => 6.4,
            2 => 5.0,
            3 => 2.4,
            4 => 0.4,
            5 => -1.6,
            6 => -1.6,
            7 => -1.6,
            8 => -1.6,
            9 => -1.6,
            10 => 0.9,
            11 => 3.8,
            12 => 5.8,
            _ => 0.0,
        },
        3 => 1.4,
        4 | 5 => match date.month() {  // nothern emisphere
            1 => -1.6,
            2 => -1.6,
            3 => -1.6,
            4 => 0.9,
            5 => 3.8,
            6 => 5.8,
            7 => 6.4,
            8 => 5.0,
            9 => 2.4,
            10 => 0.4,
            11 => -1.6,
            12 => -1.6,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

pub fn dc_rain_effect(dc: f32, rain24: f32) -> f32 {
    let rd: f32 = 0.83 * rain24 - 1.27;
    let q0: f32 = 800.0 * f32::exp(-(dc / 400.0));
    let qr: f32 = q0 + 3.937 * rd;
    let dc_new: f32 = 400.0 * f32::ln(800.0 / qr);
    dc_new
}

pub fn update_dc(dc: f32, rain24: f32, temp: f32, l_f: f32) -> f32 {
    let mut dc_new = dc;
    if rain24 > 2.8 {
        // rain effect
        dc_new = dc_rain_effect(dc, rain24);
    }
    let v: f32 = 0.36 * (temp + 2.8) + l_f;
    if v > 0.0 {
        // temperature effect
        dc_new += 0.5 * v;
    }
    // clip to positive values
    if dc_new < 0.0 {
        dc_new = 0.0;
    }
    dc_new
}

// COMPUTE MOISTURE CODES   
pub fn compute_moisture_codes(
    ffmc_init: f32,
    dmc_init: f32,
    dc_init: f32,
    rain24h: f32,
    humidity: f32,
    temperature: f32,
    wind_speed: f32,
    time: &DateTime<Utc>,
    lat: f32
) -> (f32, f32, f32) {

    // managing nodataval > keep initial values
    if rain24h == NODATAVAL
        || humidity == NODATAVAL
        || temperature == NODATAVAL
        || wind_speed == NODATAVAL
    {
        return (ffmc_init, dmc_init, dc_init);
    }

    // FFMC MODULE
    // convert ffmc to moisture scale [0, 250]
    let mut moisture: f32 = from_ffmc_to_moisture(ffmc_init);
    moisture = update_moisture(moisture, rain24h, humidity, temperature, wind_speed);
    // convert to ffmc scale and update state
    let new_ffmc = from_moisture_to_ffmc(moisture);

    // DMC MODULE
    let l_e = get_dmc_param(time, lat);
    let new_dmc = update_dmc(dmc_init, rain24h, temperature, humidity, l_e);

    // DC MODULE
    let l_f = get_dc_param(time, lat);
    let new_dc = update_dc(dc_init, rain24h, temperature, l_f);
    (new_ffmc, new_dmc, new_dc)

}


// ISI MODULE
pub fn compute_isi(moisture: f32, w_speed: f32) -> f32 {
    // conversion from m/h into km/h - required by the ISI formula
    let ws: f32 = w_speed / 1000.0;
    let fw: f32 = if w_speed != NODATAVAL {
        f32::exp(0.05039 * ws)
    } else {
        1.0
    };
    let ff: f32 =
        91.9 * f32::exp(-0.1386 * moisture) * (1.0 + f32::powf(moisture, 5.31) / (4.93 * 1e7));
    let isi: f32 = 0.208 * fw * ff;
    isi
}

// BUI MODULE
pub fn compute_bui(dmc: f32, dc: f32) -> f32 {
    let mut bui: f32 = if dmc > 0.0 {
        if dmc <= (0.4 * dc) {
            0.8 * dmc * dc / (dmc + 0.4 * dc)
        } else {
            dmc - (1.0 - 0.8 * dc / (dmc + 0.4 * dc)) * (0.92 + f32::powf(0.0114 * dmc, 1.7))
        }
    } else {
        0.0
    };
    // clip to positive values
    if bui < 0.0 {
        bui = 0.0;
    }
    bui
}

// FWI MODULE
pub fn compute_fwi(bui: f32, isi: f32) -> f32 {
    let fd: f32 = if bui <= 80.0 {
        0.626 * f32::powf(bui, 0.809) + 2.0
    } else {
        1000.0 / (25.0 + 108.64 * f32::exp(-0.023 * bui))
    };
    let b: f32 = 0.1 * isi * fd;
    let mut fwi: f32 = if b > 1.0 {
        f32::exp(2.72 * f32::powf(0.434 * f32::ln(b), 0.647))
    } else {
        b
    };
    // clip to positive values
    if fwi < 0.0 {
        fwi = 0.0;
    }
    fwi
}

pub fn compute_ifwi(fwi: f32) -> f32 {
    let ifwi: f32 = if fwi > 1.0 {
        (f32::exp(0.98 * f32::powf(f32::ln(fwi), 1.546))) / 0.289
    } else {
        0.0
    };
    ifwi
}


// WEATHER NOON - HELPERS FUNCTIONS

pub fn get_weather_noon(
    state: &FWIStateElement,
    prop: &FWIPropertiesElement,
) -> Option<(f32, f32, f32, f32)> {
    // Find local timezone from coordinates
    let tz_name = TZ_FINDER.get_tz_name(prop.lon as f64, prop.lat as f64);
    let tz: Tz = tz_name.parse().ok()?;

    let n = state.dates.len();
    if n < 2
        || state.humidity.len() != n
        || state.temperature.len() != n
        || state.wind_speed.len() != n
        || state.rain24h.len() != n
    {
        return None;
    }

    // Find the latest pair of timestamps that brackets a local noon
    let (i0, i1, noon_utc) = find_local_noon_bracketing_pair(&state.dates, tz)?;

    let w = interpolation_weight(state.dates[i0], state.dates[i1], noon_utc)?;

    let rain24h = lerp_valid(state.rain24h[i0], state.rain24h[i1], w)?;
    let humidity = lerp_valid(state.humidity[i0], state.humidity[i1], w)?;
    let temperature = lerp_valid(state.temperature[i0], state.temperature[i1], w)?;
    let wind_speed = lerp_valid(state.wind_speed[i0], state.wind_speed[i1], w)?;

    Some((rain24h, humidity, temperature, wind_speed))
}

// helper functions for weather interpolation at local noon

fn find_local_noon_bracketing_pair(
    dates: &[DateTime<Utc>],
    tz: Tz,
) -> Option<(usize, usize, DateTime<Utc>)> {
    let mut best: Option<(usize, usize, DateTime<Utc>)> = None;

    for i in 0..dates.len().saturating_sub(1) {
        let t0 = dates[i];
        let t1 = dates[i + 1];

        if t1 <= t0 {
            continue;
        }

        let local0 = t0.with_timezone(&tz);
        let local1 = t1.with_timezone(&tz);

        // Check both endpoint local dates.
        // This handles intervals that cross local midnight.
        let candidate_dates = [local0.date_naive(), local1.date_naive()];

        for local_date in candidate_dates {
            if let Some(noon_utc) = local_noon_utc_for_date(tz, local_date) {
                if t0 <= noon_utc && noon_utc <= t1 {
                    best = Some((i, i + 1, noon_utc));
                }
            }
        }
    }

    best
}


fn local_noon_utc_for_date(tz: Tz, local_date: NaiveDate) -> Option<DateTime<Utc>> {
    let naive_noon = local_date.and_hms_opt(12, 0, 0)?;

    let local_noon = match tz.from_local_datetime(&naive_noon) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(dt1, dt2) => dt1.min(dt2),
        LocalResult::None => return None,
    };

    Some(local_noon.with_timezone(&Utc))
}

fn interpolation_weight(
    t0: DateTime<Utc>,
    t1: DateTime<Utc>,
    target: DateTime<Utc>,
) -> Option<f32> {
    let total_secs = (t1 - t0).num_seconds();
    if total_secs <= 0 {
        return None;
    }

    let elapsed_secs = (target - t0).num_seconds();
    Some(elapsed_secs as f32 / total_secs as f32)
}

fn lerp(a: f32, b: f32, w: f32) -> f32 {
    a + w * (b - a)
}

fn lerp_valid(a: f32, b: f32, w: f32) -> Option<f32> {
    if a.is_nan() || a==NODATAVAL || b.is_nan() || b==NODATAVAL {
        return None;
    }
    Some(lerp(a, b, w))
}


// UPDATE STATE FUNCTION

pub fn update_state_legacy(
    state: &mut FWIStateElement,
    _prop: &FWIPropertiesElement,
    input: &InputElement,
    time: &DateTime<Utc>
) {
    let rain_in = input.rain;
    let humidity_in = input.humidity;
    let temperature_in = input.temperature;
    let wind_speed_in = input.wind_speed;

    // get the last 24 hours conditions
    let combined = izip!(
        state.dates.iter(),
        state.rain.iter(),
        state.humidity.iter(),
        state.temperature.iter(),
        state.wind_speed.iter(),
        state.rain24h.iter()
    )
    .filter(|(t, _, _, _, _, _)| time.signed_duration_since(**t) < chrono::Duration::hours(TIME_WINDOW.into()))
    .map(|(t, r, h, temp , w, r24)| (*t, *r, *h, *temp, *w, *r24))
    .collect::<Vec<_>>();

    let mut dates: Vec<DateTime<Utc>> = combined.iter().map(|(t, _, _, _, _, _)| *t).collect();
    let mut rain: Vec<f32> = combined.iter().map(|(_, r, _, _, _, _)| *r).collect();
    let mut humidity: Vec<f32> = combined.iter().map(|(_, _, h, _, _, _)| *h).collect();
    let mut temperature: Vec<f32> = combined.iter().map(|(_, _, _, temp, _, _)| *temp).collect();
    let mut wind_speed: Vec<f32> = combined.iter().map(|(_, _, _, _, w, _)| *w).collect();
    let mut rain24h: Vec<f32> = combined.iter().map(|(_, _, _, _, _, r24)| *r24).collect();

    // add last weather input    
    dates.push(*time);
    rain.push(rain_in);
    humidity.push(humidity_in);
    temperature.push(temperature_in);
    wind_speed.push(wind_speed_in);

    // aggregate the last 24 hours of rain and add to state
    let rain24h_in = rain.iter().filter(|r| **r != NODATAVAL).map(|r| *r).sum();
    rain24h.push(rain24h_in);

    // update state
    state.dates = dates;
    state.rain = rain;
    state.humidity = humidity;
    state.temperature = temperature;
    state.wind_speed = wind_speed;
    state.rain24h = rain24h;
}


pub fn update_state_sliding(
    state: &mut FWIStateElement,
    prop: &FWIPropertiesElement,
    input: &InputElement,
    time: &DateTime<Utc>
) {
    // first get weather
    let rain_in = input.rain;
    let humidity_in = input.humidity;
    let temperature_in = input.temperature;
    let wind_speed_in = input.wind_speed;

    // get last 24 hours conditions
    let combined = izip!(
        state.dates.iter(),
        state.rain.iter(),
        state.humidity.iter(),
        state.temperature.iter(),
        state.wind_speed.iter(),
        state.rain24h.iter(),
        state.ffmc.iter(),
        state.dmc.iter(),
        state.dc.iter(),
    )
    .filter(|(t, _, _, _, _, _, _, _, _)| time.signed_duration_since(**t) < chrono::Duration::hours(TIME_WINDOW.into()))
    .map(|(t, r, h, temp, w, r24, ffmc, dmc, dc)| (*t, *r, *h, *temp, *w, *r24, *ffmc, *dmc, *dc))
    .collect::<Vec<_>>();

    let mut dates: Vec<DateTime<Utc>> = combined.iter().map(|(t, _, _, _, _, _, _, _, _)| *t).collect();
    let mut rain: Vec<f32> = combined.iter().map(|(_, r, _, _, _, _, _, _, _)| *r).collect();
    let mut humidity: Vec<f32> = combined.iter().map(|(_, _, h, _, _, _, _, _, _)| *h).collect();
    let mut temperature: Vec<f32> = combined.iter().map(|(_, _, _, temp, _, _, _, _, _)| *temp).collect();
    let mut wind_speed: Vec<f32> = combined.iter().map(|(_, _, _, _, w, _, _, _, _)| *w).collect();
    let mut rain24h: Vec<f32> = combined.iter().map(|(_, _, _, _, _, r24, _, _, _)| *r24).collect();
    let mut ffmc: Vec<f32> = combined.iter().map(|(_, _, _, _, _, _, ffmc, _, _)| *ffmc).collect();
    let mut dmc: Vec<f32> = combined.iter().map(|(_, _, _, _, _, _, _, dmc, _)| *dmc).collect();
    let mut dc: Vec<f32> = combined.iter().map(|(_, _, _, _,  _, _, _, _, dc)| *dc).collect();

    // add last rain in input and last time
    dates.push(*time);
    rain.push(rain_in);
    humidity.push(humidity_in);
    temperature.push(temperature_in);
    wind_speed.push(wind_speed_in);

    // aggregate the last 24 hours of rain and add to state
    let rain24h_in = rain.iter().filter(|r| **r != NODATAVAL).map(|r| *r).sum();
    rain24h.push(rain24h_in);

    // get initial moisture values > it is the first, so it is 24 hours ago
    let ffmc_init = *ffmc.first().unwrap_or(&FFMC_INIT);
    let dmc_init = *dmc.first().unwrap_or(&DMC_INIT);
    let dc_init = *dc.first().unwrap_or(&DC_INIT);

    // compute moisture
    let (new_ffmc, new_dmc, new_dc) = compute_moisture_codes(
        ffmc_init,
        dmc_init,
        dc_init,
        rain24h_in,
        humidity_in,
        temperature_in,
        wind_speed_in,
        time,
        prop.lat
    );

    // update moisture states
    ffmc.push(new_ffmc);
    dmc.push(new_dmc);
    dc.push(new_dc);

    // update state with filtered values
    state.dates = dates;
    state.rain = rain;
    state.humidity = humidity;
    state.temperature = temperature;
    state.wind_speed = wind_speed;
    state.rain24h = rain24h;
    state.ffmc = ffmc;
    state.dmc = dmc;
    state.dc = dc;
}


pub fn update_state_fn(
    state: &mut FWIStateElement,
    prop: &FWIPropertiesElement,
    input: &InputElement,
    time: &DateTime<Utc>,
    config: &FWIModelConfig
) {
    config.update_state(state, prop, input, time);
}


// COMPUTE OUTPUTS

#[allow(non_snake_case)]
pub fn get_output_legacy(
    state: &mut FWIStateElement,
    prop: &FWIPropertiesElement,
    time: &DateTime<Utc>,
) -> OutputElement {

    // get weather conditions at local noon
    let (rain24h, humidity, temperature, wind_speed) = get_weather_noon(state, prop).unwrap_or((NODATAVAL, NODATAVAL, NODATAVAL, NODATAVAL));

    // get initial moisture values > in legacy, state moisture values are composed by just one element
    let ffmc_init = *state.ffmc.first().unwrap_or(&FFMC_INIT);
    let dmc_init = *state.dmc.first().unwrap_or(&DMC_INIT);
    let dc_init = *state.dc.first().unwrap_or(&DC_INIT);

    // compute moisture
    let (new_ffmc, new_dmc, new_dc) = compute_moisture_codes(
        ffmc_init,
        dmc_init,
        dc_init,
        rain24h,
        humidity,
        temperature,
        wind_speed,
        time,
        prop.lat
    );

    // update moisture states > in legacy, state moisture values are composed by just one element
    state.ffmc = vec![new_ffmc];
    state.dmc = vec![new_dmc];
    state.dc = vec![new_dc];

    // compute other indices
    let new_moisture = from_ffmc_to_moisture(new_ffmc);

    let isi = compute_isi(new_moisture, wind_speed);
    let bui = compute_bui(new_dmc, new_dc);
    let fwi = compute_fwi(bui, isi);
    let ifwi = compute_ifwi(fwi);

    // compute other outputs information
    let dffm = (new_moisture / (100.0 + new_moisture)) * 100.0;  // moisture in [0, 100]
    let wind_speed_out = wind_speed / 3600.0; // convert from m/h to m/s

    OutputElement {
        ffmc: new_ffmc,
        dffm: dffm,
        dmc: new_dmc,
        dc: new_dc,
        isi,
        bui,
        fwi,
        ifwi,
        rain: rain24h,
        humidity,
        temperature,
        wind_speed: wind_speed_out,
        ..OutputElement::default()
    }
}


#[allow(non_snake_case)]
pub fn get_output_sliding(
    state: &mut FWIStateElement,
    _prop: &FWIPropertiesElement,
    _time: &DateTime<Utc>
) -> OutputElement {

    // get weather conditions > last ones
    let rain24h: f32 = state.rain24h.iter().copied().last().unwrap_or(NODATAVAL);
    let humidity = state.humidity.iter().copied().last().unwrap_or(NODATAVAL);
    let temperature = state.temperature.iter().copied().last().unwrap_or(NODATAVAL);
    let wind_speed = state.wind_speed.iter().copied().last().unwrap_or(NODATAVAL);

    // get moisture values > last ones
    let ffmc = state.ffmc.iter().copied().last().unwrap_or(FFMC_INIT);
    let dmc = state.dmc.iter().copied().last().unwrap_or(DMC_INIT);
    let dc = state.dc.iter().copied().last().unwrap_or(DC_INIT);

    // compute other indices
    let moisture = from_ffmc_to_moisture(ffmc);

    let isi = compute_isi(moisture, wind_speed);
    let bui = compute_bui(dmc, dc);
    let fwi = compute_fwi(bui, isi);
    let ifwi = compute_ifwi(fwi);

    // get other outputs information
    let dffm = (moisture / (100.0 + moisture)) * 100.0; // moisture in [0, 100]
    let wind_speed_out = wind_speed / 3600.0; // convert from m/h to m/s

    OutputElement {
        ffmc: ffmc,
        dffm: dffm,
        dmc: dmc,
        dc: dc,
        isi,
        bui,
        fwi,
        ifwi,
        rain: rain24h,
        humidity,
        temperature,
        wind_speed: wind_speed_out,
        ..OutputElement::default()
    }
}


pub fn get_output_fn(
    state: &mut FWIStateElement,
    prop: &FWIPropertiesElement,
    time: &DateTime<Utc>,
    config: &FWIModelConfig
) -> OutputElement{
    config.get_output(state, prop, time)
}