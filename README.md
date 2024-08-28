# RISICO-2023

RISICO  (Rischio Incendi E Coordinamento) is a wildfire risk forecast model written in rust and developed by CIMA Research Foundation. 
It is designed to predict the likelihood and potential impact of wildfires in a given region, given a set of input parameters.

### Project Status
This project is ongoing. We welcome contributions and feedback.

### Compiling and Running the Model
To compile and run the model, you will need to have rust installed on your machine.


Compile the project using cargo:
```bash
cargo build
```
Run the model using cargo:
```bash
cargo run
```

## Using risico-2023 as library
The risico-2023 model can be used as a library in your rust project, it is published on
[crates.io](https://crates.io/crates/risico-2023).
Add the crate to your cargo.toml file or run the following command to add it to your project: 

```bash
cargo add risico-2023
```

#### Usage
This is a minimal example of how to use the risico-2023 crate
```rust
// main.rs

use std::{collections::HashMap, sync::Arc};
use chrono::Utc;

// imports
use risico::modules::risico::{
    config::ModelConfig,
    models::{
        Input, InputElement, OutputVariableName, Properties, PropertiesElement, State, Vegetation,
        WarmState,
    },
};

fn main() {
    // let's create a single cell with some properties
    let props = Properties {
        data: vec![PropertiesElement {
            lon: 0.0,
            lat: 0.0,
            slope: 0.0,
            aspect: 0.0,
            ppf_summer: 1.0,
            ppf_winter: 1.0,
            vegetation: Arc::new(Vegetation::default()),
        }]
        .into(),
        vegetations_dict: HashMap::new(),
        len: 1,
    };

    // and its initial state
    let warm_state = vec![WarmState {
        dffm: 40.0,
        ..WarmState::default()
    }];


    // some input data
    let input_data = vec![InputElement {
        temperature: 50.0,
        wind_speed: 20.0,
        humidity: 5.0,
        ..InputElement::default()
    }];

    // let's select the risico model configuration between 'legacy' and 'v2023'
    let config = ModelConfig::new("v2023");
    
    let time = Utc::now();

    // let's create a state 
    let mut state = State::new(&warm_state, &time, config);

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





## License
See [LICENSE](LICENSE) file