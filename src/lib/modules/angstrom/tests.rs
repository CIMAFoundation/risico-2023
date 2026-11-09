#[cfg(test)]
mod tests {
    use super::super::functions::{angstrom_index, get_output_fn};
    use super::super::models::AngstromStateElement;
    use super::super::constants::NODATAVAL;
    use crate::constants::NODATAVAL as OUTPUT_NODATAVAL;

    #[test]
    fn test_angstrom_index_typical() {
        // humidity=40%, temp=20°C -> (40/20) + (27-20)/10 = 2.0 + 0.7 = 2.7
        let result = angstrom_index(20.0, 40.0);
        assert!((result - 2.7).abs() < 1e-5, "Expected ~2.7, got {}", result);
    }

    #[test]
    fn test_angstrom_index_high_fire_risk() {
        // Low humidity and high temperature -> low angstrom index (high risk)
        let result = angstrom_index(35.0, 10.0);
        // (10/20) + (27-35)/10 = 0.5 + (-0.8) = -0.3
        assert!((result - (-0.3)).abs() < 1e-5, "Expected ~-0.3, got {}", result);
    }

    #[test]
    fn test_angstrom_index_low_fire_risk() {
        // High humidity and low temperature -> high angstrom index (low risk)
        let result = angstrom_index(5.0, 80.0);
        // (80/20) + (27-5)/10 = 4.0 + 2.2 = 6.2
        assert!((result - 6.2).abs() < 1e-5, "Expected ~6.2, got {}", result);
    }

    #[test]
    fn test_get_output_fn_valid() {
        let state = AngstromStateElement {
            temp: 20.0,
            humidity: 40.0,
        };
        let output = get_output_fn(&state);
        assert!((output.angstrom - 2.7).abs() < 1e-5);
        assert!((output.temperature - 20.0).abs() < 1e-5);
        assert!((output.humidity - 40.0).abs() < 1e-5);
    }

    #[test]
    fn test_get_output_fn_nodata_temp() {
        let state = AngstromStateElement {
            temp: NODATAVAL,
            humidity: 40.0,
        };
        let output = get_output_fn(&state);
        // Should return default (NODATAVAL) output when data is missing
        assert_eq!(output.angstrom, OUTPUT_NODATAVAL);
    }

    #[test]
    fn test_get_output_fn_nodata_humidity() {
        let state = AngstromStateElement {
            temp: 20.0,
            humidity: NODATAVAL,
        };
        let output = get_output_fn(&state);
        assert_eq!(output.angstrom, OUTPUT_NODATAVAL);
    }
}
