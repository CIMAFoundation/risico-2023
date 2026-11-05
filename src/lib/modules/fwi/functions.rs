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

    // get initial moisture values > 24 hours ago, or default value
    let combined_moisture_init = izip!(
        state.dates.iter(),
        state.ffmc.iter(),
        state.dmc.iter(),
        state.dc.iter(),
    )
    .filter(|(t, _, _, _)| time.signed_duration_since(**t) == chrono::Duration::hours(TIME_WINDOW.into()))
    .map(|(t, ffmc, dmc, dc)| (*t, *ffmc, *dmc, *dc))
    .collect::<Vec<_>>();

    let ffmc_init = combined_moisture_init.iter().map(|(_, ffmc, _, _)| *ffmc).collect::<Vec<_>>().first().copied().unwrap_or(FFMC_INIT);
    let dmc_init = combined_moisture_init.iter().map(|(_, _, dmc, _)| *dmc).collect::<Vec<_>>().first().copied().unwrap_or(DMC_INIT);
    let dc_init = combined_moisture_init.iter().map(|(_, _, _, dc)| *dc).collect::<Vec<_>>().first().copied().unwrap_or(DC_INIT);

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

// MODEL VERSION "cffdrs": canonical daily FWI System at local standard noon
//
// The equations are in `cffdrs.rs`. This part turns the (hourly or 3-hourly)
// forecast steps into the daily observations the system expects:
// - the daily step runs at local standard noon (time zone of the cell from
//   tzf-rs, standard offset without daylight saving time, i.e. 13:00 local
//   summer time), when the input step that reaches or passes that instant is
//   processed: it no longer depends on when outputs are written;
// - noon temperature, humidity and wind are interpolated linearly between the
//   two input steps bracketing noon (the later one alone when the earlier one
//   is unknown, e.g. right after a warm start);
// - the rain of the daily step is the rain accumulated since the previous
//   noon, the bracketing step being split linearly, so that every millimetre
//   is counted once; the accumulated rain is the `rain` of the warm state, the
//   same quantity as the `r0` column of the FWI12 state files.
//
// State vectors hold one element: `dates[0]` is the time of the last input
// step, `ffmc/dmc/dc[0]` the codes after the last daily step, `rain[0]` the
// rain accumulated since that step, `temperature/humidity/wind_speed[0]` the
// weather of the last input step, `rain24h[0]` the rain used by the last
// daily step. `noon_*` hold the noon weather of the last daily step.

fn valid(value: f32) -> bool {
    !value.is_nan() && value > NODATAVAL + 1.0
}

/// Standard (non-DST) UTC offset in hours of the time zone at (lon, lat):
/// tzf-rs boundaries, IANA base offset from chrono-tz 0.10 (`base_utc_offset`,
/// so Europe/Dublin and Africa/Casablanca count as UTC+1 as in the tz
/// database); falls back to the nautical zone `round(lon / 15)` if the zone
/// is unknown.
pub fn standard_utc_offset_hours(lon: f32, lat: f32, date: &DateTime<Utc>) -> f32 {
    use chrono_tz_current::OffsetComponents;
    let tz_name = TZ_FINDER.get_tz_name(lon as f64, lat as f64);
    match tz_name.parse::<chrono_tz_current::Tz>() {
        Ok(tz) => {
            let offset = tz.offset_from_utc_datetime(&date.naive_utc());
            offset.base_utc_offset().num_seconds() as f32 / 3600.0
        }
        Err(_) => (lon / 15.0).round(),
    }
}

/// Local standard noons (UTC instants) in the interval (from, to], oldest first.
fn local_standard_noons(from: DateTime<Utc>, to: DateTime<Utc>, offset_hours: f32) -> Vec<DateTime<Utc>> {
    let offset = chrono::Duration::seconds((offset_hours as f64 * 3600.0).round() as i64);
    let local_to = (to + offset).naive_utc();
    let mut noon = DateTime::<Utc>::from_naive_utc_and_offset(
        local_to.date().and_hms_opt(12, 0, 0).expect("valid noon"),
        Utc,
    ) - offset;
    if noon > to {
        noon -= chrono::Duration::days(1);
    }
    let mut noons = Vec::new();
    while noon > from {
        noons.push(noon);
        noon -= chrono::Duration::days(1);
    }
    noons.reverse();
    noons
}

