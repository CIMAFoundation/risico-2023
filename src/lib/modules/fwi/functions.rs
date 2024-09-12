use core::time;

use chrono::{DateTime, Datelike, Utc};

use crate::models::{input::InputElement, output::OutputElement};

use super::{
    config::FWIModelConfig,
    constants::*,
    models::{FWIPropertiesElement, FWIStateElement},
};

// FFMC MODULE
pub fn from_ffmc_to_moisture(ffmc: f32) -> f32 {
    FFMC_S1 * (101.0 - ffmc) / (FFMC_S2 + ffmc)
}

pub fn from_moisture_to_ffmc(moisture: f32) -> f32 {
    FFMC_S3 * ((250.0 - moisture) / (FFMC_S1 + moisture))
}

pub fn moisture_rain_effect(moisture: f32, rain24: f32) -> f32 {
    let rain_eff: f32 = rain24 - FFMC_MIN_RAIN;
    let mut moisture_new: f32 = moisture
        + FFMC_R1
            * rain_eff
            * f32::exp(-100.0 / (251.0 - moisture))
            * (1.0 - f32::exp(-FFMC_R2 / rain_eff));
    // sovra-saturtion conditions
    if moisture > FFMC_NORMAL_COND {
        moisture_new += FFMC_R3
                * f32::powf(moisture - FFMC_NORMAL_COND, FFMC_R4)
                * f32::powf(rain_eff, FFMC_R5);
    }
    // limit moisture to [0, 250]
    moisture_new = f32::max(0.0, f32::min(250.0, moisture_new));
    moisture_new
}

pub fn update_moisture(moisture: f32, rain24: f32, hum: f32, temp: f32, w_speed: f32) -> f32 {
    // conversion from m/h into km/h - required by the FFMC formula
    let ws: f32 = w_speed / 1000.0;
    let mut moisture_new: f32 = moisture;
    if rain24 > FFMC_MIN_RAIN {
        // rain24 effect
        moisture_new = moisture_rain_effect(moisture, rain24);
    }
    // no-rain conditions
    let emc_dry: f32 = FFMC_A1D * f32::powf(hum, FFMC_A2D)
        + FFMC_A3D * f32::exp((hum - 100.0) / 10.0)
        + FFMC_A4D * (21.1 - temp) * (1.0 - f32::exp(-FFMC_A5D * hum));
    let emc_wet: f32 = FFMC_A1W * f32::powf(hum, FFMC_A2W)
        + FFMC_A3W * f32::exp((hum - 100.0) / 10.0)
        + FFMC_A4W * (21.1 - temp) * (1.0 - f32::exp(-FFMC_A5W * hum));
    // EMC_dry > EMC_wet
    if moisture_new > emc_dry {
        // drying process
        let k0_dry: f32 = FFMC_B1 * (1.0 - f32::powf(hum / 100.0, FFMC_B2))
            + FFMC_B3 * f32::powf(ws, FFMC_B4) * (1.0 - f32::powf(hum / 100.0, FFMC_B5));
        let k_dry: f32 = FFMC_B6 * k0_dry * f32::exp(FFMC_B7 * temp);
        moisture_new = emc_dry + (moisture_new - emc_dry) * f32::powf(10.0, -k_dry);
    } else if moisture_new < emc_wet {
        // wetting process
        let k0_wet: f32 = FFMC_B1 * (1.0 - f32::powf((100.0 - hum) / 100.0, FFMC_B2))
            + FFMC_B3
                * f32::powf(ws, FFMC_B4)
                * (1.0 - f32::powf((100.0 - hum) / 100.0, FFMC_B5));
        let k_wet: f32 = FFMC_B6 * k0_wet * f32::exp(FFMC_B7 * temp);
        moisture_new = emc_wet + (moisture_new - emc_wet) * f32::powf(10.0, -k_wet);
    }
    // limit moisture to [0, 250]
    moisture_new = f32::max(0.0, f32::min(250.0, moisture_new));
    moisture_new
}

