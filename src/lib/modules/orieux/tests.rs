#[cfg(test)]
mod tests {
    use super::super::functions::fire_class;
    use crate::modules::sharples::functions::{index_fmi, index_f};

    // --- fire_class tests ---

    #[test]
    fn test_fire_class_dry_high_wind() {
        // Low water reserve (<30mm) + high wind (>40km/h) -> class 3 (maximum danger)
        let result = fire_class(10.0, 15.0); // 15 m/s = 54 km/h
        assert_eq!(result, 3.0);
    }

    #[test]
    fn test_fire_class_wet_any_wind() {
        // High water reserve (100-150mm) -> class 0 regardless of wind
        let result = fire_class(120.0, 5.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_fire_class_medium_water_low_wind() {
        // 50-100mm water reserve + low wind (<20km/h) -> class 1
        let result = fire_class(70.0, 3.0); // 3 m/s = 10.8 km/h
        assert_eq!(result, 1.0);
    }

    #[test]
    fn test_fire_class_medium_water_high_wind() {
        // 50-100mm water reserve + high wind (>40km/h) -> class 2
        let result = fire_class(70.0, 15.0); // 15 m/s = 54 km/h
        assert_eq!(result, 2.0);
    }

    #[test]
    fn test_fire_class_30_to_50_low_wind() {
        // 30-50mm water reserve + low wind -> class 1
        let result = fire_class(40.0, 3.0);
        assert_eq!(result, 1.0);
    }

    // --- Sharples index functions (defined in sharples/functions.rs but tested here for coverage) ---

    #[test]
    fn test_index_fmi_typical() {
        // fmi = 10 - 0.25*(temp - humidity) = 10 - 0.25*(30-20) = 7.5
        let result = index_fmi(30.0, 20.0);
        assert!((result - 7.5).abs() < 1e-5, "Expected 7.5, got {}", result);
    }

    #[test]
    fn test_index_fmi_equal_temp_humidity() {
        // temp == humidity -> fmi = 10
        let result = index_fmi(25.0, 25.0);
        assert!((result - 10.0).abs() < 1e-5, "Expected 10.0, got {}", result);
    }

    #[test]
    fn test_index_f_min_wind() {
        // wind_speed = 500 m/h -> ws_kph = 0.5, max(1.0, 0.5) = 1.0
        // f = 1.0 / fmi
        let fmi = 7.5;
        let result = index_f(fmi, 500.0);
        assert!((result - 1.0 / fmi).abs() < 1e-5, "Expected {}, got {}", 1.0 / fmi, result);
    }

    #[test]
    fn test_index_f_high_wind() {
        // wind_speed = 36000 m/h -> ws_kph = 36.0
        // f = 36.0 / fmi
        let fmi = 7.5;
        let result = index_f(fmi, 36000.0);
        assert!((result - 36.0 / fmi).abs() < 1e-4, "Expected {}, got {}", 36.0 / fmi, result);
    }
}
