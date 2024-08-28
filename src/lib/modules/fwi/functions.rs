use ndarray::{Array, Array1};
use std::f32::consts::PI;

///functions to work on the state of the risico model
use chrono::{DateTime, Datelike, Utc};

use super::{
    config::ModelConfig,
    constants::*,
    models::{InputElement, OutputElement, PropertiesElement, StateElement},
};

// FFMC MODULE
pub fn from_ffmc_to_moisture(ffmc: f32) -> f32 {
    FFMC_S1*(101-ffmc)/(FFMC_S2+ffmc)
}

pub fn from_moisture_to_ffmc(moisture: f32) -> f32 {
    FFMC_S3*((250-moisture)/(FFMC_S1+moisture))
}

pub fn ffmc_rain_effect(moisture: f32, rain24: f32) -> f32 {
    let mut moisture_new: f32 = NODATAVAL;
    let rain_eff: f32 = rain24-FFMC_MIN_RAIN;
    if rain24 <= FFMC_MIN_RAIN {
        moisture_new = moisture;
    } else {  
        moisture_new = moisture + FFMC_R1*rain_eff*f32::exp(-100/(251-moisture))*(1-f32::exp(-FFMC_R2/rain_eff));
    }
    // extra-humid conditions
    if moisture > FFMC_NORMAL_COND {
        moisture_new = moisture_new + FFMC_R3*f32::powf(moisture-FFMC_NORMAL_COND, FFMC_R4)*f32::powf(rain_eff, FFMC_R5);
    }
    moisture_new
}

pub fn update_ffmc(ffmc: f32, rain24: f32, hum: f32, temp: f32, w_speed: f32) -> f32 {
    // convert to moisture scale
    let moisture: f32 = from_ffmc_to_moisture(ffmc);
    // rain24 effect
    let mut moisture_new: f32 = ffmc_rain_effect(moisture, rain24);
    // dry conditions
    let emc_dry: f32 = FFMC_A1d*f32::powf(hum, FFMC_A2d) + FFMC_A3d*f32::exp((hum-100.0)/FFMC_A4d) + FFMC_A5d*(21.1-temp)*(1.0-f32::exp(-FFMC_A6d*hum));
    let emc_wet: f32 = FFMC_A1w*f32::powf(hum, FFMC_A2w) + FFMC_A3w*f32::exp((hum-100.0)/FFMC_A4w) + FFMC_A5w*(21.1-temp)*(1.0-f32::exp(-FFMC_A6w*hum));
    if moisture_new < emc_dry {
        let k0_dry: f32 = FFMC_B1*(1-f32::powf(hum/100.0, FFMC_B2)) + FFMC_B3*f32::powf(w_speed, FFMC_B4)*(1.0-f32::powf(hum/100.0, FFMC_B5));
        let k_dry: f32 = FFMC_B6*k0_dry*f32::exp(FFMC_B7*temp);
        moisture_new = emc_dry + (moisture_new-emc_dry)*f32::powf(10.0, -k_dry);
    } else if moisture_new > emc_wet {
        let k0_wet: f32 = FFMC_B1*(1-f32::powf((100.0-hum)/100.0, FFMC_B2)) + FFMC_B3*f32::powf(w_speed, FFMC_B4)*(1.0-f32::powf((100.0-hum)/100.0, FFMC_B5));
        let k_wet: f32 = FFMC_B6*k0_wet*f32::exp(FFMC_B7*temp);
        moisture_new = emc_wet + (moisture_new-emc_wet)*f32::powf(10.0, -k_wet);
    }
    // convert to ffmc scale
    let ffmc_new: f32 = from_moisture_to_ffmc(moisture_new);
    ffmc_new
}

// DMC MODULE
pub fn dmc_rain_effect(dmc: f32, rain24: f32) -> f32 {
    let mut dmc_new: f32 = NODATAVAL;
    if rain24 < DMC_MIN_RAIN {
        dmc_new = dmc;
    } else {
        let re: f32 = DMC_R1*rain24 - DMC_R2;
        let mut b: f32 = NODATAVAL;
        if dmc <= DMC_A1 {
            b = 100.0/(DMC_R3+DMC_R4*dmc);
        } else if dmc > DMC_A2 {
            b = DMC_R7*f32::log(dmc)-DMC_R8;
        } else {
            b = DMC_R5-DMC_R6*f32::log(dmc);
        }
        let m0: f32 = DMC_R9 + f32::exp(-(dmc-DMC_R10)/DMC_R11);
        let mr: f32 = m0 + 1000.0*(re/(DMC_R12+b*re));
        dmc_new = DMC_R10 - DMC_R11*f32::log(mr-DMC_R9);
        if dmc_new < 0 {
            dmc_new = 0;
        }
    }
    dmc_new
} 
 