fn interpolate_or_nearest(previous: f32, current: f32, weight: f64) -> f32 {
    match (valid(previous), valid(current)) {
        (true, true) => (previous as f64 + weight * (current as f64 - previous as f64)) as f32,
        (false, true) => current,
        (true, false) => previous,
        (false, false) => NODATAVAL,
    }
}

pub fn update_state_cffdrs(
    state: &mut FWIStateElement,
    prop: &FWIPropertiesElement,
    input: &InputElement,
    time: &DateTime<Utc>,
) {
    let last = |values: &Vec<f32>| values.last().copied().filter(|v| valid(*v));
    let previous_time = state.dates.last().copied();
    if previous_time.is_some_and(|previous| *time <= previous) {
        // already processed (input steps before the warm state)
        return;
    }
    let mut ffmc = last(&state.ffmc).unwrap_or(FFMC_INIT) as f64;
    let mut dmc = last(&state.dmc).unwrap_or(DMC_INIT) as f64;
    let mut dc = last(&state.dc).unwrap_or(DC_INIT) as f64;
    let mut accumulated = last(&state.rain).unwrap_or(0.0).max(0.0) as f64;
    let mut rain24h = state.rain24h.last().copied().unwrap_or(NODATAVAL);
    let previous_weather = (
        last(&state.temperature).unwrap_or(NODATAVAL),
        last(&state.humidity).unwrap_or(NODATAVAL),
        last(&state.wind_speed).unwrap_or(NODATAVAL),
    );
    let rain_in = if valid(input.rain) { input.rain.max(0.0) as f64 } else { 0.0 };

    if let Some(previous) = previous_time {
        let offset = if prop.std_utc_offset_hours.is_finite() {
            prop.std_utc_offset_hours
        } else {
            (prop.lon / 15.0).round()
        };
        let interval = (*time - previous).num_seconds() as f64;
        let mut cut = previous;
        for noon in local_standard_noons(previous, *time, offset) {
            let weight = (noon - previous).num_seconds() as f64 / interval;
            let portion = rain_in * (noon - cut).num_seconds() as f64 / interval;
            cut = noon;
            // rounded to 0.001 mm so that float noise in the accumulation cannot
            // decide the rain thresholds of the codes (0.5, 1.5, 2.8 mm)
            let rain_day = ((accumulated + portion) * 1000.0).round() / 1000.0;
            accumulated = 0.0;
            let temperature = interpolate_or_nearest(previous_weather.0, input.temperature, weight);
            let humidity = interpolate_or_nearest(previous_weather.1, input.humidity, weight);
            let wind_speed = interpolate_or_nearest(previous_weather.2, input.wind_speed, weight);
            if !(valid(temperature) && valid(humidity) && valid(wind_speed)) {
                // no weather for this noon: codes unchanged, the day's rain is dropped
                continue;
            }
            let offset_duration = chrono::Duration::seconds((offset as f64 * 3600.0).round() as i64);
            let month = (noon + offset_duration).month();
            (ffmc, dmc, dc) = super::cffdrs::daily_codes(
                ffmc,
                dmc,
                dc,
                temperature as f64,
                humidity as f64,
                wind_speed as f64 / 1000.0, // m/h -> km/h
                rain_day,
                prop.lat as f64,
                month,
            );
            rain24h = rain_day as f32;
            state.noon_temperature = temperature;
            state.noon_humidity = humidity;
            state.noon_wind_speed = wind_speed;
        }
        accumulated += rain_in * (*time - cut).num_seconds() as f64 / interval;
    } else {
        // cold start without a warm-state time: start accumulating from here
        accumulated += rain_in;
    }

    state.dates = vec![*time];
    state.ffmc = vec![ffmc as f32];
    state.dmc = vec![dmc as f32];
    state.dc = vec![dc as f32];
    state.rain = vec![accumulated as f32];
    state.temperature = vec![input.temperature];
    state.humidity = vec![input.humidity];
    state.wind_speed = vec![input.wind_speed];
    state.rain24h = vec![rain24h];
}

