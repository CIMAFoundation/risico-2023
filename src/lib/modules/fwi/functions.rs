use chrono::{DateTime, Datelike, Utc};

use super::{
    config::ModelConfig,
    constants::*,
    models::{PropertiesElement, InputElement, OutputElement, StateElement},
};

// FFMC MODULE
pub fn from_ffmc_to_moisture(ffmc: f32) -> f32 {
    FFMC_S1*(101.0-ffmc)/(FFMC_S2+ffmc)
}

pub fn from_moisture_to_ffmc(moisture: f32) -> f32 {
    FFMC_S3*((250.0-moisture)/(FFMC_S1+moisture))
}

pub fn moisture_rain_effect(moisture: f32, rain12: f32) -> f32 {
    let rain_eff: f32 = rain12-FFMC_MIN_RAIN;
    let mut moisture_new: f32 = moisture + FFMC_R1*rain_eff*f32::exp(-100.0/(251.0-moisture))*(1.0-f32::exp(-FFMC_R2/rain_eff));
    // sovra-saturtion conditions
    if moisture > FFMC_NORMAL_COND {
        moisture_new = moisture_new + FFMC_R3*f32::powf(moisture-FFMC_NORMAL_COND, FFMC_R4)*f32::powf(rain_eff, FFMC_R5);
    }
    // limit moisture to [0, 250]
    moisture_new = f32::max(0.0, f32::min(250.0, moisture_new));
    moisture_new
}

