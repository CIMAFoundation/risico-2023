#[cfg(test)]
mod tests {
    use super::super::functions::{nesterov_update, get_output_fn};
    use super::super::models::NesterovStateElement;
    use super::super::constants::{NODATAVAL, RAIN_TH};

    #[test]
    fn test_nesterov_update_no_rain() {
        // No rain -> nesterov accumulates: prev + temp*(temp - dew)
        // temp=25, dew=15 -> 25*(25-15) = 250
        let result = nesterov_update(100.0, 25.0, 15.0, 0.0);
        assert!((result - 350.0).abs() < 1e-4, "Expected 350.0, got {}", result);
    }

    #[test]
    fn test_nesterov_update_with_heavy_rain() {
        // Rain above threshold -> nesterov resets to 0
        let result = nesterov_update(500.0, 25.0, 15.0, RAIN_TH + 1.0);
        assert_eq!(result, 0.0, "Nesterov should reset to 0 after heavy rain");
    }

    #[test]
    fn test_nesterov_update_negative_clamp() {
        // If result would be negative, clamp to 0
        // temp=10, dew=20 -> 10*(10-20) = -100, prev=50 -> 50 + (-100) = -50 -> clamped to 0
        let result = nesterov_update(50.0, 10.0, 20.0, 0.0);
        assert_eq!(result, 0.0, "Nesterov should not go below 0");
    }

    #[test]
    fn test_nesterov_update_accumulates_over_days() {
        // Simulates accumulation over multiple dry days
        let mut nesterov = 0.0_f32;
        for _ in 0..5 {
            nesterov = nesterov_update(nesterov, 30.0, 10.0, 0.0);
        }
        // Each day: 30*(30-10) = 600 -> after 5 days: 3000
        assert!((nesterov - 3000.0).abs() < 1e-2, "Expected 3000.0, got {}", nesterov);
    }

    #[test]
    fn test_get_output_fn_valid() {
        let state = NesterovStateElement {
            nesterov: 350.0,
            temp_15: 25.0,
            temp_dew_15: 15.0,
            cum_rain: 0.0,
        };
        let output = get_output_fn(&state);
        assert!((output.nesterov - 350.0).abs() < 1e-4);
        assert!((output.temperature - 25.0).abs() < 1e-4);
        assert!((output.temp_dew - 15.0).abs() < 1e-4);
        assert!((output.rain - 0.0).abs() < 1e-4);
    }
}
