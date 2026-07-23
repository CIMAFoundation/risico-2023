# RISICO-2023

RISICO  (Rischio Incendi E Coordinamento) is a wildfire risk forecast model written in rust and developed by CIMA Research Foundation. 
It is designed to predict the likelihood and potential impact of wildfires in a given region, given a set of input parameters.

### Project Status
This project is ongoing. We welcome contributions and feedback.

### Compiling and running the model

Compile the production binary using Cargo:

```bash
cargo build --release --bin risico-2023
```

The executable takes a run date, a configuration file, and an input source:

```bash
./target/release/risico-2023 \
  YYYYMMDDHHMM \
  /path/to/configuration.yml \
  /path/to/input
```

The input source is either a text file listing legacy `.zbin` inputs or a
directory containing NetCDF inputs. The tiled streaming pipeline described
below is the only execution mode.

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

## GeoTIFF static data and NetCDF warm state

RISICO can use aligned, single-band GeoTIFF layers as an alternative to the legacy
cell, PPF, and vegetation-ID text files. Legacy configuration remains supported.

```yaml
models:
  - type: RISICO
    model_name: RISICO2023
    model_version: v2025
    static_data:
      type: geotiff
      domain_mask: /opt/risico/static/domain_mask.tif
      slope: /opt/risico/static/slope.tif
      aspect: /opt/risico/static/aspect.tif
      vegetation_id: /opt/risico/static/vegetation_id.tif
      vegetation_catalog: /opt/risico/static/p_vegetazione.csv
      ppf_summer: null
      ppf_winter: null
    warm_state:
      type: netcdf
      directory: /opt/risico/state-nc
      legacy_fallback: /opt/risico/STATE0/state0RISICO_
      max_age_hours: 120
      on_missing: error
    warm_state_hour: 0
    output_time_resolution: 1
    output_types: []
```

The domain mask defines the grid and active cells. All layers must be north-up,
unrotated EPSG:4326 rasters with identical dimensions and affine transforms.
Slope and aspect are stored in degrees. A non-zero, non-nodata mask pixel is
active. PPF layers must either both be configured or both omitted.

NetCDF snapshots store every state field as a georeferenced `y, x` grid. The files
include longitude and latitude coordinate axes, EPSG and affine-transform metadata,
and fill values outside the active domain. On load, state is sampled onto the
configured domain with nearest-neighbour sampling, so a snapshot can survive a
compatible grid-resolution change. A target active cell that maps outside the
snapshot or onto a fill pixel makes that snapshot invalid. Snapshots are also
validated against the model version, written through a temporary file, and
atomically renamed. When `legacy_fallback` is set, the first migrated run may read
a legacy text state and will subsequently write NetCDF snapshots.

FWI uses the same domain-mask and warm-state configuration, without the
RISICO-specific layers:

```yaml
models:
  - type: FWI
    model_name: FWIWORLD
    model_version: legacy
    static_data:
      type: geotiff
      domain_mask: /opt/risico/static/domain_mask.tif
    warm_state:
      type: netcdf
      directory: /opt/risico/state-nc
      legacy_fallback: /opt/risico/STATE0/state0FWI_
      max_age_hours: 120
      on_missing: error
    warm_state_hour: 0
    output_types: []
```

FWI snapshots preserve each cell's complete moisture/rain history as
`history, y, x` grids plus a `history_count` grid. Legacy-model snapshots use one
scalar observation per pixel, matching the deployed `rain ffmc dmc dc` warm-state
representation. The fallback reader accepts both the current five-column history
text format and the deployed four-column scalar format.

Existing legacy snapshots can be converted before cutover. The domain mask defines
the output NetCDF grid and places each text row at its corresponding active pixel.
By default all timestamped files matching the prefix are converted; existing
NetCDF files are skipped unless `--overwrite` is used.

```console
cargo run --bin warm-state-converter -- config \
  --config /opt/risico/configuration.yml \
  --latest-only

# The explicit form is useful outside a migrated deployment:
cargo run --bin warm-state-converter -- risico \
  --legacy-prefix /opt/risico/STATE0/state0RISICO_ \
  --domain-mask /opt/risico/static/domain_mask.tif \
  --output /opt/risico/state-nc \
  --model-version v2025

cargo run --bin warm-state-converter -- fwi \
  --legacy-prefix /opt/risico/STATE0/state0FWI_ \
  --domain-mask /opt/risico/static/domain_mask.tif \
  --output /opt/risico/state-nc \
  --model-version legacy \
  --latest-only
```

## Tiled streaming execution

Tiled streaming is the only production execution path. The former
whole-domain `run_*` loops and the runtime `enabled` switch have been removed.
The same runner is used by RISICO, FWI, Mark5, KBDI, Angstrom, Fosberg,
Nesterov, Sharples, Orieux, and HDW.

### Pipeline

For each configured model, execution proceeds as follows:

