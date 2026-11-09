#[cfg(test)]
mod tests {
    use super::super::functions::*;
    use crate::constants::NODATAVAL;
    use chrono::{TimeZone, Utc};

    // --- get_ppf ---

    #[test]
    fn test_ppf_summer() {
        // July 15 = day 196, should return ppf_summer
        let time = Utc.with_ymd_and_hms(2023, 7, 15, 0, 0, 0).unwrap();
        let ppf = get_ppf(&time, 0.9, 0.3);
        assert!((ppf - 0.9).abs() < 1e-3, "Expected summer PPF, got {}", ppf);
    }

    #[test]
    fn test_ppf_winter() {
        // January 15 = day 15, should return ppf_winter
        let time = Utc.with_ymd_and_hms(2023, 1, 15, 0, 0, 0).unwrap();
        let ppf = get_ppf(&time, 0.9, 0.3);
        assert!((ppf - 0.3).abs() < 1e-3, "Expected winter PPF, got {}", ppf);
    }

    #[test]
    fn test_ppf_transition_april() {
        // April 15 is in spring transition (day ~105): value should be between winter and summer
        let time = Utc.with_ymd_and_hms(2023, 4, 15, 0, 0, 0).unwrap();
        let ppf = get_ppf(&time, 0.9, 0.3);
        assert!(ppf > 0.3 && ppf < 0.9, "Expected transition PPF, got {}", ppf);
    }

    #[test]
    fn test_ppf_negative_returns_zero() {
        let time = Utc.with_ymd_and_hms(2023, 7, 15, 0, 0, 0).unwrap();
        let ppf = get_ppf(&time, -1.0, 0.3);
        assert_eq!(ppf, 0.0, "Negative ppf_summer should return 0");
    }

    // --- update_dffm_rain / update_dffm_rain_legacy ---

    #[test]
    fn test_update_dffm_rain_increases_moisture() {
        let dffm_before = 10.0;
        let sat = 60.0;
        let result = update_dffm_rain(5.0, dffm_before, sat);
        assert!(result > dffm_before, "Rain should increase dffm, got {}", result);
    }

    #[test]
    fn test_update_dffm_rain_clamped_to_sat() {
        let result = update_dffm_rain(100.0, 55.0, 60.0);
        assert!(result <= 60.0, "dffm should be clamped to sat, got {}", result);
    }

    #[test]
    fn test_update_dffm_rain_legacy_increases_moisture() {
        let result = update_dffm_rain_legacy(5.0, 10.0, 60.0);
        assert!(result > 10.0, "Legacy rain should increase dffm, got {}", result);
    }

    // --- update_dffm_dry ---

    #[test]
    fn test_update_dffm_dry_converges_to_emc() {
        // After a long dry period (large dt), dffm should converge toward EMC
        // T=25, H=30, W=0 -> EMC ~ some value; start wet dffm=50
        let result = update_dffm_dry(50.0, 60.0, 25.0, 0.0, 30.0, 1.0, 10000.0);
        assert!(result < 50.0, "Drying: dffm should decrease toward EMC, got {}", result);
        assert!(result >= 0.0, "dffm should not go negative, got {}", result);
    }

    #[test]
    fn test_update_dffm_dry_no_negative() {
        let result = update_dffm_dry(0.0, 60.0, 40.0, 0.0, 5.0, 1.0, 3600.0);
        assert!(result >= 0.0, "dffm should not go negative, got {}", result);
    }

    // --- get_moisture_effect ---

    #[test]
    fn test_moisture_effect_v2023_zero_is_max() {
        let eff_zero = get_moisture_effect_v2023(0.0);
        let eff_high = get_moisture_effect_v2023(30.0);
        assert!(eff_zero > eff_high, "Dry fuel should have higher moisture effect");
    }

    #[test]
    fn test_moisture_effect_v2023_clamped() {
        let result = get_moisture_effect_v2023(0.0);
        assert!(result <= 1.0 && result >= 0.0, "Expected [0,1], got {}", result);
    }

    #[test]
    fn test_moisture_effect_v2025_zero_is_max() {
        let eff_zero = get_moisture_effect_v2025(0.0);
        let eff_high = get_moisture_effect_v2025(50.0);
        assert!(eff_zero > eff_high, "Dry fuel should have higher moisture effect (v2025)");
    }