pub fn update_moisture(moisture: f32, rain12: f32, hum: f32, temp: f32, w_speed: f32) -> f32 {
    let mut moisture_new: f32 = moisture;
    if rain12 > FFMC_MIN_RAIN {
        // rain12 effect
        moisture_new = moisture_rain_effect(moisture, rain12);
    }
    // no-rain conditions
    let emc_dry: f32 = FFMC_A1D*f32::powf(hum, FFMC_A2D) + FFMC_A3D*f32::exp((hum-100.0)/10.0) + FFMC_A4D*(21.1-temp)*(1.0-f32::exp(-FFMC_A5D*hum));
    let emc_wet: f32 = FFMC_A1W*f32::powf(hum, FFMC_A2W) + FFMC_A3W*f32::exp((hum-100.0)/10.0) + FFMC_A4W*(21.1-temp)*(1.0-f32::exp(-FFMC_A5W*hum));
    // EMC_dry > EMC_wet
    if moisture_new > emc_dry {
        // drying process
        let k0_dry: f32 = FFMC_B1*(1.0-f32::powf(hum/100.0, FFMC_B2)) + FFMC_B3*f32::powf(w_speed, FFMC_B4)*(1.0-f32::powf(hum/100.0, FFMC_B5));
        let k_dry: f32 = FFMC_B6*k0_dry*f32::exp(FFMC_B7*temp);
        moisture_new = emc_dry + (moisture_new-emc_dry)*f32::powf(10.0, -k_dry);
    } else if moisture_new < emc_wet {
        // wetting process
        let k0_wet: f32 = FFMC_B1*(1.0-f32::powf((100.0-hum)/100.0, FFMC_B2)) + FFMC_B3*f32::powf(w_speed, FFMC_B4)*(1.0-f32::powf((100.0-hum)/100.0, FFMC_B5));
        let k_wet: f32 = FFMC_B6*k0_wet*f32::exp(FFMC_B7*temp);
        moisture_new = emc_wet + (moisture_new-emc_wet)*f32::powf(10.0, -k_wet);
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

pub fn dmc_rain_effect(dmc: f32, rain12: f32) -> f32 {
    let re: f32 = DMC_R1*rain12 - DMC_R2;
    let mut b: f32 = NODATAVAL;
    if dmc <= DMC_A1 {
        b = 100.0/(DMC_R3+DMC_R4*dmc);
    } else if dmc > DMC_A2 {
        b = DMC_R7*f32::ln(dmc)-DMC_R8;
    } else {  //in between
        b = DMC_R5-DMC_R6*f32::ln(dmc);
    }
    let m0: f32 = DMC_R9 + f32::exp(-(dmc-DMC_R10)/DMC_R11);
    let mr: f32 = m0 + 1000.0*(re/(DMC_R12+b*re));
    let mut dmc_new: f32 = DMC_R10 - DMC_R11*f32::ln(mr-DMC_R9);
    // clip to positive values
    if dmc_new < 0.0 {
        dmc_new = 0.0;
    }
    dmc_new
} 
 
pub fn update_dmc(dmc: f32, rain12: f32, temp: f32, hum: f32, l_e: f32) -> f32 {
    let mut dmc_new: f32 = dmc;
    if rain12 > DMC_MIN_RAIN {
        // rain effect
        dmc_new = dmc_rain_effect(dmc, rain12);
    }    
    if temp >= DMC_MIN_TEMP {
        // temperature effect
        let k: f32 = DMC_T1*(temp+DMC_T2)*(100.0-hum)*l_e*10e-6;
        dmc_new = dmc_new + 100.0*k;
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

pub fn dc_rain_effect(dc: f32, rain12: f32) -> f32 {
    let rd: f32 = DC_R1*rain12-DC_R2;
    let q0: f32 = DC_R3*f32::exp(-dc/DC_R4);
    let qr: f32 = q0+DC_R5*rd;
    let dc_new: f32 = DC_R4*f32::ln(DC_R3/qr);
    dc_new
}

pub fn update_dc(dc: f32, rain12: f32, temp: f32, l_f: f32) -> f32 {
    let mut dc_new = dc;
    if rain12 > DC_MIN_RAIN {
        // rain effect
        dc_new = dc_rain_effect(dc, rain12);
    }
    let v: f32 = DC_T1*(temp+DC_T2)+l_f;
    if v > DC_MIN_TEMP {
        // temperature effect
        dc_new = dc_new + DC_T3*v;
    }
    // clip to positive values
    if dc_new < 0.0 {
        dc_new = 0.0;
    }
    dc_new
}

// ISI MODULE
pub fn compute_isi(ffmc: f32, w_speed: f32) -> f32 {
    let moisture: f32 = from_ffmc_to_moisture(ffmc);
    let fw: f32 = f32::exp(ISI_A0*w_speed);
    let ff: f32 = ISI_A1*f32::exp(ISI_A2*moisture)*(1.0+f32::powf(moisture, ISI_A3)/ISI_A4*10e7);
    let isi: f32 = ISI_A5*fw*ff;
    isi
}

// BUI MODULE
pub fn compute_bui(dmc: f32, dc: f32) -> f32 {
    let mut bui: f32 = NODATAVAL;
    if dmc == 0.0 {
        bui = 0.0;
    } else if dmc <= BUI_A1*dc {
        bui = BUI_A2*((dmc*dc)/(dmc+BUI_A1*dc));
    } else {
        bui = dmc - (1.0-BUI_A2*(dc/(dmc+BUI_A1*dc)))*(BUI_A3+f32::powf(BUI_A4*dmc, BUI_A5));
    }
    bui
}

// FWI MODULE
pub fn compute_fwi(bui: f32, isi: f32) -> f32 {
    let mut fd: f32 = NODATAVAL;
    if bui <= 80.0 {
        fd = FWI_A1*f32::powf(bui, FWI_A2)+FWI_A3;
    } else {
        fd = 1000.0 / (FWI_A4+FWI_A5*f32::exp(FWI_A6*bui));
    }
    let b: f32 = 0.1*isi*fd;
    let mut fwi: f32 = NODATAVAL;
    if b > 1.0 {
        fwi = f32::exp(FWI_A7*f32::powf(FWI_A8*f32::ln(b), FWI_A9));
    } else {
        fwi = b;
    }
    // clip to positive values
    if fwi < 0.0 {
        fwi = 0.0;
    }
    fwi
}

// UPDATE STATES
#[allow(non_snake_case)]
pub fn update_state_fn(
    state: &mut StateElement,
    props: &PropertiesElement,
    input: &InputElement,
    time: &DateTime<Utc>,
    config: &ModelConfig
) {
    let rain = input.rain;
    let humidity = input.humidity;
    let temperature = input.temperature;
    let wind_speed = input.wind_speed;

    if rain == NODATAVAL || humidity == NODATAVAL || temperature == NODATAVAL || wind_speed == NODATAVAL{
        // keep current humidity state if we don't have all the data
        return;
    }

    // FFMC MODULE
    // convert ffmc to moisture scale
    let mut moisture: f32 = from_ffmc_to_moisture(state.ffmc);
    moisture = config.moisture(moisture, rain, humidity, temperature, wind_speed);
    // compute fuel moisture in percentage
    // let dfmc = (moisture/(100.0+moisture))*moisture;
    // convert to ffmc scale and update state
    state.ffmc = from_moisture_to_ffmc(moisture);

    let l_e = get_dmc_param(time, props.lat);
    state.dmc = config.dmc(state.dmc, rain, temperature, humidity, l_e);
    let l_f = get_dc_param(time, props.lat);
    state.dc = config.dc(state.dc, rain, temperature, l_f);
}

// COMPUTE OUTPUTS
#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &StateElement,
    input: &InputElement,
    config: &ModelConfig
) -> OutputElement {

    let rain = input.rain;
    let humidity = input.humidity;
    let temperature = input.temperature;
    let wind_speed = input.wind_speed;

    let ffmc = state.ffmc;
    let dmc = state.dmc;
    let dc = state.dc;

    let isi = config.isi(ffmc, wind_speed);
    let bui = config.bui(dmc, dc);
    let fwi = config.fwi(isi, bui);

    OutputElement {
        ffmc,
        dmc,
        dc,
        isi,
        bui,
        fwi,
        rain,
        humidity,
        temperature,
        wind_speed
    }
}