// DMC MODULE
fn get_dmc_param(date: &DateTime<Utc>, latitude: f32) -> f32 {
    if latitude >= 0.0 {
        // North emisphere
        match date.month() {
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
        }
    } else {
        // South emisphere
        match date.month() {
            1 => 12.4,
            2 => 10.9,
            3 => 9.4,
            4 => 8.0,
            5 => 7.0,
            6 => 6.0,
            7 => 6.5,
            8 => 7.5,
            9 => 9.0,
            10 => 12.8,
            11 => 13.9,
            12 => 13.9,
            _ => 0.0,
        }
    }
}

pub fn dmc_rain_effect(dmc: f32, rain24: f32) -> f32 {
    let re: f32 = DMC_R1 * rain24 - DMC_R2;
    let b: f32 = if dmc <= DMC_A1 {
        100.0 / (DMC_R3 + DMC_R4 * dmc)
    } else if dmc > DMC_A2 {
        DMC_R7 * f32::ln(dmc) - DMC_R8
    } else {
        //in between
        DMC_R5 - DMC_R6 * f32::ln(dmc)
    };
    let m0: f32 = DMC_R9 + f32::exp(-(dmc - DMC_R10) / DMC_R11);
    let mr: f32 = m0 + 1000.0 * (re / (DMC_R12 + b * re));
    let mut dmc_new: f32 = DMC_R10 - DMC_R11 * f32::ln(mr - DMC_R9);
    // clip to positive values
    if dmc_new < 0.0 {
        dmc_new = 0.0;
    }
    dmc_new
}

pub fn update_dmc(dmc: f32, rain24: f32, temp: f32, hum: f32, l_e: f32) -> f32 {
    let mut dmc_new: f32 = dmc;
    if rain24 > DMC_MIN_RAIN {
        // rain effect
        dmc_new = dmc_rain_effect(dmc, rain24);
    }
    if temp >= DMC_MIN_TEMP {
        // temperature effect
        let k: f32 = DMC_T1 * (temp + DMC_T2) * (100.0 - hum) * l_e * 10e-6;
        dmc_new += 100.0 * k;
    }
    // clip to positive values
    if dmc_new < 0.0 {
        dmc_new = 0.0;
    }
    dmc_new
}

// DC MODULE
fn get_dc_param(date: &DateTime<Utc>, latitude: f32) -> f32 {
    if latitude >= 0.0 {
        // North emisphere
        match date.month() {
            1 => 1.6,
            2 => 1.6,
            3 => 1.6,
            4 => 0.9,
            5 => 3.8,
            6 => 5.8,
            7 => 6.4,
            8 => 5.0,
            9 => 2.4,
            10 => 0.4,
            11 => 1.6,
            12 => 1.6,
            _ => 0.0,
        }
    } else {
        // South emisphere
        match date.month() {
            1 => 6.4,
            2 => 5.0,
            3 => 2.4,
            4 => 0.4,
            5 => 1.6,
            6 => 1.6,
            7 => 1.6,
            8 => 1.6,
            9 => 1.6,
            10 => 0.9,
            11 => 3.8,
            12 => 5.8,
            _ => 0.0,
        }
    }
}

pub fn dc_rain_effect(dc: f32, rain24: f32) -> f32 {
    let rd: f32 = DC_R1 * rain24 - DC_R2;
    let q0: f32 = DC_R3 * f32::exp(-dc / DC_R4);
    let qr: f32 = q0 + DC_R5 * rd;
    let dc_new: f32 = DC_R4 * f32::ln(DC_R3 / qr);
    dc_new
}

pub fn update_dc(dc: f32, rain24: f32, temp: f32, l_f: f32) -> f32 {
    let mut dc_new = dc;
    if rain24 > DC_MIN_RAIN {
        // rain effect
        dc_new = dc_rain_effect(dc, rain24);
    }
    let v: f32 = DC_T1 * (temp + DC_T2) + l_f;
    if v > DC_MIN_TEMP {
        // temperature effect
        dc_new += DC_T3 * v;
    }
    // clip to positive values
    if dc_new < 0.0 {
        dc_new = 0.0;
    }
    dc_new
}