pub fn update_dmc(dmc: f32, rain24: f32, temp: f32, hum: f32, l_e: f32) -> f32 {
    // rain effect
    let mut dmc_new: f32 = dmc_rain_effect(dmc, rain24);
    // temperature effect
    if temp >= DMC_MIN_TEMP {
        let k: f32 = DMC_T3*DMC_T1*(temp+DMC_T2)*(100.0-hum)*l_e;
        dmc_new = dmc_new + 100.0*k;
    }
    if dmc_new < 0 {
        dmc_new = 0;
    }
    dmc_new
}

// DC MODULE
pub fn dc_rain_effect(dc: f32, rain24: f32) -> f32 {
    let mut dc_new = NODATAVAL;
    if rain24 < DC_MIN_RAIN {
        dc_new = dc;
    } else {
        rd = DC_R1*rain24-DC_R2;
        let q0: f32 = DC_R3*f32::exp(-dc/DC_R4);
        let qr: f32 = q0+DC_R5*rd;
        dc_new = DC_R4*f32::log(DC_R3/qr);
    }
    dc_new
}

pub fn update_dc(dc: f32, rain24: f32, temp: f32, Lf: f32) -> f32 {
    let mut dc_new = dc_rain_effect(dc, rain24);
    if temp >= DC_MIN_TEMP {
        dc_new = dc_new + DC_T3*(DC_T1*(temp+DC_T2)+Lf);
    }
    if dc_new < 0 {
        dc_new = 0;
    }
    dc_new
}

// ISI MODULE
pub fn update_isi(ffmc: f32, w_speed: f32) -> f32 {
    let moisture: f32 = from_ffmc_to_moisture(ffmc);
    let fw: f32 = f32::exp(ISI_A0*w_speed);
    let ff: f32 = ISI_A1*f32::exp(ISI_A2*moisture)*(1+f32::powf(moisture, ISI_A3)/ISI_A4*10e7);
    let isi: f32 = ISI_A5*fw*ff;
    isi
}

// BUI MODULE
pub fn update_bui(dmc: f32, dc: f32) -> f32 {
    let mut bui: f32 = NODATAVAL;
    if dmc == 0 {
        bui = 0;
    } else if dmc <= BUI_A1*dc {
        bui = BUI_A2*((dmc*dc)/(dmc+BUI_A1*dc));
    } else {
        bui = dmc - (1-BUI_A2*(dc/(dmc+BUI_A1*dc)))*(BUI_A3+f32::powf(BUI_A4*dmc, BUI_A5));
    }
    bui
}

// FWI MODULE
pub fn compute_fwi(bui: f32, isi: f32) -> f32 {
    let fd: f32 = NODATAVAL;
    if bui <= 80 {
        fd = FWI_A1*f32::powf(bui, FWI_A2)+FWI_A3;
    } else {
        fd = 1000.0 / (FWI_A4+FWI_A5*f32::exp(FWI_A6*bui));
    }
    let b: f32 = 0.1*isi*fd;
    let fwi: f32 = NODATAVAL;
    if b > 1 {
        fwi = f32::exp(FWI_A7*f32::powf(FWI_A8*f32::log(b), FWI_A9));
    } else {
        fwi = b;
    }
    if fwi < 0 {
        fwi = 0;
    }
    fwi
}

// COMPUTE INDICES
#[allow(non_snake_case)]
pub fn update_moisture_fn(
    state: &mut StateElement,
    props: &PropertiesElement,
    input_data: &InputElement,
    config: &ModelConfig
) {
    let temperature = input_data.temperature;
    let humidity = input_data.humidity;
    let wind_speed = input_data.wind_speed;
    let rain = input_data.rain;

    if temperature == NODATAVAL || humidity == NODATAVAL {
        // keep current humidity if we don't have all the data
        return;
    }

    // METTI QUALCHE CHECK 

    state.ffmc = config.ffmc_fn(state.ffmc, rain, humidity, temperature, wind_speed);
    // continua con le altre variabili moisture
}