#[cfg(test)]
mod tests {
    use super::super::functions::{ffdi, rainfall_effect, find_rain_events};
    use super::super::constants::RAIN_TH;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_ffdi_typical_conditions() {
        // Moderate fire weather conditions
        let result = ffdi(25.0, 30.0, 20000.0, 5.0);
        assert!(result > 0.0, "FFDI should be positive, got {}", result);
    }

    #[test]
    fn test_ffdi_increases_with_temperature() {
        let result_low = ffdi(15.0, 30.0, 20000.0, 5.0);
        let result_high = ffdi(35.0, 30.0, 20000.0, 5.0);
        assert!(result_high > result_low, "FFDI should increase with temperature");
    }

    #[test]
    fn test_ffdi_decreases_with_humidity() {
        let result_dry = ffdi(25.0, 20.0, 20000.0, 5.0);
        let result_wet = ffdi(25.0, 80.0, 20000.0, 5.0);
        assert!(result_dry > result_wet, "FFDI should decrease with higher humidity");
    }

    #[test]
    fn test_ffdi_increases_with_wind() {
        let result_calm = ffdi(25.0, 30.0, 5000.0, 5.0);
        let result_windy = ffdi(25.0, 30.0, 50000.0, 5.0);
        assert!(result_windy > result_calm, "FFDI should increase with wind speed");
    }

    #[test]
    fn test_rainfall_effect_below_threshold() {
        // Rain below threshold -> rainfall effect is 1.0 (no reduction)
        let result = rainfall_effect(RAIN_TH - 0.5, 1);
        assert!((result - 1.0).abs() < 1e-5, "Expected 1.0, got {}", result);
    }

    #[test]
    fn test_rainfall_effect_above_threshold() {
        // Rain above threshold and recent -> rainfall effect < 1.0
        let result = rainfall_effect(RAIN_TH + 5.0, 1);
        assert!(result < 1.0, "Rainfall effect should reduce danger, got {}", result);
    }

    #[test]
    fn test_rainfall_effect_age_zero() {
        // age_event == 0 uses effective age of 0.8
        let result = rainfall_effect(RAIN_TH + 5.0, 0);
        assert!(result >= 0.0 && result <= 1.0, "Rainfall effect should be in [0,1], got {}", result);
    }

    #[test]
    fn test_find_rain_events_no_rain() {
        let t0 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2024, 6, 2, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 3, 0, 0, 0).unwrap();
        let dates = vec![t0, t1, t2];
        let daily_rain = vec![0.0, 0.0, 0.0];
        let events = find_rain_events(t2, &dates, &daily_rain);
        assert!(events.is_empty(), "Should find no rain events");
    }

    #[test]
    fn test_find_rain_events_single_event() {
        let t0 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2024, 6, 2, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 3, 0, 0, 0).unwrap();
        let dates = vec![t0, t1, t2];
        // Only day 1 has rain above threshold
        let daily_rain = vec![0.0, RAIN_TH + 5.0, 0.0];
        let events = find_rain_events(t2, &dates, &daily_rain);
        assert_eq!(events.len(), 1, "Should find 1 rain event");
        assert!(events[0].0 > RAIN_TH, "Rain event total should be above threshold");
    }
}
