# Programmatic Workflow

The models are a Rust library, `risico`, separate from the `risico-2023` runner: every index
is a pure computation on arrays of cells, with no files involved. Use it to embed RISICO in
another program, run it on cells you already hold in memory, or test a formulation. This
guide shows the model API; the full item-by-item documentation is in the
[API Reference](api-reference.md).

## Add the dependency

The library is published as the `risico-2023` crate. Without the default `build-binary`
feature it leaves out the runner's dependencies (NetCDF, GeoTIFF, Zarr, CLI):

```toml
[dependencies]
risico-2023 = { git = "https://github.com/CIMAFoundation/risico-2023", branch = "dev", default-features = false }
chrono = "0.4"
```

The crate is imported as `risico`. Add the `config` feature for the `configuration.yml`
schema types (`risico::config`).

## The model API

Every model in `risico::modules` is made of the same pieces:

| Piece | Type | What it holds |
| --- | --- | --- |
| Properties | `RISICOProperties`, `FWIProperties`, ... | What does not change during a run, one element per cell: coordinates, and for RISICO slope, aspect, vegetation and PPF |
| Warm state | `RISICOWarmState`, `FWIWarmState`, ... | The per-cell memory a run starts from and saves at its end |
| State | `RISICOState`, `FWIState`, ... | The running state, created from the warm state and a start time |
| Input | `models::input::Input` | The weather of one timestamp on every cell |
| Output | `models::output::Output` | The variables computed at one timestamp, read by `OutputVariableName` |

Properties, states and inputs are aligned arrays: element `i` of each is the same cell.

### Units

The library has its own units; the runner converts the input files to them:

| Quantity | Unit |
| --- | --- |
| Temperature | °C |
| Relative humidity | % |
| Wind speed | **m/h** |
| Wind direction | **radians**, the direction the wind comes from |
| Rain | mm |
| Snow cover | cm |
| Slope, aspect | **radians** |

Missing values are `risico::constants::NODATAVAL`. `InputElement::default()` fills every
field with it, so set the variables a model needs and leave the others.

### Stepping each model

The runner drives each model with the same calls in every tile; follow the same order:

| Model | Every timestamp | When an output is wanted |
| --- | --- | --- |
| RISICO | `state.update(&props, &input)` | `state.output(&props, &input)` |
| FWI | `state.update(&props, &input)` | `state.output(&props)` |
| Mark 5 | `state.store(&input, &props)` | `state.output(&props)`, once a day |
| KBDI, Orieux | `state.store(&input)` | `state.update(&props)` then `state.output()`, once a day |
| Nesterov | `state.store(&input, &props)` | `state.update()` then `state.output()`, once a day |
| Angström, Fosberg, Sharples, HDW | `state.store(&input)` | `state.output()` |

FWI steps at local standard noon: call `FWIProperties::compute_standard_offsets` once
before the run.

## RISICO on one cell

`examples/risico_single_cell.rs` runs RISICO for two days of synthetic weather and prints
the fine fuel moisture, rate of spread and intensity every six hours:

```rust
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
```

```console
$ cargo run --release --example risico_single_cell
2026-07-01 06:00:00 UTC  dffm  35.7 %  V    7.1 m/h  I    10.6 kW/m
2026-07-01 12:00:00 UTC  dffm  30.8 %  V   17.7 m/h  I    28.7 kW/m
2026-07-01 18:00:00 UTC  dffm  25.7 %  V   41.4 m/h  I    73.3 kW/m
...
2026-07-03 00:00:00 UTC  dffm  12.7 %  V  139.6 m/h  I   298.9 kW/m
```

`RISICOModelConfig::new` selects the formulation: `legacy`, `v2023` or `v2025`.

### Vegetation

`RISICOVegetation` holds one row of the vegetation catalogue (`p_vegetazione.csv`):

| Field | Meaning |
| --- | --- |
| `id`, `name` | Identifier in the vegetation-id layer, description |
| `d0`, `d1` | Dead fine fuel load, live fuel load |
| `hhv` | Higher heating value [kJ/kg] |
| `umid` | Live fuel moisture |
| `v0` | Base rate of spread [m/h] |
| `T0` | Response time of the dead fine fuel moisture [h] |
| `sat` | Saturation moisture of the dead fine fuel [%] |
| `use_ndvi` | Whether satellite NDVI modulates the live fuel |

## FWI on one cell

`examples/fwi_single_cell.rs` runs the Canadian FWI System for five days of hourly weather
and prints the codes at noon:

```rust
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
```

```console
$ cargo run --release --example fwi_single_cell
2026-07-01 12:00:00 UTC  FFMC  91.8  DMC  10.8  DC   23.7  ISI 13.8  BUI  10.7  FWI 13.5
2026-07-02 12:00:00 UTC  FFMC  92.4  DMC  15.6  DC   32.5  ISI 15.0  BUI  15.5  FWI 17.0
...
2026-07-05 12:00:00 UTC  FFMC  92.6  DMC  29.9  DC   58.7  ISI 15.3  BUI  29.8  FWI 23.3
```

## Many cells

Nothing changes for a grid: pass one element per cell in every vector, and one
`InputElement` per cell in each `Input`. `update` and `output` process the cells in parallel
with `ndarray`'s `Zip`. The `risico-2023` runner does exactly this on tiles of the domain,
so memory stays bounded however large the domain is (see
[Large Domains and Performance](execution.md)).

To save and restore a run, map the state's cells back to warm-state records (for RISICO,
the `dffm`, snow and satellite fields of each `RISICOStateElement`) and pass them to
`State::new` next time.
