# Getting Started

This page installs `risico-2023`, runs it on the sample data that comes with the repository and
shows how to look at the results. It takes about ten minutes. To see the model without
installing anything, open the [interactive demo](demo.md) first.

## Install

`risico-2023` is a single command-line program. It runs on Linux and macOS, and on Windows
through WSL.

### Build from source

You need [Git](https://git-scm.com) and the Rust toolchain. Install Rust with
[rustup](https://rustup.rs) if `cargo --version` does not work:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then clone the repository and build:

```bash
git clone https://github.com/CIMAFoundation/risico-2023.git
cd risico-2023
cargo build --release --bin risico-2023
```

The first build downloads the dependencies and takes a few minutes. The program is then at
`target/release/risico-2023`; check it with:

```bash
./target/release/risico-2023 --help
```

No other software is needed: NetCDF and GeoTIFF files are read and written by the program
itself, without the NetCDF, HDF5 or GDAL libraries.

!!! note "Prebuilt binaries"
    [GitHub releases](https://github.com/CIMAFoundation/risico-2023/releases) carry a static
    Linux x86_64 binary. This documentation follows the `dev` branch, which may be ahead of the
    latest release: build from source to match it.

## Run the sample

The repository ships a small, complete case in `docs/demo/data/`: the land data of a 1 km
grid over Liguria, in north-western Italy, and one month of hourly weather (July 2025) over
the same area.

```text
docs/demo/data/
├── static/                        land data, 274 × 103 cells of 1 km
│   ├── domain_mask.tif            cells to compute
│   ├── slope.tif, aspect.tif      terrain
│   ├── vegetation_id.tif          vegetation type of each cell
│   ├── ppf_summer.tif, ppf_winter.tif
│   └── p_vegetazione.csv          vegetation catalogue
└── sample/
    ├── configuration.yml          runs RISICO (hourly) and FWI (daily)
    └── input/
        └── weather-2025-07.nc     hourly weather, 744 hours
```

Run it from the `sample` directory:

```bash
cd docs/demo/data/sample
../../../../target/release/risico-2023 202507010000 configuration.yml input/
```

The three arguments are the run date (1 July 2025, 00:00 UTC), the configuration and the
directory of weather files. The run takes about 20 seconds and logs its progress. There is no
warm state yet, so both models start from default values (`on_missing: defaults` in the
configuration) and their first days are a spin-up.

When it ends, the directory holds:

```text
output/
├── RISICO/   dffm.nc  V.nc  I.nc  T.nc  H.nc  WS.nc  P.nc     one map per hour
└── FWI/      ffmc.nc  dmc.nc  dc.nc  isi.nc  bui.nc  fwi.nc  one map per day
state/
├── RISICO/   one snapshot per day (warm state)
└── FWI/
```

Run it again with a later date, for example `202507150000`: each model now starts from the
snapshot of the day before instead of default values. That is how an operational chain runs
day after day (see [How RISICO Works](concepts.md#the-daily-cycle)).

## Look at the results

The outputs are ordinary NetCDF files with `time`, `latitude` and `longitude` axes:

```bash
ncdump -h output/RISICO/V.nc          # with the NetCDF tools installed
```

In Python, with xarray and matplotlib:

```python
import xarray as xr

v = xr.open_dataset("output/RISICO/V.nc").V
v.sel(time="2025-07-07T15:00").plot(cmap="inferno")   # rate of spread, m/h
v.sel(latitude=44.45, longitude=8.9, method="nearest").plot()   # one cell over the month
```

They also open in [QGIS](https://qgis.org) (drag the file into the map) and in
[Panoply](https://www.giss.nasa.gov/tools/panoply/).

The variables are described in [Fire Danger Indices](indices.md). The ones in the sample are:

| File | Variable | Units |
|---|---|---|
| `RISICO/dffm.nc` | Dead fine fuel moisture | % |
| `RISICO/V.nc` | Rate of spread | m/h |
| `RISICO/I.nc` | Fireline intensity | kW/m |
| `RISICO/T.nc`, `H.nc`, `WS.nc`, `P.nc` | The weather the model used: temperature, humidity, wind speed, rain | °C, %, m/s, mm |
| `FWI/*.nc` | Canadian FWI System codes and indices | – |

## About the sample weather

The weather comes from the Open-Meteo historical forecast archive, on a 0.125° grid: weather
data by [Open-Meteo.com](https://open-meteo.com), licensed under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The file carries the same attribution
in its `attribution` and `license` attributes. `docs/demo/tools/make_sample_weather.py` rebuilds
it for another period. Both files are also online: the
[weather input](demo/data/sample/input/weather-2025-07.nc) and the
[configuration](demo/data/sample/configuration.yml).

## Next steps

To run RISICO on your own area:

1. Read [How RISICO Works](concepts.md) for the ideas behind a run.
2. Prepare your weather and land data as described in [Input Data](input-data.md).
3. Write a configuration, starting from the sample's: see [Configuration](configuration.md).
4. Run it as shown in [CLI Usage](cli.md).

!!! warning "Test runs on a production configuration"
    The output and warm-state paths of a configuration are the real destinations: a test run
    overwrites them. To try a change, copy the configuration and point those paths to a
    temporary directory.
