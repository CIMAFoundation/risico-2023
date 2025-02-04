use ndarray::{Array, Array1};
use std::f32::consts::PI;

///functions to work on the state of the risico model
use chrono::{DateTime, Datelike, Utc};

use crate::{
    constants::NODATAVAL,
    models::{input::InputElement, output::OutputElement},
};

use super::{
    config::RISICOModelConfig,
    constants::*,
    models::{RISICOPropertiesElement, RISICOStateElement},
};


// ---------------- LEGACY - RISICO 2015 ---------------- //

///Get the new value for the dfmm when is raining (p>p*)
pub fn update_dffm_rain_legacy(r: f32, dffm: f32, sat: f32) -> f32 {
    let delta_dffm = r
        * R1_LEGACY
        * f32::exp(-R2_LEGACY / ((sat + 1.0) - dffm))
        * (1.0 - f32::exp(-R3_LEGACY / r));
    let dffm = dffm + delta_dffm;

    f32::min(dffm, sat)
}

#[allow(non_snake_case)]
///Get the new value for the dfmm when there is no rain (p<p*)
pub fn update_dffm_dry_legacy(
    dffm: f32,
    _sat: f32,
    T: f32,
    W: f32,
    H: f32,
    T0: f32,
    dT: f32,
) -> f32 {
    let EMC = A1_LEGACY * f32::powf(H, A2)
        + A3 * f32::exp((H - 100.0) / 10.0)
        + A4 * (30.0 - f32::min(T, 30.0)) * (1.0 - f32::exp(-A5 * H));
    let K1 = T0 / (1.0 + A6 * f32::powf(T, B1) + A7 * f32::powf(W, B2));

    // drying-wtting dynamic
    let dffm = EMC + (dffm - EMC) * f32::exp(-dT / K1);

    if dffm >= 0.0 {
        dffm
    } else {
        0.0
    }
}

///calculate PPF from the date and the two values
pub fn get_ppf(time: &DateTime<Utc>, ppf_summer: f32, ppf_winter: f32) -> f32 {
    const MARCH_31: u32 = 89;
    const APRIL_1: u32 = 90;
    const MAY_31: u32 = 150;
    const JUNE_1: u32 = 151;
    const SEPTEMBER_30: u32 = 272;
    const OCTOBER_1: u32 = 273;
    const NOVEMBER_30: u32 = 334;
    const DECEMBER_1: u32 = 335;

    if ppf_summer < 0.0 || ppf_winter < 0.0 {
        return 0.0;
    }
    let day_number: u32 = time.date_naive().ordinal();

    match day_number {
        1..=MARCH_31 => ppf_winter,
        APRIL_1..=MAY_31 => {
            let val: f32 = (day_number - (MARCH_31 + 1)) as f32 / (MAY_31 - MARCH_31) as f32;
            val * ppf_summer + (1.0 - val) * ppf_winter
        }
        JUNE_1..=SEPTEMBER_30 => ppf_summer,
        OCTOBER_1..=NOVEMBER_30 => {
            let val: f32 = 1.0
                - ((day_number - (SEPTEMBER_30 + 1)) as f32 / (NOVEMBER_30 - SEPTEMBER_30) as f32);
            val * ppf_summer + (1.0 - val) * ppf_winter
        }
        DECEMBER_1..=366 => ppf_winter,
        _ => panic!("Invalid day number"),
    }
}

///calculate the wind effect on fire propagation
pub fn get_wind_effect_legacy(wind_speed: f32, wind_dir: f32, slope: f32, aspect: f32) -> f32 {
    if wind_speed == NODATAVAL || wind_dir == NODATAVAL {
        return 1.0;
    }
    //wind speed effect
    let ws = (1.0 + DELTA1 * (DELTA2 + f32::tanh((wind_speed / DELTA3) - DELTA4)))
        * (1.0 - (wind_speed / DELTA5));
    let eta = wind_dir - aspect;
    //aspect contribution
    let mut n =
        1.0 + (slope / (PI / 2.0)) * (ws - 1.0) * f32::exp(-f32::powf(eta - PI, 2.0) / QEPSIX2);
    if n < 1.0 {
        n = 1.0;
    }
    ws / n
}

///calculate the slope effect on fire propagation
pub fn get_slope_effect_legacy(slope: f32) -> f32 {
    1.0 + LAMBDA * (slope / (PI / 2.0))
}

///calculate the moisture effect on fire propagation
pub fn get_moisture_effect_legacy(dffm: f32) -> f32 {
    f32::exp(-1.0 * f32::powf(dffm / 20.0, 2.0))
}