pub fn get_output_cffdrs(
    state: &mut FWIStateElement,
    _prop: &FWIPropertiesElement,
    _time: &DateTime<Utc>,
) -> OutputElement {
    use super::cffdrs;
    let last = |values: &Vec<f32>, default: f32| values.last().copied().filter(|v| valid(*v)).unwrap_or(default);
    let ffmc = last(&state.ffmc, FFMC_INIT);
    let dmc = last(&state.dmc, DMC_INIT);
    let dc = last(&state.dc, DC_INIT);
    let moisture = cffdrs::ffmc_to_moisture(ffmc as f64);
    let bui = cffdrs::buildup_index(dmc as f64, dc as f64);
    let (isi, fwi, ifwi) = if valid(state.noon_wind_speed) {
        let isi = cffdrs::initial_spread_index(ffmc as f64, state.noon_wind_speed as f64 / 1000.0);
        let fwi = cffdrs::fire_weather_index(isi, bui);
        (isi as f32, fwi as f32, compute_ifwi(fwi as f32))
    } else {
        (NODATAVAL, NODATAVAL, NODATAVAL)
    };
    OutputElement {
        ffmc,
        dffm: (moisture / (100.0 + moisture) * 100.0) as f32,
        dmc,
        dc,
        isi,
        bui: bui as f32,
        fwi,
        ifwi,
        rain: state.rain24h.last().copied().unwrap_or(NODATAVAL),
        humidity: state.noon_humidity,
        temperature: state.noon_temperature,
        wind_speed: if valid(state.noon_wind_speed) {
            state.noon_wind_speed / 3600.0
        } else {
            NODATAVAL
        },
        ..OutputElement::default()
    }
}