// ISI MODULE
pub fn compute_isi(ffmc: f32, w_speed: f32) -> f32 {
    // conversion from m/h into km/h - required by the ISI formula
    let ws: f32 = w_speed / 1000.0;
    let moisture: f32 = from_ffmc_to_moisture(ffmc);
    let fw: f32 = f32::exp(ISI_A0 * ws);
    let ff: f32 =
        ISI_A1 * f32::exp(ISI_A2 * moisture) * (1.0 + f32::powf(moisture, ISI_A3) / ISI_A4 * 10e7);
    let isi: f32 = ISI_A5 * fw * ff;
    isi
}

// BUI MODULE
pub fn compute_bui(dmc: f32, dc: f32) -> f32 {
    let bui: f32 = if dmc == 0.0 {
        0.0
    } else if dmc <= BUI_A1 * dc {
        BUI_A2 * ((dmc * dc) / (dmc + BUI_A1 * dc))
    } else {
        dmc - (1.0 - BUI_A2 * (dc / (dmc + BUI_A1 * dc)))
            * (BUI_A3 + f32::powf(BUI_A4 * dmc, BUI_A5))
    };
    bui
}

// FWI MODULE
pub fn compute_fwi(bui: f32, isi: f32) -> f32 {
    let fd: f32 = if bui <= 80.0 {
        FWI_A1 * f32::powf(bui, FWI_A2) + FWI_A3
    } else {
        1000.0 / (FWI_A4 + FWI_A5 * f32::exp(FWI_A6 * bui))
    };
    let b: f32 = 0.1 * isi * fd;
    let mut fwi: f32 = if b > 1.0 {
        f32::exp(FWI_A7 * f32::powf(FWI_A8 * f32::ln(b), FWI_A9))
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
    let mut ifwi: f32 = 0.0;
    if fwi > 1.0 {
        ifwi = (f32::exp(IFWI_A1 * f32::powf(f32::ln(fwi), IFWI_A2))) / IFWI_A3;
    }
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

    // native time step of the model
    let time_step: i64 = 24; // hours

    if rain == NODATAVAL
        || humidity == NODATAVAL
        || temperature == NODATAVAL
        || wind_speed == NODATAVAL
    {
        // keep current humidity state if we don't have all the data

        let mut ffmc_history_new: Vec<(DateTime<Utc>, f32)> = state.ffmc_history.clone();
        let mut dmc_history_new: Vec<(DateTime<Utc>, f32)> = state.dmc_history.clone();
        let mut dc_history_new: Vec<(DateTime<Utc>, f32)> = state.dc_history.clone();

        let last_ffmc = ffmc_history_new.last().map(|(_, h)| *h).unwrap_or(FFMC_INIT);
        let last_dmc = dmc_history_new.last().map(|(_, h)| *h).unwrap_or(DMC_INIT);
        let last_dc = dc_history_new.last().map(|(_, h)| *h).unwrap_or(DC_INIT);

        ffmc_history_new.push((*time, last_ffmc));
        dmc_history_new.push((*time, last_dmc));
        dc_history_new.push((*time, last_dc));

        // keep only the last 24 hours
        let ffmc_history: Vec<(DateTime<Utc>, f32)> = ffmc_history_new
            .iter()
            .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
            .map(|(t, f)| (*t, *f))
            .collect();
        let dmc_history: Vec<(DateTime<Utc>, f32)> = dmc_history_new
            .iter()
            .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
            .map(|(t, d)| (*t, *d))
            .collect();
        let dc_history: Vec<(DateTime<Utc>, f32)> = dc_history_new
            .iter()
            .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
            .map(|(t, d)| (*t, *d))
            .collect();

        state.ffmc_history = ffmc_history;
        state.dmc_history = dmc_history;
        state.dc_history = dc_history;

        return;
    }

    // update rain history
    let mut rain_history_new: Vec<(DateTime<Utc>, f32)> = state.rain_history.clone();
    rain_history_new.push((*time, rain));
    // keep only the last 24 hours
    let rain_history: Vec<(DateTime<Utc>, f32)> = rain_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
        .map(|(t, r)| (*t, *r))
        .collect();
    // compute total rain in the last 24 hours
    let rain24: f32 = rain_history.iter().map(|(_, r)| r).sum();
    state.rain_history = rain_history;

    // get FFMC 24 hours ago
    let mut ffmc_history_new: Vec<(DateTime<Utc>, f32)> = state.ffmc_history.clone();
    let first_ffmc: f32 = ffmc_history_new.first().map(|(_, f)| *f).unwrap_or(FFMC_INIT);
    let ffmc_24hours_ago: f32 = ffmc_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() == time_step)
        .map(|(_, f)| *f)
        .last().unwrap_or(first_ffmc);

    // get DMC 24 hours ago
    let mut dmc_history_new: Vec<(DateTime<Utc>, f32)> = state.dmc_history.clone();
    let first_dmc: f32 = dmc_history_new.first().map(|(_, d)| *d).unwrap_or(DMC_INIT);
    let dmc_24hours_ago: f32 = dmc_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() == time_step)
        .map(|(_, d)| *d)
        .last().unwrap_or(first_dmc);

    // get DC 24 hours ago
    let mut dc_history_new: Vec<(DateTime<Utc>, f32)> = state.dc_history.clone();
    let first_dc: f32 = dc_history_new.first().map(|(_, d)| *d).unwrap_or(DC_INIT);
    let dc_24hours_ago: f32 = dc_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() == time_step)
        .map(|(_, d)| *d)
        .last().unwrap_or(first_dc);

    // FFMC MODULE
    // convert ffmc to moisture scale [0, 250]
    let mut moisture: f32 = from_ffmc_to_moisture(ffmc_24hours_ago);
    moisture = config.moisture(moisture, rain24, humidity, temperature, wind_speed);
    // convert to ffmc scale and update state
    let new_ffmc = from_moisture_to_ffmc(moisture);

    // DMC MODULE
    let l_e = get_dmc_param(time, props.lat);
    let new_dmc = config.dmc(dmc_24hours_ago, rain24, temperature, humidity, l_e);

    // DC MODULE
    let l_f = get_dc_param(time, props.lat);
    let new_dc = config.dc(dc_24hours_ago, rain24, temperature, l_f);

    // update history of states
    ffmc_history_new.push((*time, new_ffmc));
    dmc_history_new.push((*time, new_dmc));
    dc_history_new.push((*time, new_dc));
    // keep only the last 24 hours
    let ffmc_history: Vec<(DateTime<Utc>, f32)> = ffmc_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
        .map(|(t, f)| (*t, *f))
        .collect();
    let dmc_history: Vec<(DateTime<Utc>, f32)> = dmc_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
        .map(|(t, d)| (*t, *d))
        .collect();
    let dc_history: Vec<(DateTime<Utc>, f32)> = dc_history_new
        .iter()
        .filter(|(t, _)| time.signed_duration_since(*t).num_hours() <= time_step)
        .map(|(t, d)| (*t, *d))
        .collect();

    state.ffmc_history = ffmc_history;
    state.dmc_history = dmc_history;
    state.dc_history = dc_history;
}

