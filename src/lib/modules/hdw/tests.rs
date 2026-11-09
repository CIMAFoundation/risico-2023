#[cfg(test)]
mod tests {
    use super::super::functions::{hdw, get_output_fn};
    use super::super::models::HdwStateElement;
    use super::super::constants::NODATAVAL;
    use crate::constants::NODATAVAL as OUTPUT_NODATAVAL;

    #[test]
    fn test_hdw_zero_wind() {
        // With zero wind speed, HDW should be 0
        let result = hdw(10.0, 0.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_hdw_typical() {
        // vpd=10 hPa, wind_speed=3600 m/h (=1 m/s) -> hdw = 1.0 * 10.0 = 10.0
        let result = hdw(10.0, 3600.0);
        assert!((result - 10.0).abs() < 1e-4, "Expected ~10.0, got {}", result);
    }

    #[test]
    fn test_hdw_high_conditions() {
        // Higher vpd and wind -> higher HDW
        let result_high = hdw(20.0, 7200.0);
        let result_low = hdw(5.0, 3600.0);
        assert!(result_high > result_low, "Higher vpd/wind should give higher HDW");
    }

    #[test]
    fn test_get_output_fn_valid() {
        let state = HdwStateElement {
            vpd: 10.0,
            wind_speed: 3600.0,
        };
        let output = get_output_fn(&state);
        assert!((output.hdw - 10.0).abs() < 1e-4);
        assert!((output.wind_speed - 1.0).abs() < 1e-5); // converted to m/s
        assert!((output.vpd - 10.0).abs() < 1e-5);
    }

    #[test]
    fn test_get_output_fn_nodata_vpd() {
        let state = HdwStateElement {
            vpd: NODATAVAL,
            wind_speed: 3600.0,
        };
        let output = get_output_fn(&state);
        assert_eq!(output.hdw, OUTPUT_NODATAVAL);
    }

    #[test]
    fn test_get_output_fn_nodata_wind() {
        let state = HdwStateElement {
            vpd: 10.0,
            wind_speed: NODATAVAL,
        };
        let output = get_output_fn(&state);
        assert_eq!(output.hdw, OUTPUT_NODATAVAL);
    }
}
