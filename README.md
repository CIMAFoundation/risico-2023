# RISICO-2023

RISICO  (Rischio Incendi E Coordinamento) is a wildfire risk forecast model written in rust and developed by CIMA Research Foundation. 
It is designed to predict the likelihood and potential impact of wildfires in a given region, given a set of input parameters.

### Project Status
This project is ongoing. We welcome contributions and feedback.

### Compiling and running the model

Compile the production binary using Cargo. No native NetCDF or HDF5
libraries are needed: NetCDF I/O is pure Rust. Only the optional
`static-converter` (feature `gdal`) links a C library, GDAL.

```bash
cargo build --release --bin risico-2023
```

The release workflow publishes a fully static `x86_64-unknown-linux-musl`
binary that runs on any x86_64 Linux regardless of the glibc version. The same
build works locally on Linux with the `musl-tools` package installed:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --bin risico-2023 --target x86_64-unknown-linux-musl
```

`./build-with-docker.sh` still builds a glibc-linked binary inside an Ubuntu
16.04 container when one is needed.

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

FWI model versions:

- `cffdrs`: the canonical daily Canadian FWI System (Van Wagner 1987; equations
  and constants of the cffdrs R package, `src/lib/modules/fwi/cffdrs.rs`, tested
  against its published fixtures). The daily step runs at each cell's local
  *standard* noon (tzf-rs time zone, IANA rules, no DST), with
  temperature, humidity and wind interpolated linearly between the input steps
  around noon and the rain accumulated since the previous noon. The snapshot
  keeps the codes and the rain accumulated since the last noon, so a restart is
  exact. Write one map per day after the latest noon of the domain with
  `output_time_resolution: 24` and `output_time_offset` (hours from the run
  date, e.g. `12` for domains whose standard offsets are all >= 0).
- `legacy`: the port used by FWI-ITALIA-ICON/IFS (civil-time noon, daily step
  computed when the output is written; kept unchanged).
- `sliding_window`: rolling 24 h windows.

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

1. Build a spatial tile plan, gather every tile's immutable properties once,
   seed every tile's live state from the warm state, and register each tile's
   input coordinates. The whole-domain property and warm-state arrays are
   released at this point.
2. Process the input timeline in timestamp order, decoding each timestamp's
   source fields once.
3. Within each timestamp, run every tile through the model: its state is
   either kept in memory or restored from its checkpoint, as `tile_state`
   decides.
4. Write the tile's requested native variables into the current timestamp's
   plane-oriented, little-endian output scratch file and, when the state is
   on disk, checkpoint its updated state.
5. After all tiles complete that timestamp, resample its native
   variables onto each configured output grid, encode NETCDF, ZBIN, PNGWJSON,
   or, in a GDAL-enabled build, GEOTIFF output, and remove the scratch file.
   Independent output variables may be postprocessed in parallel, and the
   whole step may run in the background while the next timestamp is stepped.
6. Assemble any scheduled warm-state records in canonical model-cell order and
   write the configured legacy or NetCDF snapshot, in the same background
   step as the output.

Raster-backed models use clipped rectangular windows and omit empty windows.
A window holding fewer than a quarter of a full tile's cells, as coastal and
border windows do, is merged with a neighbour in its tile row, never beyond a
full tile's worth of cells. Tiles are then ordered largest first so the
work-stealing pool spends the tail of each timestep on the small ones.
Models configured with legacy cell files use bounded batches in configured
cell order. Both enter the same runner and model-adapter interface.

Meteorological input grids may be regular or curvilinear. Curvilinear grids
retain nearest-neighbour R-tree lookup. Because source meteorological fields
are normally low resolution, each decoded source field is cached whole and
then sampled repeatedly for the model tiles.

GeoTIFF static layers and gridded warm-state variables are read using bounded
source windows. The current configuration objects retain their compact
active-cell properties and initial warm-state records; the warm state is
released once every tile has been seeded from it. Exact per-tile live state,
including transient accumulators and history, either stays in memory for the
whole run or is checkpointed between timestamps, as `tile_state` decides.
Decoded meteorological fields are retained while all tiles consume the current
timestamp and then released. Scheduled warm-state
records for the current timestamp are assembled in memory before the writer is
called; native forecast output is the disk-backed portion of the pipeline.

### Configuration

Paths in `configuration.yml` may be relative. They are resolved against the
directory that holds the configuration file, not the working directory, so a
chain directory can describe itself without repeating its absolute location.
Absolute paths are used as they are. Palettes can come from one shared mapping
file, whose own relative paths are resolved against its directory. Entries
under `palettes` win over that file:

```yaml
models:
  - type: RISICO
    static_data:
      type: geotiff
      domain_mask: STATIC/geotiff/domain_mask.tif   # <chain>/STATIC/...
    warm_state:
      type: netcdf
      directory: STATE0-NC
    output_types:
      - path: OUTPUT-NC
        grid_path: GRID/GRID.txt
        # ...