#[cfg(test)]
mod cffdrs_step_tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn state_at(time: &str, ffmc: f32, dmc: f32, dc: f32, rain: f32) -> FWIStateElement {
        FWIStateElement {
            dates: vec![utc(time)],
            ffmc: vec![ffmc],
            dmc: vec![dmc],
            dc: vec![dc],
            rain: vec![rain],
            humidity: vec![NODATAVAL],
            temperature: vec![NODATAVAL],
            wind_speed: vec![NODATAVAL],
            rain24h: vec![NODATAVAL],
            noon_temperature: NODATAVAL,
            noon_humidity: NODATAVAL,
            noon_wind_speed: NODATAVAL,
        }
    }

    fn prop(lat: f32, offset: f32) -> FWIPropertiesElement {
        FWIPropertiesElement { lon: offset * 15.0, lat, std_utc_offset_hours: offset }
    }

    fn input(temperature: f32, humidity: f32, wind_kmh: f32, rain: f32) -> InputElement {
        InputElement { temperature, humidity, wind_speed: wind_kmh * 1000.0, rain, ..InputElement::default() }
    }

    #[test]
    fn noons_in_half_open_interval() {
        // UTC+4: local standard noon is 08:00 UTC
        let n = local_standard_noons(utc("2026-11-03T21:00:00Z"), utc("2026-11-04T08:00:00Z"), 4.0);
        assert_eq!(n, vec![utc("2026-11-04T08:00:00Z")]);
        let n = local_standard_noons(utc("2026-11-04T08:00:00Z"), utc("2026-11-05T07:00:00Z"), 4.0);
        assert!(n.is_empty(), "the start of the interval is excluded");
        let n = local_standard_noons(utc("2026-11-03T00:00:00Z"), utc("2026-11-05T12:00:00Z"), -5.0);
        assert_eq!(n, vec![utc("2026-11-03T17:00:00Z"), utc("2026-11-04T17:00:00Z")]);
        // UTC+13: noon at 23:00 UTC of the previous day
        let n = local_standard_noons(utc("2026-11-03T23:00:00Z"), utc("2026-11-04T02:00:00Z"), 13.0);
        assert!(n.is_empty());
        let n = local_standard_noons(utc("2026-11-03T22:00:00Z"), utc("2026-11-04T02:00:00Z"), 13.0);
        assert_eq!(n, vec![utc("2026-11-03T23:00:00Z")]);
    }

    #[test]
    fn daily_step_at_noon_uses_noon_weather_and_24h_rain() {
        // 3-hourly steps, noon at 12 UTC (offset 0); warm state at 00 UTC with 1 mm accumulated
        let p = prop(45.0, 0.0);
        let mut s = state_at("2026-07-01T00:00:00Z", 85.0, 6.0, 15.0, 1.0);
        for (h, t) in [(3, 15.0), (6, 18.0), (9, 21.0), (12, 24.0)] {
            update_state_cffdrs(&mut s, &p, &input(t, 40.0, 20.0, 3.0), &utc(&format!("2026-07-01T{h:02}:00:00Z")));
        }
        let expected = super::super::cffdrs::daily_codes(85.0, 6.0, 15.0, 24.0, 40.0, 20.0, 13.0, 45.0, 7);
        assert_eq!(s.rain24h[0], 13.0, "1 mm carried + 4 steps of 3 mm");
        assert_eq!(s.rain[0], 0.0);
        assert!((s.ffmc[0] as f64 - expected.0).abs() < 1e-4);
        assert!((s.dmc[0] as f64 - expected.1).abs() < 1e-4);
        assert!((s.dc[0] as f64 - expected.2).abs() < 1e-4);
        assert_eq!(s.noon_temperature, 24.0);
        // no further noon before 15 UTC: codes unchanged, rain accumulates
        let ffmc = s.ffmc[0];
        update_state_cffdrs(&mut s, &p, &input(25.0, 40.0, 20.0, 2.0), &utc("2026-07-01T15:00:00Z"));
        assert_eq!(s.ffmc[0], ffmc);
        assert_eq!(s.rain[0], 2.0);
    }

    #[test]
    fn noon_inside_a_step_splits_rain_and_interpolates() {
        // offset +1.5 h: noon at 10:30 UTC, inside the 09-12 step
        let p = prop(45.0, 1.5);
        let mut s = state_at("2026-07-01T09:00:00Z", 85.0, 6.0, 15.0, 0.25);
        s.temperature = vec![20.0];
        s.humidity = vec![50.0];
        s.wind_speed = vec![10_000.0];
        update_state_cffdrs(&mut s, &p, &input(30.0, 30.0, 30.0, 3.0), &utc("2026-07-01T12:00:00Z"));
        assert_eq!(s.rain24h[0], 1.75, "0.25 carried + half of the 3 mm step");
        assert!((s.rain[0] - 1.5).abs() < 1e-6, "the other half is carried to the next day");
        assert!((s.noon_temperature - 25.0).abs() < 1e-5);
        assert!((s.noon_humidity - 40.0).abs() < 1e-5);
        assert!((s.noon_wind_speed - 20_000.0).abs() < 1e-2);
        let expected = super::super::cffdrs::daily_codes(85.0, 6.0, 15.0, 25.0, 40.0, 20.0, 1.75, 45.0, 7);
        assert!((s.dmc[0] as f64 - expected.1).abs() < 1e-4);
    }

    #[test]
    fn steps_before_the_warm_state_are_ignored_and_restart_is_exact() {
        let p = prop(42.0, 4.0);
        let steps: Vec<(DateTime<Utc>, InputElement)> = (1..=24)
            .map(|k| {
                let t = utc("2026-11-03T00:00:00Z") + chrono::Duration::hours(3 * k);
                (t, input(5.0 + (k % 8) as f32, 60.0 + (k % 5) as f32 * 5.0, 12.0, if k % 3 == 0 { 1.2 } else { 0.0 }))
            })
            .collect();
        let mut continuous = state_at("2026-11-03T00:00:00Z", 80.0, 20.0, 200.0, 0.0);
        for (t, i) in &steps {
            update_state_cffdrs(&mut continuous, &p, i, t);
        }
        // restart from a snapshot taken after step 8 (scalars only, as the warm state stores them)
        let mut first = state_at("2026-11-03T00:00:00Z", 80.0, 20.0, 200.0, 0.0);
        for (t, i) in &steps[..8] {
            update_state_cffdrs(&mut first, &p, i, t);
        }
        let mut restarted = state_at(
            &first.dates[0].to_rfc3339(), first.ffmc[0], first.dmc[0], first.dc[0], first.rain[0],
        );
        for (t, i) in &steps {
            update_state_cffdrs(&mut restarted, &p, i, t); // the first 8 are skipped
        }
        assert_eq!(restarted.ffmc, continuous.ffmc);
        assert_eq!(restarted.dmc, continuous.dmc);
        assert_eq!(restarted.dc, continuous.dc);
        assert_eq!(restarted.rain, continuous.rain);
    }

    #[test]
    fn missing_noon_weather_keeps_codes() {
        let p = prop(45.0, 0.0);
        let mut s = state_at("2026-07-01T09:00:00Z", 85.0, 6.0, 15.0, 0.0);
        update_state_cffdrs(&mut s, &p, &input(NODATAVAL, 40.0, 20.0, 1.0), &utc("2026-07-01T12:00:00Z"));
        assert_eq!((s.ffmc[0], s.dmc[0], s.dc[0]), (85.0, 6.0, 15.0));
        let out = get_output_cffdrs(&mut s, &p, &utc("2026-07-01T12:00:00Z"));
        assert_eq!(out.fwi, NODATAVAL);
    }
}
