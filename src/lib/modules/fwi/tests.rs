#[cfg(test)]
mod tests {
    use super::super::functions::{
        from_ffmc_to_moisture, from_moisture_to_ffmc,
        moisture_rain_effect, compute_isi, compute_bui, compute_fwi, compute_ifwi,
        dmc_rain_effect, update_dmc, dc_rain_effect, update_dc,
    };

    // --- FFMC conversion round-trips ---

    #[test]
    fn test_ffmc_moisture_roundtrip() {
        // Converting ffmc -> moisture -> ffmc should give back the original value
        let ffmc_init = 85.0;
        let moisture = from_ffmc_to_moisture(ffmc_init);
        let ffmc_back = from_moisture_to_ffmc(moisture);
        assert!((ffmc_init - ffmc_back).abs() < 0.1, "Round-trip failed: {} -> {} -> {}", ffmc_init, moisture, ffmc_back);
    }

    #[test]
    fn test_ffmc_to_moisture_range() {
        // FFMC in [0, 101] -> moisture in [0, 250]
        for ffmc in [0.0_f32, 50.0, 85.0, 101.0] {
            let m = from_ffmc_to_moisture(ffmc);
            assert!(m >= 0.0, "Moisture should be >= 0 for ffmc={}, got {}", ffmc, m);
        }
    }

    #[test]
    fn test_moisture_rain_effect_increases_moisture() {
        // Rain should increase moisture content
        let moisture_init = 50.0;
        let result = moisture_rain_effect(moisture_init, 10.0);
        assert!(result > moisture_init, "Rain should increase moisture, got {}", result);
    }

    #[test]
    fn test_moisture_rain_effect_clamped() {
        // Result should be clamped to [0, 250]
        let result = moisture_rain_effect(200.0, 100.0);
        assert!(result <= 250.0 && result >= 0.0, "Expected [0, 250], got {}", result);
    }

    // --- DMC tests ---

    #[test]
    fn test_dmc_rain_effect_reduces_dmc() {
        // Heavy rain -> DMC decreases
        let dmc_init = 50.0;
        let result = dmc_rain_effect(dmc_init, 20.0);
        assert!(result < dmc_init, "Rain should reduce DMC, got {}", result);
    }

    #[test]
    fn test_update_dmc_no_rain_warm() {
        // No rain, warm temp -> DMC increases
        let dmc_init = 20.0;
        let result = update_dmc(dmc_init, 0.0, 25.0, 40.0, 9.0);
        assert!(result > dmc_init, "Warm dry day should increase DMC");
    }

    #[test]
    fn test_update_dmc_non_negative() {
        // DMC should never go negative
        let result = update_dmc(0.0, 50.0, 25.0, 40.0, 9.0);
        assert!(result >= 0.0, "DMC should be >= 0, got {}", result);
    }

    // --- DC tests ---

    #[test]
    fn test_dc_rain_effect_reduces_dc() {
        let dc_init = 200.0;
        let result = dc_rain_effect(dc_init, 30.0);
        assert!(result < dc_init, "Rain should reduce DC, got {}", result);
    }

    #[test]
    fn test_update_dc_no_rain_warm() {
        // No rain, warm temp -> DC increases
        let dc_init = 100.0;
        let result = update_dc(dc_init, 0.0, 25.0, 5.0);
        assert!(result > dc_init, "Warm dry day should increase DC");
    }

    // --- ISI tests ---

    #[test]
    fn test_isi_increases_with_wind() {
        let isi_calm = compute_isi(80.0, 5000.0);
        let isi_windy = compute_isi(80.0, 50000.0);
        assert!(isi_windy > isi_calm, "ISI should increase with wind speed");
    }

    #[test]
    fn test_isi_decreases_with_moisture() {
        let isi_dry = compute_isi(20.0, 20000.0);
        let isi_wet = compute_isi(150.0, 20000.0);
        assert!(isi_dry > isi_wet, "ISI should decrease with higher moisture");
    }

    // --- BUI tests ---

    #[test]
    fn test_bui_zero_dmc() {
        // When DMC is 0, BUI should be 0
        let result = compute_bui(0.0, 100.0);
        assert_eq!(result, 0.0, "BUI should be 0 when DMC is 0");
    }

    #[test]
    fn test_bui_positive() {
        let result = compute_bui(30.0, 100.0);
        assert!(result > 0.0, "BUI should be positive, got {}", result);
    }

    #[test]
    fn test_bui_non_negative() {
        let result = compute_bui(5.0, 500.0);
        assert!(result >= 0.0, "BUI should be non-negative, got {}", result);
    }

    // --- FWI tests ---

    #[test]
    fn test_fwi_increases_with_bui() {
        let fwi_low = compute_fwi(10.0, 5.0);
        let fwi_high = compute_fwi(100.0, 5.0);
        assert!(fwi_high > fwi_low, "FWI should increase with BUI");
    }

    #[test]
    fn test_fwi_increases_with_isi() {
        let fwi_low = compute_fwi(50.0, 1.0);
        let fwi_high = compute_fwi(50.0, 20.0);
        assert!(fwi_high > fwi_low, "FWI should increase with ISI");
    }

    #[test]
    fn test_fwi_non_negative() {
        let result = compute_fwi(0.0, 0.0);
        assert!(result >= 0.0, "FWI should be non-negative, got {}", result);
    }

    // --- IFWI tests ---

    #[test]
    fn test_ifwi_zero_when_fwi_le_one() {
        // When FWI <= 1, IFWI should be 0
        let result = compute_ifwi(0.5);
        assert_eq!(result, 0.0, "IFWI should be 0 when FWI <= 1");
    }

    #[test]
    fn test_ifwi_positive_when_fwi_gt_one() {
        let result = compute_ifwi(10.0);
        assert!(result > 0.0, "IFWI should be positive when FWI > 1, got {}", result);
    }

    #[test]
    fn test_ifwi_increases_with_fwi() {
        let ifwi_low = compute_ifwi(5.0);
        let ifwi_high = compute_ifwi(50.0);
        assert!(ifwi_high > ifwi_low, "IFWI should increase with FWI");
    }
}
