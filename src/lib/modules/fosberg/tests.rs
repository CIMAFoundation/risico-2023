#[cfg(test)]
mod tests {
    use super::super::functions::{emc, ffwi, get_output_fn};
    use super::super::models::FosbergStateElement;
    use super::super::constants::NODATAVAL;
    use crate::constants::NODATAVAL as OUTPUT_NODATAVAL;

    #[test]
    fn test_emc_low_humidity() {
        // humidity < 10%
        let result = emc(20.0, 5.0);
        // temp_f = 20 * 9/5 + 32 = 68.0
        // emc = 0.03229 + 0.281073 * 5 - 0.000578 * 68 * 5
        let expected = 0.03229 + 0.281073 * 5.0 - 0.000578 * 68.0 * 5.0;
        assert!((result - expected).abs() < 1e-4, "Expected ~{}, got {}", expected, result);
    }

    #[test]
    fn test_emc_mid_humidity() {
        // humidity in [10, 50)
        let result = emc(25.0, 30.0);
        // temp_f = 25 * 9/5 + 32 = 77.0
        // emc = 2.22749 + 0.160107 * 30 - 0.01478 * 77
        let expected = 2.22749 + 0.160107 * 30.0 - 0.01478 * 77.0;
        assert!((result - expected).abs() < 1e-4, "Expected ~{}, got {}", expected, result);
    }

    #[test]
    fn test_emc_high_humidity() {
        // humidity >= 50%
        let result = emc(15.0, 70.0);
        // temp_f = 15 * 9/5 + 32 = 59.0
        // emc = 21.0606 + 0.005565 * 70^2 - 0.00035 * 59 * 70 - 0.483199 * 70
        let expected = 21.0606 + 0.005565 * 70.0f32.powi(2) - 0.00035 * 59.0 * 70.0 - 0.483199 * 70.0;
        assert!((result - expected).abs() < 1e-3, "Expected ~{}, got {}", expected, result);
    }

    #[test]
    fn test_ffwi_clamp_min() {
        // Very high humidity -> ffwi should be clamped to 0
        let result = ffwi(5.0, 95.0, 0.0);
        assert!(result >= 0.0, "ffwi should be >= 0, got {}", result);
    }

    #[test]
    fn test_ffwi_clamp_max() {
        // Extreme conditions -> ffwi should be clamped to 100
        let result = ffwi(45.0, 2.0, 200000.0);
        assert!(result <= 100.0, "ffwi should be <= 100, got {}", result);
    }

    #[test]
    fn test_ffwi_moderate_conditions() {
        // Moderate conditions -> ffwi should be in [0, 100]
        let result = ffwi(25.0, 30.0, 3600.0);
        assert!(result >= 0.0 && result <= 100.0, "ffwi should be in [0, 100], got {}", result);
    }

    #[test]
    fn test_get_output_fn_valid() {
        let state = FosbergStateElement {
            temp: 25.0,
            humidity: 30.0,
            wind_speed: 3600.0, // 1 m/s in m/h
        };
        let output = get_output_fn(&state);
        assert!(output.ffwi >= 0.0 && output.ffwi <= 100.0);
        assert!((output.wind_speed - 1.0).abs() < 1e-5); // should be converted to m/s
    }

    #[test]
    fn test_get_output_fn_nodata() {
        let state = FosbergStateElement {
            temp: NODATAVAL,
            humidity: 30.0,
            wind_speed: 3600.0,
        };
        let output = get_output_fn(&state);
        assert_eq!(output.ffwi, OUTPUT_NODATAVAL);
    }
}