palette_file: ../PALETTE/palettes.yml   # V: RISICO_V.pal, I: RISICO_I.pal, ...
```

The optional `streaming` section tunes the mandatory tiled runner:

```yaml
streaming:
  tile_height: 1024
  tile_width: 1024
  cells_per_tile: 1048576
  max_memory: 6GB
  tile_state: auto
  overlap_postprocess: true
  scratch_directory: /var/tmp/risico
```

Defaults are `512 × 512` for raster tiles and 1,048,576 cells for legacy cell
batches. `scratch_directory` defaults to a `risico-streaming` directory below
the operating system's temporary directory. Tile dimensions and
`cells_per_tile` must be greater than zero.

`max_memory` (for example `6GB` or `768 MiB`) is a target peak resident size.
Tile concurrency is derived from what is left of it once the whole-domain
arrays are charged, so the run adapts to the model and the domain; an explicit
`tile_concurrency` wins over it. With neither set, one tile runs per core.

`tile_state` says where each tile's live state lives between timestamps:

- `auto` (default): in memory when `max_memory` has room for every tile's
  state at the concurrency the run will use, or when no `max_memory` is set;
  on disk otherwise.
- `memory`: always in memory. No checkpoint is written or read, which removes
  the largest I/O cost of a run.
- `disk`: always checkpointed to the scratch directory, so only the tiles in
  flight hold state. This is the smallest footprint.

`overlap_postprocess` says whether a timestamp's output is resampled and
written, and its warm state saved, while the next timestamp is being stepped.
Unset, the overlap is taken when the output grids in flight and the gathered
warm state still fit `max_memory` beside a step, or when no budget is set;
`true` and `false` force it. At most one timestamp is finished in the
background, so a failure there stops the run at the next timestamp.

Each model logs the decision it made and the peak it expects before the
timeline starts, then logs per-timestamp phase timings (preload, step,
postprocess, warm state) with the current and peak resident set size, and a
total per phase at the end. `streaming-inspect` prints the same decision for
a configuration without running it.

Output resolution does not affect numerical model execution: interpolation,
clustering, precision rounding, and encoding happen only during the
postprocessing stage.

Output types are written as `ZBIN`, `NETCDF`, `ZARR`, `PNGWJSON` or, in a
GDAL-enabled build, `GEOTIFF`; any other `format` is a configuration error.
`ZARR` writes one Zarr v3 store per variable (`<path>/<name>.zarr`) with the
same `time`, `latitude` and `longitude` axes as the NetCDF output, one
zstd-compressed chunk per timestamp, replacing any store left by an earlier
run. Zarr stores take no global lock, so a timestamp's variables are
compressed in parallel.

NETCDF output types accept a `compression_level` (0 to 9, default 7); for
`ZARR` the same key is the zstd level (0 to 22). NetCDF-4 files are written
by the in-repo `nc4-stream` crate (`crates/nc4-stream`), a pure-Rust
streaming writer: each timestamp's grid is shuffled and deflated on the
thread that produced it and appended to the file as one chunk, so the
variables of a timestamp compress in parallel and an open output file costs
one chunk of memory however long the timeline is. The chunk index and the
rest of the file metadata are written when the run ends, so output files
are readable only after a run finishes or fails cleanly. The files have the
same structure netcdf-c writes (unlimited `time`, one chunk per timestamp,
dimension scales), so ncdump, xarray and GDAL read them as before.

NetCDF input files and warm-state snapshots are read with the pure-Rust
`netcdf-reader` crate and snapshots are written with `nc4-stream`, so the
build needs neither libnetcdf nor libhdf5.

Allow scratch capacity for approximately four bytes multiplied by active model
cells and configured native variables, plus, when the state is on disk, the
serialized live-state checkpoints and filesystem overhead. An output timestamp is held on disk
only until it is postprocessed and removed; with `overlap_postprocess` that
can be two timestamps at once.

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
continue. Completed output scratch files are removed after successful
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
GDAL version has no bindings bundled by `gdal-sys`. This requires libclang and
does not guarantee compatibility with newer GDAL APIs: `gdal` 0.19 does not
compile against GDAL 3.13. Use a supported GDAL version (for example, 3.6.2)
with `--features gdal` in that case. When GDAL is outside the system development
paths, configure `GDAL_LIB_DIR` and `GDAL_VERSION`, or point `PKG_CONFIG_PATH`
to its `gdal.pc`.

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