/// DEPRECATED
pub fn get_t_effect(t: f32) -> f32 {
    if t <= 0.0 {
        return 1.0;
    }
    f32::exp(t * 0.0171)
}


///calculate the rate of spread
#[allow(clippy::too_many_arguments)]
pub fn get_v_legacy(
    v0: f32,
    d0: f32,
    _d1: f32,
    snow_cover: f32,
    dffm: f32,
    slope: f32,
    aspect: f32,
    wind_speed: f32,
    wind_dir: f32,
    t_effect: f32,
) -> (f32, f32) {
    let w_effect: f32 = get_wind_effect_legacy(wind_speed, wind_dir, slope, aspect);
    if snow_cover > 0.0 || d0 == NODATAVAL {
        return (0.0, w_effect);
    }

    if dffm == NODATAVAL {
        return (0.0, w_effect);
    }

    let moist_eff: f32 = get_moisture_effect_legacy(dffm);
    let s_effect: f32 = get_slope_effect_legacy(slope);
    let ros = v0 * moist_eff * w_effect * s_effect * t_effect;
    (ros, w_effect)
}

///calculate the low heating value for the dead fine fuel
pub fn get_lhv_dff(hhv: f32, dffm: f32) -> f32 {
    hhv * (1.0 - (dffm / 100.0)) - Q * (dffm / 100.0)
}

///calculate the low heating value for the live fuel
pub fn get_lhv_l1(humidity: f32, msi: f32, hhv: f32) -> f32 {
    if humidity == NODATAVAL {
        return 0.0;
    }

    if (0.0..=1.0).contains(&msi) {
        let l1_msi = f32::max(20.0, humidity - (20.0 * msi));
        hhv * (1.0 - (l1_msi / 100.0)) - Q * (l1_msi / 100.0)
    } else {
        hhv * (1.0 - (humidity / 100.0)) - Q * (humidity / 100.0)
    }
}

///calculate the fire intensity
pub fn get_intensity(
    d0: f32,
    d1: f32,
    v: f32,
    relative_greenness: f32,
    lhv_dff: f32,
    lhv_l1: f32,
) -> f32 {
    let mut d0 = d0;
    let mut d1 = d1;

    if d1 == NODATAVAL {
        d1 = 0.0;
    }
    if d0 == NODATAVAL {
        d0 = 0.0;
    }

    if relative_greenness >= 0.0 {
        if d1 == 0.0 {
            return v * (lhv_dff * d0 * (1.0 - relative_greenness)) / 3600.0;
        }

        return v * (lhv_dff * d0 + lhv_l1 * (d1 * (1.0 - relative_greenness))) / 3600.0;
    }

    v * (lhv_dff * d0 + lhv_l1 * d1) / 3600.0
}

pub fn index_from_swi(dffm: f32, swi: f32) -> f32 {
    if swi <= 10.0 {
        return 0.0;
    };
    dffm
}


/// Get the Meteorological Index by using dffm and w_effect
pub fn get_meteo_index_legacy(dffm: f32, w_effect: f32) -> f32 {
    if dffm <= NODATAVAL || w_effect < 1.0 || w_effect == NODATAVAL {
        return NODATAVAL;
    };

    let col = if (0.0..=5.0).contains(&dffm) {
        0
    } else if dffm <= 12.0 && dffm > 5.0 {
        1
    } else if dffm <= 20.0 && dffm > 12.0 {
        2
    } else if dffm <= 30.0 && dffm > 20.0 {
        3
    } else if dffm <= 40.0 && dffm > 30.0 {
        4
    } else {
        5
    };

    let row = if (1.0..=1.5).contains(&w_effect) {
        0
    } else if w_effect > 1.5 && w_effect <= 1.8 {
        1
    } else if w_effect > 1.8 && w_effect <= 2.2 {
        2
    } else if w_effect > 2.2 && w_effect <= 2.5 {
        3
    } else {
        4
    };

    FWI_TABLE[col + row * 6]
}


// ---------------- v2023 ---------------- //


///Get the new value for the dfmm when is raining (p>p*)
pub fn update_dffm_rain(r: f32, dffm: f32, sat: f32) -> f32 {
    let delta_dffm = r * R1 * f32::exp(-R2 / ((sat + 1.0) - dffm)) * (1.0 - f32::exp(-R3 / r));
    let dffm = dffm + delta_dffm;

    f32::min(dffm, sat)
}