1. Build a spatial tile plan.
2. For each tile, load its properties and initial state, map the tile
   coordinates to every meteorological input grid, and process the complete
   input timeline. Processing a tile through the complete timeline keeps only
   that tile's live model state resident.
3. Write each requested native model variable into a plane-oriented,
   little-endian memory-mapped scratch file. There is one scratch file per
   output timestamp.
4. After all tiles are complete, read native variables from mmap on demand,
   resample them onto each configured output grid, and encode NETCDF, ZBIN,
   PNGWJSON, or, in a GDAL-enabled build, GEOTIFF output. Independent output
   variables may be postprocessed in parallel.
5. Assemble scheduled warm-state records in canonical model-cell order and
   write the configured legacy or NetCDF snapshot.
6. Flush and remove completed output scratch files.

Raster-backed models use clipped rectangular windows and omit empty windows.
Models configured with legacy cell files use bounded batches in configured
cell order. Both enter the same runner and model-adapter interface.

Meteorological input grids may be regular or curvilinear. Curvilinear grids
retain nearest-neighbour R-tree lookup. Because source meteorological fields
are normally low resolution, each decoded source field is cached whole and
then sampled repeatedly for the model tiles.

GeoTIFF static layers and gridded warm-state variables are read using bounded
source windows. The current configuration objects retain their compact
active-cell properties and initial warm-state records, while live numerical
state is tile-local. Scheduled warm-state records are currently assembled in
memory before the final writer is called; native forecast output is the
disk-backed portion of the pipeline.

### Configuration

The optional `streaming` section tunes the mandatory tiled runner:

```yaml
streaming:
  tile_height: 512
  tile_width: 512
  cells_per_tile: 262144
  scratch_directory: /var/tmp/risico
```

Defaults are `512 × 512` for raster tiles and 262,144 cells for legacy cell
batches. `scratch_directory` defaults to a `risico-streaming` directory below
the operating system's temporary directory. Tile dimensions and
`cells_per_tile` must be greater than zero.

Output resolution does not affect numerical model execution: interpolation,
clustering, precision rounding, and encoding happen only during the
postprocessing stage.

Allow scratch capacity for approximately four bytes multiplied by active model
cells, configured native variables, and output timestamps, plus filesystem
overhead. Memory mapping lets the operating system page these planes without
materializing every output simultaneously in process memory.

### Running

Run a configuration directly with:

```console
./target/release/risico-2023 \
  202607230000 \
  /share/risico/RISICO2023/configuration.yml \
  /path/to/generated-input-list.txt
```

The configured output and warm-state paths are live destinations. For
development tests, copy the configuration and redirect those paths, plus
`streaming.scratch_directory`, to a temporary directory.

### Inspecting a configuration

`streaming-inspect` builds each model, creates its tile plan, and instantiates
the first tile's property/state adapters without running meteorological input
or writing forecast output:

```console
cargo run --bin streaming-inspect -- \
  202607240000 /opt/risico/configuration.yml \
  --tile-height 256 --tile-width 256
```

The inspector still performs normal configuration and warm-state validation,
so the referenced static layers, palettes, and required warm snapshot must
exist.

### Failure and scratch behavior

Model failures are logged independently so another configured model may
continue. Completed mmap files are flushed and removed after successful
postprocessing. If execution is interrupted or fails before cleanup, the
run-specific scratch directory may remain for diagnosis and can be removed
after confirming that no process is using it.

## Static and warm-state conversion

Convert existing regular-grid static files with the offline GDAL-based utility.
Runtime reading remains pure Rust and does not require GDAL. The converter
detects, per axis, whether legacy bounds represent outer edges or cell centres;
this accommodates the conventions used by the deployed grids.

```console
cargo run --features gdal --bin static-converter -- risico \
  --cells /share/risico/RISICO2023/STATIC/risico2023_input_1km.txt \
  --grid /share/risico/RISICO2023/GRID/input_1km_GRID.txt \
  --output /opt/risico/RISICO2023/STATIC/geotiff

cargo run --features gdal --bin static-converter -- fwi \
  --cells /share/risico/FWIWORLD/STATIC/FWI_world.txt \
  --grid /share/risico/FWIWORLD/GRID/GRID.txt \
  --output /opt/risico/FWIWORLD/STATIC/geotiff
```

Use `--features gdal_bindgen` instead of `--features gdal` when the installed
GDAL version is newer than the bindings bundled by `gdal-sys`.

The converter checks that cells are unique and in the canonical north-to-south,
west-to-east order. This keeps legacy state rows aligned during a progressive
migration.

For `warm-state-converter`, `--legacy-prefix` may name a prefix or a directory
whose files are bare `YYYYMMDDHHMM` timestamps. Every written snapshot is read
back and validated before conversion is reported as successful. A malformed
legacy file is reported without preventing other discovered snapshots from
being checked.





## License
See [LICENSE](LICENSE.md) file
