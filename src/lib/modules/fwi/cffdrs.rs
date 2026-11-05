//! Canonical daily Canadian Forest Fire Weather Index System (`model_version: cffdrs`).
//!
//! Equations: Van Wagner, C.E. 1987, *Development and structure of the
//! Canadian Forest Fire Weather Index System*, Forestry Technical Report 35
//! (equation numbers below), and Van Wagner, C.E. & Pickett, T.L. 1985,
//! *Equations and FORTRAN program for the Canadian Forest Fire Weather Index
//! System*, Forestry Technical Report 33. The implementation follows the
//! reference R package `cffdrs` (<https://github.com/cffdrs/cffdrs_r>,
//! `R/fine_fuel_moisture_code.r`, `R/duff_moisture_code.r`,
//! `R/drought_code.r`, `R/initial_spread_index.r`, `R/buildup_index.r`,
//! `R/fire_weather_index.r`, `R/fwi.r`) line by line, including its latitude
//! adjustment of the day-length factors and its constraints (RH >= 100 is
//! taken as 99.9999, temperature floors of -1.1 C for the DMC and -2.8 C for
//! the DC, rain-phase DC floored at 0).
//!
//! The daily step uses the weather at local standard noon (temperature [C],
//! relative humidity [%], 10 m wind [km/h]) and the rain of the 24 hours
//! ending at that noon [mm]; see `update_state_cffdrs` in `functions.rs` for
//! how hourly or 3-hourly forecasts are turned into those values.
//!
//! All arithmetic is done in `f64`; the state is stored in `f32`.

/// `FFMC_COEFFICIENT <- 250.0 * 59.5 / 101.0` (cffdrs >= 1.9.2; Van Wagner
/// 1987 Eq. 1 and 10 round it to 147.2).
pub const FFMC_COEFFICIENT: f64 = 250.0 * 59.5 / 101.0;

/// Effective day length Le for the DMC (hours), cffdrs `ell01`..`ell04`.
const ELL01: [f64; 12] = [6.5, 7.5, 9.0, 12.8, 13.9, 13.9, 12.4, 10.9, 9.4, 8.0, 7.0, 6.0]; // lat > 30 (VW87 Table 1, 46N)
const ELL02: [f64; 12] = [7.9, 8.4, 8.9, 9.5, 9.9, 10.2, 10.1, 9.7, 9.1, 8.6, 8.1, 7.8]; // 30 >= lat > 10
const ELL03: [f64; 12] = [10.1, 9.6, 9.1, 8.5, 8.1, 7.8, 7.9, 8.3, 8.9, 9.4, 9.9, 10.2]; // -10 >= lat > -30
const ELL04: [f64; 12] = [11.5, 10.5, 9.2, 7.9, 6.8, 6.2, 6.5, 7.4, 8.7, 10.0, 11.2, 11.8]; // lat <= -30
/// Day-length factor Lf for the DC, cffdrs `fl01`, `fl02`.
const FL01: [f64; 12] = [-1.6, -1.6, -1.6, 0.9, 3.8, 5.8, 6.4, 5.0, 2.4, 0.4, -1.6, -1.6]; // lat > 20 (VW87 Table 2)
const FL02: [f64; 12] = [6.4, 5.0, 2.4, 0.4, -1.6, -1.6, -1.6, -1.6, -1.6, 0.9, 3.8, 5.8]; // lat <= -20

fn month_index(month: u32) -> usize {
    (month.clamp(1, 12) - 1) as usize
}

/// DMC day length Le for a latitude (degrees) and month (1-12), cffdrs
/// `duff_moisture_code(lat.adjust = TRUE)`.
pub fn day_length_le(lat: f64, month: u32) -> f64 {
    let i = month_index(month);
    if lat <= 10.0 && lat > -10.0 {
        9.0
    } else if lat <= 30.0 && lat > 10.0 {
        ELL02[i]
    } else if lat <= -10.0 && lat > -30.0 {
        ELL03[i]
    } else if lat <= -30.0 {
        ELL04[i]
    } else {
        ELL01[i]
    }
}

/// DC day-length factor Lf, cffdrs `drought_code(lat.adjust = TRUE)`.
pub fn day_length_lf(lat: f64, month: u32) -> f64 {
    let i = month_index(month);
    if lat <= -20.0 {
        FL02[i]
    } else if lat <= 20.0 {
        1.4
    } else {
        FL01[i]
    }
}