///Get the new value for the dfmm when there is no rain (p<p*)
#[allow(non_snake_case)]
pub fn update_dffm_dry(dffm: f32, _sat: f32, T: f32, W: f32, H: f32, T0: f32, dT: f32) -> f32 {
    let W = W / 3600.0; //wind is in m/h, should be in m/s

    let EMC = A1 * f32::powf(H, A2)
        + A3 * f32::exp((H - 100.0) / 10.0)
        + A4 * (30.0 - f32::min(T, 30.0)) * (1.0 - f32::exp(-A5 * H));

    let D_dry: f32 =
        (1.0 + B1_D * f32::powf(T_STANDARD, C1_D) + B2_D * f32::powf(W_STANDARD, C2_D))
            / (1.0 + B3_D * f32::powf(H_STANDARD, C3_D));
    let K_dry: f32 = T0
        * D_dry
        * ((1.0 + B3_D * f32::powf(H, C3_D))
            / (1.0 + B1_D * f32::powf(T, C1_D) + B2_D * f32::powf(W, C2_D)));

    let D_wet: f32 = (1.0 + B3_W * f32::powf(H_STANDARD, C3_W))
        / (1.0 + B1_W * f32::powf(T_STANDARD, C1_W) + B2_W * f32::powf(W_STANDARD, C2_W));
    let K_wet: f32 = T0
        * D_wet
        * ((1.0 + B1_W * f32::powf(T, C1_W) + B2_W * f32::powf(W, C2_W))
            / (1.0 + B3_W * f32::powf(H, C3_W)));

    let K: f32 = if dffm >= EMC { K_dry } else { K_wet };

    // drying-wtting dynamic
    let const_G: f32 = (EMC - dffm) / (100.0 - dffm);
    let dffm = (EMC - 100.0 * const_G * f32::exp(-dT / K)) / (1.0 - const_G * f32::exp(-dT / K));

    if dffm >= 0.0 {
        dffm
    } else {
        0.0
    }
}




/// Get the wind effect on the fire propagation at the desired angle
/// # Arguments
/// * `wind_speed` - Wind speed \[m/h\]
/// * `wind_dir` - Wind direction \[radians\]
/// * `angle` - Angle \[radians\]
/// # Returns
/// * `w_eff_on_dir` - Wind effect in angle direction \[adim\]
pub fn get_wind_effect_angle(wind_speed: f32, wind_dir: f32, angle: f32) -> f32 {
    // convert from m/h to km/h
    let ws_kph: f32 = wind_speed * 0.001;
    // constant for formula
    let a_const: f32 = 1. - ((D1 * (D2 * f32::tanh((0. / D3) - D4))) + (0. / D5));
    // contribution of wind - module
    let w_eff_mod: f32 = a_const + (D1 * (D2 * f32::tanh((ws_kph / D3) - D4))) + (ws_kph / D5);
    let a: f32 = (w_eff_mod - 1.) / 4.;
    // normalize on direction
    let theta: f32 = wind_dir - angle;
    let theta_norm: f32 = (theta + PI) % (2. * PI) - PI;
    let w_eff_on_dir: f32 = (a + 1.) * (1. - f32::powf(a, 2.)) / (1. - a * f32::cos(theta_norm));
    w_eff_on_dir
}

/// Get the slope effect on the fire propagation at the desired angle
/// # Arguments
/// * `slope` - Slope \[radians\]
/// * `aspect` - Aspect \[radians\]
/// * `angle` - Angle \[radians\]
/// # Returns
/// * `h_eff_on_dir` - Slope effect in angle direction \[adim\]
pub fn get_slope_effect_angle(slope: f32, aspect: f32, angle: f32) -> f32 {
    // slope in angle direction
    let s: f32 = f32::atan(f32::cos(aspect - angle) * f32::tan(slope));
    // slope effect in angle direction
    let h_eff_on_dir: f32 = f32::powf(2., f32::tanh(f32::powf(s * 3., 2.) * f32::signum(s)));
    h_eff_on_dir
}

/// Get the combined effect of wind and slope on the fire propagation at the desired angle
/// # Arguments
/// * `slope` - Slope \[radians\]
/// * `aspect` - Aspect \[radians\]
/// * `wind_speed` - Wind speed \[m/h\]
/// * `wind_dir` - Wind direction \[radians\]
/// * `angle` - Angle \[radians\]
/// # Returns
/// * `wh` - Combined effect of wind and slope in angle direction \[adim\]
pub fn get_wind_slope_effect_angle(
    slope: f32,
    aspect: f32,
    wind_speed: f32,
    wind_dir: f32,
    angle: f32,
) -> f32 {
    let w_eff: f32 = get_wind_effect_angle(wind_speed, wind_dir, angle);
    let s_eff: f32 = get_slope_effect_angle(slope, aspect, angle);
    let wh: f32 = s_eff * w_eff;
    wh
}

