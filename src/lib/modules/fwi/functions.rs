use chrono::{DateTime, Datelike, Utc};
use itertools::izip;

use crate::models::{input::InputElement, output::OutputElement};

use super::{
    config::FWIModelConfig,
    constants::*,
    models::{FWIPropertiesElement, FWIStateElement},
};

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

// UPDATE STATES
#[allow(non_snake_case)]
pub fn update_state_fn(
    state: &mut FWIStateElement,
    props: &FWIPropertiesElement,
    input: &InputElement,
    time: &DateTime<Utc>,
    config: &FWIModelConfig,
) {
    let rain = input.rain;
    let humidity = input.humidity;
    let temperature = input.temperature;
    let wind_speed = input.wind_speed;

    if rain == NODATAVAL
        || humidity == NODATAVAL
        || temperature == NODATAVAL
        || wind_speed == NODATAVAL
    {
        // keep current humidity state if we don't have all the data
        let last_ffmc = state.ffmc.iter().copied().last().unwrap_or(FFMC_INIT);
        let last_dmc = state.dmc.iter().copied().last().unwrap_or(DMC_INIT);
        let last_dc = state.dc.iter().copied().last().unwrap_or(DC_INIT);
        let rain_nan = f32::NAN; // add NaN to rain history
                                 // update state
        state.update(time, last_ffmc, last_dmc, last_dc, rain_nan);
        return;
    }

    // add last rain in input, get 24 hours of rain and aggregate
    let (mut dates, _, _, _, mut history_rain) = state.get_time_window(time);
    dates.push(*time);
    history_rain.push(rain);
    let rain24 = izip!(dates.iter(), history_rain.iter())
        .filter(|(t, _)| time.signed_duration_since(**t).num_hours() < TIME_WINDOW)
        .filter(|(_, r)| !r.is_nan())
        .map(|(_, r)| *r)
        .sum();

    // get moisture values to start computation - initial moisture values on the time window
    let (ffmc_24h_ago, dmc_24h_ago, dc_24h_ago) = state.get_initial_moisture(time);

    // FFMC MODULE
    // convert ffmc to moisture scale [0, 250]
    let mut moisture: f32 = from_ffmc_to_moisture(ffmc_24h_ago);
    moisture = config.moisture(moisture, rain24, humidity, temperature, wind_speed);
    // convert to ffmc scale and update state
    let new_ffmc = from_moisture_to_ffmc(moisture);

    // DMC MODULE
    let l_e = get_dmc_param(time, props.lat);
    let new_dmc = config.dmc(dmc_24h_ago, rain24, temperature, humidity, l_e);

    // DC MODULE
    let l_f = get_dc_param(time, props.lat);
    let new_dc = config.dc(dc_24h_ago, rain24, temperature, l_f);

    // update history of states
    state.update(time, new_ffmc, new_dmc, new_dc, rain);
}

// COMPUTE OUTPUTS
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &FWIStateElement,
    input: &InputElement,
    config: &FWIModelConfig,
) -> OutputElement {
    // let rain = input.rain;  // DEPRECATED
    // the rain information to save in output is the total rain in the state time window
    let rain_tot: f32 = state.rain.iter().filter(|&r| !r.is_nan()).sum();

    let humidity = input.humidity;
    let temperature = input.temperature;
    let wind_speed = input.wind_speed;

    // get last moisture values to save in output
    let ffmc_last = state.ffmc.iter().copied().last().unwrap_or(FFMC_INIT);
    let dmc_last = state.dmc.iter().copied().last().unwrap_or(DMC_INIT);
    let dc_last = state.dc.iter().copied().last().unwrap_or(DC_INIT);

    // compute fine fuel moisture in [0, 100]
    let moisture_last = from_ffmc_to_moisture(ffmc_last);
    let dffm_last = (moisture_last / (100.0 + moisture_last)) * 100.0;

    let isi = config.isi(moisture_last, wind_speed);
    let bui = config.bui(dmc_last, dc_last);
    let fwi = config.fwi(isi, bui);

    let ifwi = compute_ifwi(fwi);

    let wind_speed_out = wind_speed / 3600.0; // convert from m/h to m/s

    OutputElement {
        ffmc: ffmc_last,
        dffm: dffm_last,
        dmc: dmc_last,
        dc: dc_last,
        isi,
        bui,
        fwi,
        ifwi,
        rain: rain_tot,
        humidity,
        temperature,
        wind_speed: wind_speed_out,
        ..OutputElement::default()
    }
}
