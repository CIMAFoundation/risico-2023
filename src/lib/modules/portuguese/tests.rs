#[cfg(test)]
mod tests {
    use super::super::functions::{ignition_index, get_output_fn};
    use super::super::models::PortugueseStateElement;

    #[test]
    fn test_ignition_index_typical() {
        // temp=25, dew=15 -> ign = 25*(25-15) = 250
        let result = ignition_index(25.0, 15.0);
        assert!((result - 250.0).abs() < 1e-4, "Expected 250.0, got {}", result);
    }

    #[test]
    fn test_ignition_index_zero_diff() {
        // temp == dew -> ign = 0
        let result = ignition_index(20.0, 20.0);
        assert!((result - 0.0).abs() < 1e-5, "Expected 0.0, got {}", result);
    }

    #[test]
    fn test_ignition_index_negative() {
        // dew > temp -> negative ignition index (very moist air)
        let result = ignition_index(10.0, 15.0);
        assert!(result < 0.0, "Expected negative ignition index, got {}", result);
    }

    #[test]
    fn test_ignition_index_increases_with_temp() {
        let low = ignition_index(20.0, 10.0);
        let high = ignition_index(35.0, 10.0);
        assert!(high > low, "Ignition index should increase with temperature");
    }

    #[test]
    fn test_get_output_fn_valid() {
        let state = PortugueseStateElement {
            ign: 250.0,
            fire_index: 300.0,
            temp_12: 25.0,
            temp_dew_12: 15.0,
            cum_rain: 0.0,
            cum_index: 50.0,
            sum_ign: 250.0,
        };
        let output = get_output_fn(&state);
        assert!((output.portuguese_ignition - 250.0).abs() < 1e-4);
        assert!((output.portuguese_fdi - 300.0).abs() < 1e-4);
        assert!((output.temperature - 25.0).abs() < 1e-4);
    }
}
