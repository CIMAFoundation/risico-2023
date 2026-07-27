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

## GeoTIFF static data and NetCDF warm state

RISICO and FWI configure their static data and warm state as aligned, single-band
GeoTIFF layers and NetCDF snapshots. This is the only supported layout: the older
cell/PPF/vegetation-ID text files and plain-text warm-state files are no longer
readable directly by the model binary, and configuration files must be YAML (the
older whole-file `.txt` key/value format is no longer supported). Existing
deployments convert once with `static-converter` and `warm-state-converter`
(below) before switching their configuration over.

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
atomically renamed. A run with no valid NetCDF snapshot follows `on_missing`
directly: `defaults` seeds the run with default state, `error` stops the run.
There is no automatic fallback to a legacy text state; convert existing legacy
warm state once with `warm-state-converter` (below) before switching a
deployment over.

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
  --model-name RISICO2023 \
  --legacy-prefix /opt/risico/STATE0/state0RISICO_ \
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
2. Process the input timeline in timestamp order.
3. Within each timestamp, load one tile's properties and its exact live-state
   checkpoint, map its input coordinates, and run the model with the existing
   in-tile parallelism.
4. Write the tile's requested native variables into the current timestamp's
   plane-oriented, little-endian memory-mapped output, checkpoint its updated
   state, and drop all tile-local allocations before loading the next tile.
5. After all tiles complete that timestamp, flush the mmap, resample its native
   variables onto each configured output grid, encode NETCDF, ZBIN, PNGWJSON,
   or, in a GDAL-enabled build, GEOTIFF output, and remove the scratch file.
   Independent output variables may be postprocessed in parallel.
6. Assemble any scheduled warm-state records in canonical model-cell order and
   write the configured legacy or NetCDF snapshot before advancing to the next
   timestamp.

Raster-backed models use clipped rectangular windows and omit empty windows.
Models configured with legacy cell files use bounded batches in configured
cell order. Both enter the same runner and model-adapter interface.

Meteorological input grids may be regular or curvilinear. Curvilinear grids
retain nearest-neighbour R-tree lookup. Because source meteorological fields
are normally low resolution, each decoded source field is cached whole and
then sampled repeatedly for the model tiles.

GeoTIFF static layers and gridded warm-state variables are read using bounded
source windows. The current configuration objects retain their compact
active-cell properties and initial warm-state records. Live numerical state is
retained only for the tile currently being processed; exact per-tile state,
including transient accumulators and history, is checkpointed between timestamps.
Decoded meteorological fields are retained while all tiles consume the current
timestamp and then released. Scheduled warm-state
records for the current timestamp are assembled in memory before the writer is
called; native forecast output is the disk-backed portion of the pipeline.

### Configuration

The optional `streaming` section tunes the mandatory tiled runner:

```yaml
streaming:
  tile_height: 1024
  tile_width: 1024
  cells_per_tile: 1048576
  scratch_directory: /var/tmp/risico
```

Defaults are `1024 × 1024` for raster tiles and 1,048,576 cells for legacy cell
batches. `scratch_directory` defaults to a `risico-streaming` directory below
the operating system's temporary directory. Tile dimensions and
`cells_per_tile` must be greater than zero.

Output resolution does not affect numerical model execution: interpolation,
clustering, precision rounding, and encoding happen only during the
postprocessing stage.

Allow scratch capacity for approximately four bytes multiplied by active model
cells and configured native variables, plus the serialized live-state
checkpoints and filesystem overhead. Only the current output timestamp is
mapped; it is postprocessed and removed before advancing to the next timestamp.

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
