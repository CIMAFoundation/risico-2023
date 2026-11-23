# CLI Usage

The repository builds two commands:

| Command | Purpose |
| --- | --- |
| `risico-2023` | Run the models of a configuration on a set of weather inputs |
| `streaming-inspect` | Validate a configuration and print its tile plan, without running |

```bash
cargo build --release --bin risico-2023 --bin streaming-inspect
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
:   A directory of NetCDF weather files: every `.nc` file in it is read (see
    [Input Data](input-data.md#weather-inputs)).

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
  <CONFIG>  YAML configuration

Options:
      --tile-height <TILE_HEIGHT>        Override raster tile height
      --tile-width <TILE_WIDTH>          Override raster tile width
      --cells-per-tile <CELLS_PER_TILE>  Override cells per batch for models reading a cell file
```

```bash
./target/release/streaming-inspect 202607010000 configuration.yml --tile-height 256 --tile-width 256
```
