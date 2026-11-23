# Getting Started

## Build

Compile the production binary with Cargo. No native C libraries are needed: NetCDF and GeoTIFF
I/O are pure Rust.

```bash
cargo build --release --bin risico-2023
```

The release workflow publishes a fully static `x86_64-unknown-linux-musl` binary that runs on any
x86_64 Linux regardless of the glibc version. The same build works locally on Linux with the
`musl-tools` package installed:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --bin risico-2023 --target x86_64-unknown-linux-musl
```

`./build-with-docker.sh` still builds a glibc-linked binary inside an Ubuntu 16.04 container when
one is needed.

## Run

The executable takes a run date, a configuration file, and an input source:

```bash
./target/release/risico-2023 \
  YYYYMMDDHHMM \
  /path/to/configuration.yml \
  /path/to/input
```

The input source is either a directory containing NetCDF inputs or a text file listing legacy
`.zbin` inputs.

### NetCDF inputs

Every `.nc` file in the input directory is inspected. By default a file provides `latitude` and
`longitude` on two dimensions, a `time` axis and any of the input variables below, named as in the
table (the names, the coordinate names and the time units can be remapped in the configuration).
Files written on the same grid share one grid in memory; regular grids are mapped by their axes,
curvilinear ones through an R-tree.

| Variable | Meaning | Units |
|---|---|---|
| `T` | Forecast temperature | °C or K |
| `K` | Observed temperature | °C or K |
| `H` | Forecast relative humidity | % |
| `F` | Observed relative humidity | % |
| `R` | Dew point temperature | °C or K |
| `Q`, `PSFC` | Specific humidity and surface pressure (to derive humidity) | kg/kg, Pa |
| `W`, `D` | Wind speed and direction (from, meteorological) | m/s, ° |
| `U`, `V` | Wind components | m/s |
| `P` | Forecast precipitation | mm |
| `O` | Observed precipitation | mm |
| `SNOW` | Snow cover | cm |
| `NDWI`, `NDVI`, `M`, `SWI` | Satellite indices | – |

### Development runs

The configured output and warm-state paths are live destinations. For development tests, copy the
configuration and redirect those paths, plus `streaming.scratch_directory`, to a temporary directory.

Next: [Configuration](configuration.md) for static data, warm state and outputs, and
[Fire danger indices](indices.md) for the models a configuration can run.
