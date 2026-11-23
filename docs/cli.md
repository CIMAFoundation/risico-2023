# CLI Usage

The repository builds four commands:

| Command | Purpose |
| --- | --- |
| `risico-2023` | Run the models of a configuration on a set of weather inputs |
| `streaming-inspect` | Validate a configuration and print its tile plan, without running |
| `static-converter` | Convert the text static data of the previous RISICO implementation to GeoTIFF layers |
| `warm-state-converter` | Convert the text warm states of the previous RISICO implementation to NetCDF snapshots |

Most users need only `risico-2023`. The two converters are for existing installations of the
previous RISICO implementation (see [Converting Legacy Data](conversion.md)).

```bash
cargo build --release --bins    # builds all four in target/release/
```

Every command prints its usage with `--help`.

## `risico-2023`

```text
Usage: risico-2023 <DATE> <CONFIG_PATH> <INPUT_PATH>

Arguments:
  <DATE>         Model date in the format YYYYMMDDHHMM
  <CONFIG_PATH>  Path to the configuration file
  <INPUT_PATH>   Path to the input data file

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Arguments

`DATE`
:   The run date, in UTC. Each model starts from the warm-state snapshot of the day before
    (see [Configuration](configuration.md#warm-state)), and output times are counted from it.
    Every timestamp of the inputs is processed, so give inputs that start after that
    snapshot.

`CONFIG_PATH`
:   A YAML configuration: the models to run, their static data, warm state and outputs
    (see [Configuration](configuration.md)).

`INPUT_PATH`
:   Either a **directory** of NetCDF weather files (every `.nc` file in it is read, see
    [Input Data](input-data.md#weather-inputs)) or a **text file** listing `.zbin` files of the
    previous RISICO implementation, one per line.

### Example

```bash
./target/release/risico-2023 \
  202607010000 \
  /opt/risico/configuration.yml \
  /data/weather/20260701/
```

A run reads the static data and warm state of every model, then streams the inputs one
timestamp at a time through each tile of each model, writing outputs at the configured
hours and the warm state at `warm_state_hour` (see
[Large Domains and Performance](execution.md)).

### Environment

| Variable | Default | Effect |
| --- | --- | --- |
| `RUST_LOG` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace`. Logs go to standard error. |
| `TMPDIR` | `/tmp` | Base of the scratch directory when the configuration sets none. |

The command exits with a non-zero status and an error message when the configuration,
the static data or the inputs cannot be read.

## `streaming-inspect`

Checks a configuration without reading any weather: it loads the static data, resolves
every model and prints the tile plan the runner would use. Useful before deploying a new
configuration, or to try other tile sizes.

```text
Usage: streaming-inspect [OPTIONS] <DATE> <CONFIG>

Arguments:
  <DATE>    Model date in YYYYMMDDHHMM form
  <CONFIG>  YAML or legacy text configuration

Options:
      --tile-height <TILE_HEIGHT>        Override raster tile height
      --tile-width <TILE_WIDTH>          Override raster tile width
      --cells-per-tile <CELLS_PER_TILE>  Override legacy cells per tile
```

```bash
./target/release/streaming-inspect 202607010000 configuration.yml --tile-height 256 --tile-width 256
```

## `static-converter`

Converts the legacy regular-grid static files to the aligned GeoTIFF layers the model
reads (see [Converting Legacy Data](conversion.md)).

```text
Usage: static-converter risico [OPTIONS] --cells <CELLS> --grid <GRID> --output <OUTPUT>
      --cells <CELLS>    RISICO cell file: lon lat slope aspect vegetation
      --grid <GRID>      Legacy grid file (GRIDNROWS=, MINLAT=, ...)
      --output <OUTPUT>  Destination directory for the GeoTIFF layers
      --ppf <PPF>        Optional legacy two-column summer/winter PPF file

Usage: static-converter fwi --cells <CELLS> --grid <GRID> --output <OUTPUT>
      --cells <CELLS>    FWI cell file: lon lat
      --grid <GRID>      Legacy grid file
      --output <OUTPUT>  Destination directory for the domain mask
```

## `warm-state-converter`

Converts legacy text warm states to NetCDF snapshots, validated against a domain mask.

```text
Usage: warm-state-converter config [OPTIONS] --config <CONFIG> --model-name <MODEL_NAME> --legacy-prefix <LEGACY_PREFIX>
      --config <CONFIG>                Migrated YAML configuration containing GeoTIFF and NetCDF warm-state settings
      --model-name <MODEL_NAME>        `model_name` of the model entry to convert, as it appears in the config file
      --legacy-prefix <LEGACY_PREFIX>  Legacy filename prefix, or a directory when files are named only by timestamp
      --latest-only                    Convert only the newest legacy snapshot
      --overwrite                      Replace an existing NetCDF snapshot with the same timestamp

Usage: warm-state-converter risico|fwi [OPTIONS] --legacy-prefix <LEGACY_PREFIX> --domain-mask <DOMAIN_MASK> --output <OUTPUT> --model-version <MODEL_VERSION>
      --legacy-prefix <LEGACY_PREFIX>  Legacy filename prefix, or a directory when files are named only by timestamp
      --domain-mask <DOMAIN_MASK>      GeoTIFF domain mask used to validate cell count and define the output grid
      --output <OUTPUT>                Destination directory for NetCDF snapshots
      --model-version <MODEL_VERSION>  Model version stored in and validated against each snapshot
      --latest-only                    Convert only the newest legacy snapshot
      --overwrite                      Replace an existing NetCDF snapshot with the same timestamp
```

The `config` form takes paths and model metadata from a configuration that already points
at the GeoTIFF static data and the NetCDF warm-state directory; the `risico` and `fwi`
forms take them explicitly.
