use ndarray::Array;
use std::f32::consts::PI;

///functions to work on the state of the risico model
use chrono::{DateTime, Datelike, Utc};

use super::constants::*;

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

    let ppf = match day_number {
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
    };
    ppf
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

pub fn get_slope_effect_legacy(slope: f32) -> f32 {
    1.0 + LAMBDA * (slope / (PI / 2.0))
}

pub fn get_moisture_effect_legacy(dffm: f32) -> f32 {
    f32::exp(-1.0 * f32::powf(dffm / 20.0, 2.0))
}

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
    if snow_cover > 0.0 || d0 == NODATAVAL {
        return (0.0, 0.0);
    }
    let moist_eff: f32 = get_moisture_effect_legacy(dffm);
    let w_effect: f32 = get_wind_effect_legacy(wind_speed, wind_dir, slope, aspect);
    let s_effect: f32 = get_slope_effect_legacy(slope);
    (v0 * moist_eff * w_effect * s_effect * t_effect, w_effect)
}

// NEW FORMULATION ROS

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

pub fn get_slope_effect_angle(slope: f32, aspect: f32, angle: f32) -> f32 {
    // slope in angle direction
    let s: f32 = f32::atan(f32::cos(aspect - angle) * f32::tan(slope));
    // slope effect in angle direction
    let h_eff_on_dir: f32 = f32::powf(2., f32::tanh(f32::powf(s * 3., 2.) * f32::signum(s)));
    h_eff_on_dir
}

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
    // DEPRECATED - normalization
    // wh = wh - 1.0;
    // if wh > 0. {
    //     wh = wh / 2.13;
    // } else if wh < 0. {
    //     wh = wh / 1.12;
    // }
    // wh + 1.
}

pub fn get_wind_slope_effect(slope: f32, aspect: f32, wind_speed: f32, wind_dir: f32) -> f32 {
    let angles: ndarray::prelude::ArrayBase<
        ndarray::OwnedRepr<f32>,
        ndarray::prelude::Dim<[usize; 1]>,
    > = Array::linspace(0., 2. * PI, N_ANGLES_ROS);
    let ws_all = angles
        .iter()
        .map(|x| get_wind_slope_effect_angle(slope, aspect, wind_speed, wind_dir, *x));
    let ws_effect: f32 = ws_all.reduce(f32::max).unwrap_or(NODATAVAL);
    ws_effect
}

pub fn get_moisture_effect(dffm: f32) -> f32 {
    // noramlize in [0, 1] and divide by moisture of extintion
    let x: f32 = (dffm / 100.) / MX;
    // moisture effect
    let moist_eff: f32 = M5 * f32::powf(x, 5.)
        + M4 * f32::powf(x, 4.)
        + M3 * f32::powf(x, 3.)
        + M2 * f32::powf(x, 2.)
        + M1 * x
        + M0;
    // clip in [0, 1]
    f32::max(0.0, f32::min(1., moist_eff))
}

pub fn get_v(
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
    if snow_cover > 0.0 || d0 == NODATAVAL {
        return (0.0, 0.0);
    }
    // moisture effect
    let moist_coeff: f32 = get_moisture_effect(dffm);
    // wind-slope contribution
    let w_s_eff: f32 = get_wind_slope_effect(slope, aspect, wind_speed, wind_dir);
    (v0 * moist_coeff * w_s_eff * t_effect, w_s_eff)
}

/// DEPRECATED
pub fn get_t_effect(t: f32) -> f32 {
    if t <= 0.0 {
        return 1.0;
    }
    f32::exp(t * 0.0171)
}

pub fn get_lhv_dff(hhv: f32, dffm: f32) -> f32 {
    hhv * (1.0 - (dffm / 100.0)) - Q * (dffm / 100.0)
}

pub fn get_lhv_l1(humidity: f32, msi: f32, hhv: f32) -> f32 {
    if humidity == NODATAVAL {
        return 0.0;
    }
    let lhv_l1: f32;
    if msi >= 0.0 && msi <= 1.0 {
        let l1_msi = f32::max(20.0, humidity - (20.0 * msi));
        lhv_l1 = hhv * (1.0 - (l1_msi / 100.0)) - Q * (l1_msi / 100.0);
    } else {
        lhv_l1 = hhv * (1.0 - (humidity / 100.0)) - Q * (humidity / 100.0);
    }
    lhv_l1
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

///Get the new value for the dfmm when is raining (p>p*)
pub fn update_dffm_rain_legacy(r: f32, dffm: f32, sat: f32) -> f32 {
    let delta_dffm = r
        * R1_LEGACY
        * f32::exp(-R2_LEGACY / ((sat + 1.0) - dffm))
        * (1.0 - f32::exp(-R3_LEGACY / r));
    let dffm = dffm + delta_dffm;

    f32::min(dffm, sat)
}

///Get the new value for the dfmm when is raining (p>p*)
pub fn update_dffm_rain(r: f32, dffm: f32, sat: f32) -> f32 {
    let delta_dffm = r * R1 * f32::exp(-R2 / ((sat + 1.0) - dffm)) * (1.0 - f32::exp(-R3 / r));
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

pub fn index_from_swi(dffm: f32, swi: f32) -> f32 {
    if swi <= 10.0 {
        return 0.0;
    };
    dffm
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};

    #[test]
    fn test_get_ppf_winter() {
        // create an utc datetime object
        let date: DateTime<Utc> = DateTime::<Utc>::from_utc(
            NaiveDate::from_ymd_opt(2019, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            Utc,
        );
        let ppf = get_ppf(&date, 0.0, 1.0);
        assert_eq!(ppf, 0.0);
    }

    #[test]
    fn test_get_ppf_summer() {
        let date: DateTime<Utc> = DateTime::<Utc>::from_utc(
            NaiveDate::from_ymd_opt(2019, 7, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            Utc,
        );

        let ppf = get_ppf(&date, 0.0, 1.0);
        assert_eq!(ppf, 1.0);
    }

    #[test]
    fn test_get_ppf_spring() {
        let date: DateTime<Utc> = DateTime::<Utc>::from_utc(
            NaiveDate::from_ymd_opt(2019, 4, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            Utc,
        );
        let ppf = get_ppf(&date, 0.0, 1.0);
        assert_eq!(ppf, 0.5);
    }

    #[test]
    fn test_get_ppf_autumn() {
        let date: DateTime<Utc> = DateTime::<Utc>::from_utc(
            NaiveDate::from_ymd_opt(2019, 10, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            Utc,
        );
        let ppf = get_ppf(&date, 0.0, 1.0);
        assert_eq!(ppf, 0.5);
    }

    // #[test]
    // fn test_get_ppf_autumn() {
    //     let date = NaiveDate::from_ymd(2019, 10, 1);
    //     let ppf = get_ppf(&date, 0.0, 1.0);
    //     assert_eq!(ppf, 0.5);
    // }
}