// COMPUTE OUTPUTS
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &FWIStateElement,
    input: &InputElement,
    config: &FWIModelConfig,
) -> OutputElement {
    // let rain = input.rain;
    // the rain information to save in output is the total rain in the last 24 hours
    let rain24 = state.rain_history.iter().map(|(_, r)| r).sum();
    let humidity = input.humidity;
    let temperature = input.temperature;
    let wind_speed = input.wind_speed;

    let ffmc_last = state.ffmc_history.last().map(|(_, f)| *f).unwrap_or(FFMC_INIT);
    let dmc_last = state.dmc_history.last().map(|(_, d)| *d).unwrap_or(DMC_INIT);
    let dc_last = state.dc_history.last().map(|(_, d)| *d).unwrap_or(DC_INIT);

    // compute fine fuel moisture in [0, 100]
    let moisture = from_ffmc_to_moisture(ffmc_last);
    let dffm_last = (moisture / (100.0 + moisture)) * moisture;

    let isi = config.isi(ffmc_last, wind_speed);
    let bui = config.bui(dmc_last, dc_last);
    let fwi = config.fwi(isi, bui);

    let ifwi = compute_ifwi(fwi);

    OutputElement {
        ffmc: ffmc_last,
        dffm: dffm_last,
        dmc: dmc_last,
        dc: dc_last,
        isi,
        bui,
        fwi,
        ifwi,
        rain: rain24,
        humidity,
        temperature,
        wind_speed,
        ..OutputElement::default()
    }
}
