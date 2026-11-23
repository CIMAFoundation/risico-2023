//! Run RISICO on one cell for two days of synthetic hourly weather.
//!
//! ```bash
//! cargo run --example risico_single_cell
//! ```

use std::{collections::HashMap, f32::consts::PI, sync::Arc};

use chrono::{Duration, TimeZone, Utc};
use risico::{
    models::{
        input::{Input, InputElement},
        output::OutputVariableName,
    },
    modules::risico::{
        config::RISICOModelConfig,
        models::{
            RISICOCellPropertiesContainer, RISICOProperties, RISICOState, RISICOVegetation,
            RISICOWarmState,
        },
    },
};

fn main() {
    // One cell: position, terrain and vegetation. The vegetation parameters
    // normally come from the catalogue CSV; the default is a generic fuel.
    let vegetation = Arc::new(RISICOVegetation::default());
    let properties = RISICOProperties::new(
        RISICOCellPropertiesContainer {
            lons: vec![8.86],
            lats: vec![44.45],
            slopes: vec![5.0_f32.to_radians()],
            aspects: vec![160.0_f32.to_radians()],
            vegetations: vec![vegetation.clone()],
        },
        HashMap::from([(vegetation.id.clone(), vegetation)]),
        vec![1.0], // summer PPF
        vec![1.0], // winter PPF
    );

    // Start from 40 % fine fuel moisture, the default warm state.
    let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    let mut state = RISICOState::new(&[RISICOWarmState::default()], &start, RISICOModelConfig::new("v2025"));

    for hour in 1..=48 {
        let time = start + Duration::hours(hour);
        let phase = 2.0 * PI * ((hour as f32 - 15.0) / 24.0);
        // Units are the library's own: wind speed in m/h, direction in radians.
        let weather = InputElement {
            temperature: 26.0 + 6.0 * phase.cos(),
            humidity: 45.0 - 20.0 * phase.cos(),
            wind_speed: 4.0 * 3600.0,
            wind_dir: 270.0_f32.to_radians(),
            rain: 0.0,
            snow_cover: 0.0,
            ..InputElement::default()
        };
        let input = Input { time, data: vec![weather].into() };

        state.update(&properties, &input);
        if hour % 6 == 0 {
            let output = state.output(&properties, &input);
            let value = |name| output.get(&name).unwrap()[0];
            println!(
                "{time}  dffm {:5.1} %  V {:6.1} m/h  I {:7.1} kW/m",
                value(OutputVariableName::dffm),
                value(OutputVariableName::V),
                value(OutputVariableName::I),
            );
        }
    }
}
