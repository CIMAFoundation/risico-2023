# Tiled Streaming Execution

Tiled streaming is the only production execution path. The former
whole-domain `run_*` loops and the runtime `enabled` switch have been removed.
The same runner is used by RISICO, FWI, Mark5, KBDI, Angstrom, Fosberg,
Nesterov, Sharples, Orieux, and HDW.

## Pipeline

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
   variables onto each configured output grid, encode NETCDF, ZBIN, ZARR,
   PNGWJSON or GEOTIFF output, and remove the scratch file.
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

## Configuration

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

See [Configuration](configuration.md#outputs) for the output formats.

NetCDF input files and warm-state snapshots are read with the pure-Rust
`netcdf-reader` crate and snapshots are written with `nc4-stream`, so the
build needs neither libnetcdf nor libhdf5.

Allow scratch capacity for approximately four bytes multiplied by active model
cells and configured native variables, plus, when the state is on disk, the
serialized live-state checkpoints and filesystem overhead. An output timestamp is held on disk
only until it is postprocessed and removed; with `overlap_postprocess` that
can be two timestamps at once.

## Running

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

## Inspecting a configuration

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

## Failure and scratch behavior

Model failures are logged independently so another configured model may
continue. Completed output scratch files are removed after successful
postprocessing. If execution is interrupted or fails before cleanup, the
run-specific scratch directory may remain for diagnosis and can be removed
after confirming that no process is using it.