/// Get the wind and slope effect on the fire propagation considering all angles
/// # Arguments
/// * `slope` - Slope \[radians\]
/// * `aspect` - Aspect \[radians\]
/// * `wind_speed` - Wind speed \[m/h\]
/// * `wind_dir` - Wind direction \[radians\]
/// # Returns
/// * `ws_effect` - Wind and slope effect \[adim\]
pub fn get_wind_slope_effect(slope: f32, aspect: f32, wind_speed: f32, wind_dir: f32) -> f32 {
    let angles: Array1<f32> = Array::linspace(0., 2. * PI, N_ANGLES_ROS);
    let ws_effect: f32 = angles
        .iter()
        .map(|x| get_wind_slope_effect_angle(slope, aspect, wind_speed, wind_dir, *x))
        .reduce(f32::max)
        .unwrap_or(NODATAVAL);
    ws_effect
}

pub fn get_moisture_effect_v2023(dffm: f32) -> f32 {
    // normalize in [0, 1] and divide by moisture of extintion
    let x: f32 = (dffm / 100.) / MX;
    // moisture effect
    let moist_eff: f32 = M5 * f32::powf(x, 5.)
        + M4 * f32::powf(x, 4.)
        + M3 * f32::powf(x, 3.)
        + M2 * f32::powf(x, 2.)
        + M1 * x
        + M0;
    // clip in [0, 1]
    moist_eff.clamp(0.0, 1.)
}

#[allow(clippy::too_many_arguments)]
pub fn get_v_v2023(
    v0: f32,
    d0: f32,
    _d1: f32,
    snow_cover: f32,
    dffm: f32,
    slope: f32,
    aspect: f32,
    wind_speed: f32,
    wind_dir: f32,
    t_effect: f32,
) -> (f32, f32) {
    let w_s_eff: f32 = get_wind_slope_effect(slope, aspect, wind_speed, wind_dir);
    if snow_cover > 0.0 || d0 == NODATAVAL {
        return (0.0, w_s_eff);
    }
    if dffm == NODATAVAL {
        return (0.0, w_s_eff);
    }
    // moisture effect
    let moist_coeff: f32 = get_moisture_effect_v2023(dffm);
    // wind-slope contribution
    let ros = v0 * moist_coeff * w_s_eff * t_effect;
    (ros, w_s_eff)
}

///compute the meteo index v2023
pub fn get_meteo_index(dffm: f32, w_effect: f32) -> f32 {
    if dffm <= NODATAVAL || w_effect < 1.0 || w_effect == NODATAVAL {
        return NODATAVAL;
    };
    // values set according to RISICO 2023 Italia implementation
    let col = if (0.0..=3.5).contains(&dffm) {
        0  // extreme
    } else if dffm <= 5.9 && dffm > 3.5 {
        1  // high - medium high
    } else if dffm <= 10.3 && dffm > 5.9 {
        2  // medium
    } else if dffm <= 15.9 && dffm > 10.3 {
        3  // medium low
    } else if dffm <= 25.0 && dffm > 15.9 {
        4  // low
    } else {
        5  // very low
    };

    let row = if (1.0..=1.24).contains(&w_effect) {
        0  // very low - low
    } else if w_effect > 1.24 && w_effect <= 2.1 {
        1  // medium low - medium
    } else if w_effect > 2.1 && w_effect <= 2.44 {
        2  // medium high
    } else if w_effect > 2.44 && w_effect <= 3.38 {
        3  // high
    } else {
        4  // extreme
    };

    FWI_TABLE[col + row * 6]
}


// ---------------- v2025 ---------------- //

pub fn get_moisture_effect_v2025(dffm: f32) -> f32 {
    // normalize in [0, 1]
    let x: f32 = dffm / 100.;
    // moisture effect
    let x0: f32 = 2.0;
    let f: f32 = 60.0;
    let a: f32 = 0.2;
    let b: f32 = 20.0;
    let d: f32 = 1.0;
    let moist_eff: f32 = (x0-d)*f32::exp(-f*x) + d / (1.0 + f32::exp(b*(x-a))); 
    // clip in [0, 2]
    moist_eff.clamp(0.0, x0)
}

