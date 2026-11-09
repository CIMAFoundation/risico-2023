#[cfg(test)]
mod tests {
    use super::super::functions::{index_fmi, index_f, get_output_fn};
    use super::super::models::SharplesStateElement;
    use super::super::constants::NODATAVAL;
    use crate::constants::NODATAVAL as OUTPUT_NODATAVAL;

    #[test]
    fn test_fmi_typical() {
        // fmi = 10 - 0.25*(temp - humidity) = 10 - 0.25*(30-20) = 7.5
        let result = index_fmi(30.0, 20.0);
        assert!((result - 7.5).abs() < 1e-5, "Expected 7.5, got {}", result);
    }

    #[test]
    fn test_fmi_equal_temp_humidity() {
        // temp == humidity -> fmi = 10.0
        let result = index_fmi(25.0, 25.0);
        assert!((result - 10.0).abs() < 1e-5, "Expected 10.0, got {}", result);
    }

    #[test]
    fn test_fmi_high_temp_low_humidity() {
        // High temp, low humidity -> lower fmi (drier fuel)
        let result = index_fmi(40.0, 10.0);
        // fmi = 10 - 0.25*(40-10) = 10 - 7.5 = 2.5
        assert!((result - 2.5).abs() < 1e-5, "Expected 2.5, got {}", result);
    }

    #[test]
    fn test_index_f_clamps_low_wind() {
        // wind_speed < 1000 m/h -> ws_kph < 1.0 -> max(1.0, ws) = 1.0
        let fmi = 5.0;
        let result = index_f(fmi, 500.0);
        assert!((result - 1.0 / fmi).abs() < 1e-5, "Expected {}, got {}", 1.0 / fmi, result);
    }

    #[test]
    fn test_index_f_high_wind() {
        // wind_speed = 72000 m/h = 72 km/h -> f = 72 / fmi
        let fmi = 8.0;
        let result = index_f(fmi, 72000.0);
        assert!((result - 9.0).abs() < 1e-4, "Expected 9.0, got {}", result);
    }

    #[test]
    fn test_get_output_fn_valid() {
        let state = SharplesStateElement {
            temp: 30.0,
            humidity: 20.0,
            wind_speed: 36000.0, // 36 km/h = 10 m/s
        };
        let output = get_output_fn(&state);
        // fmi = 10 - 0.25*(30-20) = 7.5
        assert!((output.fmi - 7.5).abs() < 1e-5);
        // f = 36 / 7.5 = 4.8
        assert!((output.f - 4.8).abs() < 1e-4);
        // wind speed should be converted to m/s: 36000/3600 = 10 m/s
        assert!((output.wind_speed - 10.0).abs() < 1e-4);
    }

    #[test]
    fn test_get_output_fn_nodata_temp() {
        let state = SharplesStateElement {
            temp: NODATAVAL,
            humidity: 20.0,
            wind_speed: 36000.0,
        };
        let output = get_output_fn(&state);
        assert_eq!(output.fmi, OUTPUT_NODATAVAL);
    }

    #[test]
    fn test_get_output_fn_nodata_wind() {
        let state = SharplesStateElement {
            temp: 30.0,
            humidity: 20.0,
            wind_speed: NODATAVAL,
        };
        let output = get_output_fn(&state);
        assert_eq!(output.fmi, OUTPUT_NODATAVAL);
    }
}
