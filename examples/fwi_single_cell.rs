//! Run the Canadian Fire Weather Index on one cell for five days of hourly
//! weather.
//!
//! ```bash
//! cargo run --example fwi_single_cell
//! ```

use chrono::{Duration, TimeZone, Utc};
use risico::{
    models::{
        input::{Input, InputElement},
        output::OutputVariableName,
    },
    modules::fwi::{
        config::FWIModelConfig,
        models::{FWICellPropertiesContainer, FWIProperties, FWIState, FWIWarmState},
    },
};

fn main() {
    let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();

    let mut properties = FWIProperties::new(FWICellPropertiesContainer {
        lons: vec![8.86],
        lats: vec![44.45],
    });
    // FWI steps at local standard noon: resolve each cell's UTC offset once.
    properties.compute_standard_offsets(&start);

    // A warm state is a short history of codes; start from the standard
    // FFMC 85, DMC 6, DC 15 the day before.
    let warm_state = FWIWarmState {
        dates: vec![start - Duration::days(1)],
        ffmc: vec![85.0],
        dmc: vec![6.0],
        dc: vec![15.0],
        rain: vec![0.0],
    };
    let mut state = FWIState::new(&[warm_state], &start, FWIModelConfig::new("legacy"));

    for hour in 1..=5 * 24 {
        let time = start + Duration::hours(hour);
        let weather = InputElement {
            temperature: 28.0,
            humidity: 30.0,
            wind_speed: 5.0 * 3600.0, // m/h
            rain: 0.0,
            ..InputElement::default()
        };
        state.update(&properties, &Input { time, data: vec![weather].into() });

        if hour % 24 == 12 {
            let output = state.output(&properties);
            let value = |name| output.get(&name).unwrap()[0];
            println!(
                "{time}  FFMC {:5.1}  DMC {:5.1}  DC {:6.1}  ISI {:4.1}  BUI {:5.1}  FWI {:4.1}",
                value(OutputVariableName::ffmc),
                value(OutputVariableName::dmc),
                value(OutputVariableName::dc),
                value(OutputVariableName::isi),
                value(OutputVariableName::bui),
                value(OutputVariableName::fwi),
            );
        }
    }
}