/// FFMC to fine fuel moisture content (%), Eq. 1.
pub fn ffmc_to_moisture(ffmc: f64) -> f64 {
    FFMC_COEFFICIENT * (101.0 - ffmc) / (59.5 + ffmc)
}

/// Fine fuel moisture content (%) to FFMC, Eq. 10.
pub fn moisture_to_ffmc(moisture: f64) -> f64 {
    59.5 * (250.0 - moisture) / (FFMC_COEFFICIENT + moisture)
}

/// Fine Fuel Moisture Code of today, Eq. 1-10.
pub fn fine_fuel_moisture_code(ffmc_yda: f64, temp: f64, rh: f64, ws: f64, prec: f64) -> f64 {
    let mut wmo = ffmc_to_moisture(ffmc_yda);
    if prec > 0.5 {
        let ra = prec - 0.5; // Eq. 2
        let mut wetting = 42.5 * ra * (-100.0 / (251.0 - wmo)).exp() * (1.0 - (-6.93 / ra).exp()); // Eq. 3a
        if wmo > 150.0 {
            wetting += 0.0015 * (wmo - 150.0) * (wmo - 150.0) * ra.sqrt(); // Eq. 3b
        }
        wmo += wetting;
    }
    wmo = wmo.min(250.0);
    let common = 0.18 * (21.1 - temp) * (1.0 - 1.0 / (rh * 0.115).exp());
    let ed = 0.942 * rh.powf(0.679) + 11.0 * ((rh - 100.0) / 10.0).exp() + common; // Eq. 4
    let ew = 0.618 * rh.powf(0.753) + 10.0 * ((rh - 100.0) / 10.0).exp() + common; // Eq. 5
    let wm = if wmo < ed && wmo < ew {
        // wetting: Eq. 7a, 7b, 9
        let z = 0.424 * (1.0 - ((100.0 - rh) / 100.0).powf(1.7))
            + 0.0694 * ws.sqrt() * (1.0 - ((100.0 - rh) / 100.0).powf(8.0));
        let x = z * 0.581 * (0.0365 * temp).exp();
        ew - (ew - wmo) / 10f64.powf(x)
    } else if wmo > ed {
        // drying: Eq. 6a, 6b, 8
        let z = 0.424 * (1.0 - (rh / 100.0).powf(1.7))
            + 0.0694 * ws.sqrt() * (1.0 - (rh / 100.0).powf(8.0));
        let x = z * 0.581 * (0.0365 * temp).exp();
        ed + (wmo - ed) / 10f64.powf(x)
    } else {
        wmo
    };
    moisture_to_ffmc(wm).clamp(0.0, 101.0)
}

/// Duff Moisture Code of today, Eq. 11-17.
pub fn duff_moisture_code(dmc_yda: f64, temp: f64, rh: f64, prec: f64, lat: f64, month: u32) -> f64 {
    let temp = temp.max(-1.1);
    let rk = 1.894 * (temp + 1.1) * (100.0 - rh) * day_length_le(lat, month) * 1e-4; // Eq. 16, 17 (100 K)
    let pr = if prec <= 1.5 {
        dmc_yda
    } else {
        let rw = 0.92 * prec - 1.27; // Eq. 11
        let wmi = 20.0 + 280.0 / (0.023 * dmc_yda).exp(); // Eq. 12 (cffdrs form)
        let b = if dmc_yda <= 33.0 {
            100.0 / (0.5 + 0.3 * dmc_yda) // Eq. 13a
        } else if dmc_yda <= 65.0 {
            14.0 - 1.3 * dmc_yda.ln() // Eq. 13b
        } else {
            6.2 * dmc_yda.ln() - 17.2 // Eq. 13c
        };
        let wmr = wmi + 1000.0 * rw / (48.77 + b * rw); // Eq. 14
        43.43 * (5.6348 - (wmr - 20.0).ln()) // Eq. 15
    };
    (pr.max(0.0) + rk).max(0.0)
}

/// Drought Code of today, Eq. 18-23.
pub fn drought_code(dc_yda: f64, temp: f64, prec: f64, lat: f64, month: u32) -> f64 {
    let temp = temp.max(-2.8);
    let pe = ((0.36 * (temp + 2.8) + day_length_lf(lat, month)) / 2.0).max(0.0); // Eq. 22, 23
    let dr = if prec <= 2.8 {
        dc_yda
    } else {
        let rw = 0.83 * prec - 1.27; // Eq. 18
        let smi = 800.0 * (-dc_yda / 400.0).exp(); // Eq. 19
        (dc_yda - 400.0 * (1.0 + 3.937 * rw / smi).ln()).max(0.0) // Eq. 20, 21
    };
    (dr + pe).max(0.0)
}

