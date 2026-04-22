#[cfg(test)]
mod tests {
    use super::super::functions::{kbdi_update_mm};
    use super::super::constants::NODATAVAL;

    #[test]
    fn test_kbdi_clamp_min() {
        // With heavy rain, KBDI should decrease and clamp to 0
        let result = kbdi_update_mm(10.0, 30.0, &[0.0, 50.0], 800.0);
        assert!(result >= 0.0, "KBDI should be >= 0, got {}", result);
    }

    #[test]
    fn test_kbdi_clamp_max() {
        // With very high temperature and no rain, KBDI should clamp to 200
        let result = kbdi_update_mm(199.0, 45.0, &[0.0, 0.0], 300.0);
        assert!(result <= 200.0, "KBDI should be <= 200, got {}", result);
    }

    #[test]
    fn test_kbdi_increases_with_high_temp_no_rain() {
        // High temp, no rain -> KBDI increases
        let initial = 50.0;
        let result = kbdi_update_mm(initial, 35.0, &[0.0, 0.0], 800.0);
        assert!(result > initial, "KBDI should increase with high temp and no rain, got {}", result);
    }

    #[test]
    fn test_kbdi_decreases_with_rain() {
        // Rain > runoff threshold -> KBDI decreases
        let initial = 100.0;
        let result = kbdi_update_mm(initial, 30.0, &[0.0, 30.0], 800.0);
        assert!(result < initial, "KBDI should decrease with heavy rain, got {}", result);
    }

    #[test]
    fn test_kbdi_rain_runoff_consecutive_days() {
        // Consecutive rainy days reduce the runoff threshold
        // last_rain from prev days = 10mm -> effective rain = day_rain - max(0, RUNOFF - 10)
        let result = kbdi_update_mm(100.0, 30.0, &[0.0, 10.0, 15.0], 800.0);
        assert!(result >= 0.0 && result <= 200.0);
    }
}