#[allow(clippy::too_many_arguments)]
pub fn get_v_v2025(
    v0: f32,
    d0: f32,
    _d1: f32,
    snow_cover: f32,
    dffm: f32,
    slope: f32,
    aspect: f32,
    wind_speed: f32,
    wind_dir: f32,
    t_effect: f32,
) -> (f32, f32) {
    if wind_speed == NODATAVAL || wind_dir == NODATAVAL {
        return (0.0, NODATAVAL);
    }
    let w_s_eff: f32 = get_wind_slope_effect(slope, aspect, wind_speed, wind_dir);
    if snow_cover > 0.0 || d0 == NODATAVAL {
        return (0.0, w_s_eff);
    }
    if dffm == NODATAVAL {
        return (0.0, w_s_eff);
    }
    // moisture effect
    let moist_coeff: f32 = get_moisture_effect_v2025(dffm);
    // wind-slope contribution
    let ros = v0 * moist_coeff * w_s_eff * t_effect;
    (ros, w_s_eff)
}

//------------------ GENERIC UPDATE FUNCTIONS ------------------//


#[allow(non_snake_case)]
pub fn update_moisture_fn(
    state: &mut RISICOStateElement,
    props: &RISICOPropertiesElement,
    input_data: &InputElement,
    config: &RISICOModelConfig,
    dt: f32,
) {
    let veg = &props.vegetation;
    let d0 = veg.d0;
    let sat = veg.sat;
    let temperature = input_data.temperature;
    let humidity = input_data.humidity;
    let wind_speed = input_data.wind_speed;
    let rain = input_data.rain;
    let T0 = veg.T0;

    if d0 <= 0.0 {
        state.dffm = NODATAVAL;
        return;
    } else if state.snow_cover > SNOW_COVER_THRESHOLD {
        state.dffm = sat;
        return;
    } else if temperature == NODATAVAL || humidity == NODATAVAL {
        // keep current humidity if we don't have all the data
        return;
    }

    let t = if temperature > 0.0 { temperature } else { 0.0 };

    let h = if humidity <= 100.0 { humidity } else { 100.0 };
    let w = if wind_speed != NODATAVAL {
        wind_speed
    } else {
        0.0
    };
    let r = if rain != NODATAVAL { rain } else { 0.0 };

    if r > MAXRAIN {
        state.dffm = config.ffmc_rain(r, state.dffm, sat);
    } else {
        state.dffm = config.ffmc_no_rain(state.dffm, sat, t, w, h, T0, dt);
    }

    // limit dffm to [0, sat]

    state.dffm = f32::max(0.0, f32::min(sat, state.dffm));
}


#[allow(non_snake_case)]
pub fn get_output_fn(
    state: &RISICOStateElement,
    props: &RISICOPropertiesElement,
    input: &InputElement,
    config: &RISICOModelConfig,
    time: &DateTime<Utc>,
) -> OutputElement {
    let veg = &props.vegetation;

    let wind_dir = input.wind_dir;
    let wind_speed = input.wind_speed;
    let humidity = input.humidity;
    let rain = input.rain;

    let slope = props.slope;
    let aspect = props.aspect;

    let temperature = input.temperature;
    let snow_cover = state.snow_cover;
    let NDVI = state.NDVI;
    let NDWI = state.NDWI;

    let dffm = state.dffm;

    let ndvi = if veg.use_ndvi && NDVI != NODATAVAL {
        (1.0 - NDVI).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let ndwi = if NDWI != NODATAVAL {
        (1.0 - NDWI).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let t_effect = if config.use_t_effect {
        get_t_effect(temperature)
    } else {
        1.0
    };

    let (ros, wind_effect) = config.ros(
        veg.v0, veg.d0, veg.d1, dffm, snow_cover, slope, aspect, wind_speed, wind_dir, t_effect,
    );

    let meteo_index = config.meteo_index(dffm, wind_effect);

    let ppf = get_ppf(time, props.ppf_summer, props.ppf_winter);

    let intensity = if ros != NODATAVAL && veg.hhv != NODATAVAL {
        let LHVdff = get_lhv_dff(veg.hhv, dffm);
        // calcolo LHV per la vegetazione viva
        let LHVl1 = get_lhv_l1(veg.umid, state.MSI, veg.hhv);
        // Calcolo Intensità
        get_intensity(veg.d0, veg.d1, ros, state.NDVI, LHVdff, LHVl1)
    } else {
        NODATAVAL
    };

    let wind_speed_out = wind_speed / 3600.0; // convert to m/s
    let wind_dir_out = wind_dir.to_degrees();
    OutputElement {
        V: ros,
        W: wind_effect,
        PPF: ppf,
        I: intensity,
        temperature,
        humidity,
        wind_speed: wind_speed_out,
        wind_dir: wind_dir_out,
        rain,
        snow_cover,
        dffm,
        t_effect,
        NDWI: ndwi,
        NDVI: ndvi,
        meteo_index,
        ..OutputElement::default()
    }
}