/// Initial Spread Index, Eq. 24-26 (wind in km/h).
pub fn initial_spread_index(ffmc: f64, ws: f64) -> f64 {
    let fm = ffmc_to_moisture(ffmc);
    let fw = (0.05039 * ws).exp(); // Eq. 24
    let ff = 91.9 * (-0.1386 * fm).exp() * (1.0 + fm.powf(5.31) / 49_300_000.0); // Eq. 25
    0.208 * fw * ff // Eq. 26
}

/// Buildup Index, Eq. 27a, 27b.
pub fn buildup_index(dmc: f64, dc: f64) -> f64 {
    let bui1 = if dmc == 0.0 && dc == 0.0 {
        0.0
    } else {
        0.8 * dc * dmc / (dmc + 0.4 * dc)
    };
    if bui1 < dmc {
        let p = if dmc == 0.0 { 0.0 } else { (dmc - bui1) / dmc };
        let cc = 0.92 + (0.0114 * dmc).powf(1.7);
        (dmc - cc * p).max(0.0)
    } else {
        bui1
    }
}

/// Fire Weather Index, Eq. 28-30.
pub fn fire_weather_index(isi: f64, bui: f64) -> f64 {
    let bb = if bui > 80.0 {
        0.1 * isi * (1000.0 / (25.0 + 108.64 / (0.023 * bui).exp()))
    } else {
        0.1 * isi * (0.626 * bui.powf(0.809) + 2.0)
    };
    if bb <= 1.0 {
        bb
    } else {
        (2.72 * (0.434 * bb.ln()).powf(0.647)).exp()
    }
}

/// Daily Severity Rating, Eq. 31.
pub fn daily_severity_rating(fwi: f64) -> f64 {
    0.0272 * fwi.powf(1.77)
}