    // --- get_wind_effect_legacy ---

    #[test]
    fn test_wind_effect_legacy_nodata_wind_speed() {
        let result = get_wind_effect_legacy(NODATAVAL, 0.0, 0.0, 0.0);
        assert_eq!(result, 1.0, "NODATAVAL wind_speed should return 1.0");
    }

    #[test]
    fn test_wind_effect_legacy_nodata_wind_dir() {
        let result = get_wind_effect_legacy(5.0, NODATAVAL, 0.0, 0.0);
        assert_eq!(result, 1.0, "NODATAVAL wind_dir should return 1.0");
    }

    // --- get_slope_effect_legacy ---

    #[test]
    fn test_slope_effect_legacy_zero_slope() {
        use std::f32::consts::PI;
        let result = get_slope_effect_legacy(0.0);
        assert!((result - 1.0).abs() < 1e-3, "Zero slope should give effect ~1.0, got {}", result);
    }

    #[test]
    fn test_slope_effect_legacy_increases_with_slope() {
        let low = get_slope_effect_legacy(0.1);
        let high = get_slope_effect_legacy(0.5);
        assert!(high > low, "Steeper slope should give higher effect");
    }

    // --- get_lhv_dff ---

    #[test]
    fn test_lhv_dff_decreases_with_moisture() {
        let lhv_dry = get_lhv_dff(18000.0, 5.0);
        let lhv_wet = get_lhv_dff(18000.0, 30.0);
        assert!(lhv_dry > lhv_wet, "Higher moisture should reduce LHV");
    }

    // --- get_lhv_l1 ---

    #[test]
    fn test_lhv_l1_nodata_humidity() {
        let result = get_lhv_l1(NODATAVAL, 0.5, 18000.0);
        assert_eq!(result, 0.0, "NODATAVAL humidity should return 0.0");
    }

    // --- get_intensity ---

    #[test]
    fn test_intensity_zero_ros() {
        let result = get_intensity(1.0, 0.0, 0.0, -1.0, 15000.0, 12000.0);
        assert_eq!(result, 0.0, "Zero ROS should give zero intensity");
    }

    #[test]
    fn test_intensity_positive_with_valid_inputs() {
        let result = get_intensity(1.0, 0.0, 10.0, -1.0, 15000.0, 12000.0);
        assert!(result > 0.0, "Positive ROS should give positive intensity");
    }

    // --- get_meteo_index ---

    #[test]
    fn test_meteo_index_legacy_nodata() {
        let result = get_meteo_index_legacy(NODATAVAL, 1.5);
        assert_eq!(result, NODATAVAL, "NODATAVAL dffm should return NODATAVAL");
    }

    #[test]
    fn test_meteo_index_legacy_low_wind_effect() {
        // w_effect < 1.0 should return NODATAVAL
        let result = get_meteo_index_legacy(10.0, 0.5);
        assert_eq!(result, NODATAVAL, "w_effect < 1.0 should return NODATAVAL");
    }

    #[test]
    fn test_meteo_index_v2023_nodata() {
        let result = get_meteo_index_v2023(NODATAVAL, 1.5);
        assert_eq!(result, NODATAVAL);
    }

    #[test]
    fn test_meteo_index_v2025_nodata() {
        let result = get_meteo_index_v2025(NODATAVAL, 1.5);
        assert_eq!(result, NODATAVAL);
    }

    #[test]
    fn test_meteo_index_v2023_dry_high_wind() {
        // Very dry (dffm=1.0) + high wind (w_effect=3.0) -> should be high danger (4.0)
        let result = get_meteo_index_v2023(1.0, 3.0);
        assert_eq!(result, 4.0, "Extreme conditions should return 4.0, got {}", result);
    }

    // --- index_from_swi ---

    #[test]
    fn test_index_from_swi_below_threshold() {
        let result = index_from_swi(15.0, 5.0);
        assert_eq!(result, 0.0, "SWI <= 10 should return 0");
    }

    #[test]
    fn test_index_from_swi_above_threshold() {
        let result = index_from_swi(15.0, 20.0);
        assert_eq!(result, 15.0, "SWI > 10 should return dffm");
    }
}
