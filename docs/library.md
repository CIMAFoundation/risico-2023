# Using the Crate

The risico-2023 model can be used as a library in your rust project, it is published on
[crates.io](https://crates.io/crates/risico-2023).
Add the crate to your cargo.toml file or run the following command to add it to your project: 

```bash
cargo add risico-2023
```

## Usage
This is a minimal example of how to use the risico-2023 crate
```rust
// main.rs

use std::{collections::HashMap, sync::Arc};
use chrono::Utc;

// imports
use risico::{
    models::input::{Input, InputElement},
    models::output::OutputVariableName,
    modules::risico::{
        config::RISICOModelConfig,
        models::{
            RISICOProperties, RISICOPropertiesElement, RISICOState, RISICOVegetation,
            RISICOWarmState,
        },
    },
};

fn main() {
    // let's create a single cell with some properties
    let props = RISICOProperties {
        data: vec![RISICOPropertiesElement {
            lon: 0.0,
            lat: 0.0,
            slope: 0.0,
            aspect: 0.0,
            ppf_summer: 1.0,
            ppf_winter: 1.0,
            vegetation: Arc::new(RISICOVegetation::default()),
        }]
        .into(),
        vegetations_dict: HashMap::new(),
        len: 1,
    };

    // and its initial state
    let warm_state = vec![RISICOWarmState {
        dffm: 40.0,
        ..RISICOWarmState::default()
    }];


    // some input data
    let input_data = vec![InputElement {
        temperature: 50.0,
        wind_speed: 20.0,
        humidity: 5.0,
        ..InputElement::default()
    }];

    // let's select the risico model configuration among 'legacy', 'v2023' and 'v2025'
    let config = RISICOModelConfig::new("v2023");
    
    let time = Utc::now();

    // let's create a state 
    let mut state = RISICOState::new(&warm_state, &time, config);

    let input = Input {
        data: input_data.into(),
        time,
    };

    // execute the model
    state.update(&props, &input);

    // get the output
    let output = state.output(&props, &input);

    println!("{:?}", output.get(&OutputVariableName::dffm).unwrap());
}
```