/// Moisture codes after one daily step (FFMC, DMC, DC), cffdrs `fwi()`.
#[allow(clippy::too_many_arguments)]
pub fn daily_codes(
    ffmc_yda: f64,
    dmc_yda: f64,
    dc_yda: f64,
    temp: f64,
    rh: f64,
    ws: f64,
    prec: f64,
    lat: f64,
    month: u32,
) -> (f64, f64, f64) {
    let rh = if rh >= 100.0 { 99.9999 } else { rh.max(0.0) };
    let prec = prec.max(0.0);
    let ws = ws.max(0.0);
    (
        fine_fuel_moisture_code(ffmc_yda, temp, rh, ws, prec),
        duff_moisture_code(dmc_yda, temp, rh, prec, lat, month),
        drought_code(dc_yda, temp, prec, lat, month),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cffdrs `data/test_fwi.csv` (inputs, LAT 40, start 85/6/15) and the
    /// expected outputs of `tests/testthat/data/fwi_01.csv` (cffdrs 1.10.0,
    /// four significant digits): FFMC, DMC, DC, ISI, BUI, FWI, DSR.
    #[rustfmt::skip]
    const FWI_01: [(u32, f64, f64, f64, f64, [f64; 7]); 48] = include!("cffdrs_fwi_01.in");

    fn assert_sig4(name: &str, row: usize, got: f64, expected: f64) {
        let tolerance = expected.abs() * 1.5e-3 + 1e-9;
        let rounded = if got == 0.0 {
            0.0
        } else {
            let e = got.abs().log10().floor() - 3.0;
            (got / 10f64.powf(e)).round() * 10f64.powf(e)
        };
        assert!(
            (rounded - expected).abs() <= tolerance,
            "{name} row {row}: got {got} (4 s.d. {rounded}), expected {expected}"
        );
    }

    #[test]
    fn reproduces_the_cffdrs_fwi_test_series() {
        let (mut ffmc, mut dmc, mut dc) = (85.0, 6.0, 15.0);
        for (row, (month, temp, rh, ws, prec, expected)) in FWI_01.iter().enumerate() {
            let (f, p, d) = daily_codes(ffmc, dmc, dc, *temp, *rh, *ws, *prec, 40.0, *month);
            let isi = initial_spread_index(f, *ws);
            let bui = buildup_index(p, d);
            let fwi = fire_weather_index(isi, bui);
            let dsr = daily_severity_rating(fwi);
            for (name, got, exp) in [
                ("FFMC", f, expected[0]),
                ("DMC", p, expected[1]),
                ("DC", d, expected[2]),
                ("ISI", isi, expected[3]),
                ("BUI", bui, expected[4]),
                ("FWI", fwi, expected[5]),
                ("DSR", dsr, expected[6]),
            ] {
                assert_sig4(name, row, got, exp);
            }
            (ffmc, dmc, dc) = (f, p, d);
        }
    }

    /// Rows of the cffdrs component fixtures (`tests/testthat/data/
    /// DuffMoistureCode.csv`, `DroughtCode.csv`, `FineFuelMoistureCode.csv`,
    /// `BuildupIndex.csv`, `FireWeatherIndex.csv`, `InitialSpreadIndex.csv`)
    /// chosen to cover every latitude band, the temperature floors and the
    /// rain branches.
    #[rustfmt::skip]
    const DMC_ROWS: &[(f64, f64, f64, f64, f64, u32, f64)] = &include!("cffdrs_dmc.in");
    #[rustfmt::skip]
    const DC_ROWS: &[(f64, f64, f64, f64, u32, f64)] = &include!("cffdrs_dc.in");
    #[rustfmt::skip]
    const FFMC_ROWS: &[(f64, f64, f64, f64, f64, f64)] = &include!("cffdrs_ffmc.in");
    #[rustfmt::skip]
    const BUI_ROWS: &[(f64, f64, f64)] = &include!("cffdrs_bui.in");
    #[rustfmt::skip]
    const FWI_ROWS: &[(f64, f64, f64)] = &include!("cffdrs_fwi.in");
    #[rustfmt::skip]
    const ISI_ROWS: &[(f64, f64, f64)] = &include!("cffdrs_isi.in");

    #[test]
    fn reproduces_the_cffdrs_component_fixtures() {
        for (row, (dmc, temp, rh, prec, lat, month, expected)) in DMC_ROWS.iter().enumerate() {
            assert_sig4("DMC", row, duff_moisture_code(*dmc, *temp, *rh, *prec, *lat, *month), *expected);
        }
        for (row, (dc, temp, prec, lat, month, expected)) in DC_ROWS.iter().enumerate() {
            assert_sig4("DC", row, drought_code(*dc, *temp, *prec, *lat, *month), *expected);
        }
        for (row, (ffmc, temp, rh, ws, prec, expected)) in FFMC_ROWS.iter().enumerate() {
            assert_sig4("FFMC", row, fine_fuel_moisture_code(*ffmc, *temp, *rh, *ws, *prec), *expected);
        }
        for (row, (dmc, dc, expected)) in BUI_ROWS.iter().enumerate() {
            assert_sig4("BUI", row, buildup_index(*dmc, *dc), *expected);
        }
        for (row, (isi, bui, expected)) in FWI_ROWS.iter().enumerate() {
            assert_sig4("FWI", row, fire_weather_index(*isi, *bui), *expected);
        }
        for (row, (ffmc, ws, expected)) in ISI_ROWS.iter().enumerate() {
            assert_sig4("ISI", row, initial_spread_index(*ffmc, *ws), *expected);
        }
    }

    #[test]
    fn day_length_bands_follow_cffdrs() {
        assert_eq!(day_length_le(46.0, 11), 7.0);
        assert_eq!(day_length_le(30.0, 6), 10.2);
        assert_eq!(day_length_le(10.0, 6), 9.0);
        assert_eq!(day_length_le(-10.0, 6), 7.8);
        assert_eq!(day_length_le(-30.0, 6), 6.2);
        assert_eq!(day_length_lf(20.5, 7), 6.4);
        assert_eq!(day_length_lf(20.0, 7), 1.4);
        assert_eq!(day_length_lf(-19.9, 7), 1.4);
        assert_eq!(day_length_lf(-20.0, 7), -1.6);
    }

    #[test]
    fn humidity_at_or_above_saturation_is_finite() {
        let (f, p, d) = daily_codes(85.0, 6.0, 15.0, 10.0, 104.0, 20.0, 0.0, 45.0, 6);
        assert!(f.is_finite() && p.is_finite() && d.is_finite());
    }
}
