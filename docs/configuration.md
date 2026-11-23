# Configuration

A run is described by one YAML file: which models to run, where their data are and which
outputs to write. This page is the reference of that file. The
[sample configuration](demo/data/sample/configuration.yml) is a complete, working example to
start from.

## Structure

```yaml
models:                       # required: the models to run, one entry each
  - type: RISICO
    ...
  - type: FWI
    ...
palettes: {}                  # required: colour palettes for PNG outputs ({} if none)
netcdf_input_configuration:   # optional: names used in the weather files
  ...
streaming:                    # optional: memory and performance tuning
  ...
```

| Key | Content | See |
|---|---|---|
| `models` | List of model entries | [below](#models) |
| `palettes` | Map from output variable name to a palette file, for `PNGWJSON` outputs | [Outputs](#png-palettes) |
| `netcdf_input_configuration` | Variable, coordinate and time names of the weather files, when they differ from the defaults | [Input Data](input-data.md#other-variable-names) |
| `streaming` | Tile size, memory budget, scratch directory | [Large Domains and Performance](execution.md#configuration) |

Relative paths are resolved from the directory the command is run in.

## Models

A RISICO entry, with every key:

```yaml
models:
  - type: RISICO
    model_name: RISICO
    model_version: v2025
    static_data:
      type: geotiff
      domain_mask: static/domain_mask.tif
      slope: static/slope.tif
      aspect: static/aspect.tif
      vegetation_id: static/vegetation_id.tif
      vegetation_catalog: static/p_vegetazione.csv
      ppf_summer: static/ppf_summer.tif      # optional, with ppf_winter
      ppf_winter: static/ppf_winter.tif
    warm_state:
      type: netcdf
      directory: state/RISICO
      max_age_hours: 120
      on_missing: error
    warm_state_hour: 0
    warm_state_lag_days: 1
    output_time_resolution: 1
    output_types:
      - ...                                  # see Outputs
```

| Key | Meaning | Default |
|---|---|---|
| `type` | The model: `RISICO`, `FWI`, or one of the [other indices](indices.md#other-indices) | required |
| `model_name` | A name for the entry, used in the logs | required |
| `model_version` | The formulation, see [Fire Danger Indices](indices.md) | required (RISICO, FWI and some others) |
| `static_data` | The land data, [below](#static-data) | required (RISICO, FWI) |
| `warm_state` | Where snapshots are read and written, [below](#warm-state) | required (RISICO, FWI) |
| `warm_state_hour` | Hour of the day (UTC) at which a snapshot is saved | `0` |
| `warm_state_lag_days` | How many days before the run date the snapshot to start from is | `1` |
| `output_time_resolution` | Hours between two written outputs, counted from the run date | required for RISICO; `24` for FWI |
| `output_time_offset` | FWI only: hours from the run date of the first daily output | `0` |
| `output_types` | The outputs, [below](#outputs) | required |

The other indices take `cells_file_path` (their cells, see
[Input Data](input-data.md#cell-files-for-the-other-indices)) instead of `static_data`, and
`warm_state_path` (a file name prefix) instead of `warm_state` when they have a memory.

An FWI entry needs only the domain mask:

```yaml
  - type: FWI
    model_name: FWI
    model_version: cffdrs
    static_data:
      type: geotiff
      domain_mask: static/domain_mask.tif
    warm_state:
      type: netcdf
      directory: state/FWI
      on_missing: defaults
    output_time_resolution: 24
    output_time_offset: 12
    output_types:
      - ...
```

### Static data

`static_data` lists the GeoTIFF layers described in [Input Data](input-data.md#static-data).
`type` is always `geotiff`; `domain_mask` is required, the RISICO layers are required for
RISICO, and the two PPF layers are optional but go together. The domain mask defines the grid
the model computes on and the cells it computes.

### Warm state

| Key | Meaning | Default |
|---|---|---|
| `type` | Always `netcdf` | required |
| `directory` | Directory of the snapshots; created if missing | required |
| `max_age_hours` | Oldest snapshot accepted, in hours before the time searched | `120` |
| `on_missing` | What to do when no snapshot is found: `error` stops the run, `defaults` starts from default values | `error` |

A snapshot is saved at `warm_state_hour` of every day the run goes through, as
`<directory>/RISICO_YYYYMMDDTHHMMSSZ.nc` (or `FWI_...`). Give each model its own directory.

To start, a run looks for the snapshot at the run date minus `warm_state_lag_days`, plus
`warm_state_hour`; if there is none, it takes the newest older one within `max_age_hours`.
Snapshots record the model version and are rejected if it differs. They are NetCDF grids with
latitude and longitude, so they can be opened like any output; when the static grid changes
resolution, the state is resampled onto the new grid.

Use `on_missing: defaults` for the first run of a new setup, then switch to `error` so that a
broken chain stops instead of silently restarting from defaults.

## Outputs

Each model lists any number of `output_types`. An output type is a set of variables written in
one format, on one grid, into one directory:

```yaml
    output_types:
      - internal_name: RISICO
        name: RISICO
        path: output/RISICO
        format: NETCDF
        compression_level: 4
        grid:
          nrows: 103
          ncols: 274
          min_lat: 43.704665
          max_lat: 44.749078
          min_lon: 7.403763
          max_lon: 10.199103
        variables:
          - { internal_name: dffm, name: dffm, cluster_mode: Mean, precision: -1 }
          - { internal_name: V,    name: V,    cluster_mode: Max,  precision: 0 }
```

| Key | Meaning |
|---|---|
| `internal_name`, `name` | Names of the output type, used in logs and file names |
| `path` | Output directory, created if missing |
| `format` | `NETCDF`, `GEOTIFF`, `ZARR`, `PNGWJSON` or `ZBIN`, [below](#formats) |
| `compression_level` | `NETCDF`: deflate level 0–9; `ZARR`: zstd level 0–22. Default `7` |
| `grid` | The output grid: `nrows` × `ncols` cell centres from `min_lat`/`min_lon` to `max_lat`/`max_lon`, both included |
| `grid_path` | Instead of `grid`: a text grid file of the previous RISICO implementation |
| `variables` | The variables to write |

Each variable has:

| Key | Meaning |
|---|---|
| `internal_name` | The model variable, as listed in [Fire Danger Indices](indices.md) |
| `name` | Its name in the output files |
| `cluster_mode` | How the model cells falling in one output pixel are combined: `Mean`, `Min` or `Max` |
| `precision` | Values are rounded to a multiple of 10<sup>precision</sup>: `-1` keeps one decimal, `0` rounds to units, `1` to tens |

The output grid does not have to match the model grid: a coarser grid aggregates cells with
`cluster_mode`, which is how a 1 km run produces a 5 km map. Use the domain mask's own grid to
write every cell.

Outputs are written every `output_time_resolution` hours from the run date: `1` writes every
hour, `24` once a day at the hour of the run date (shifted by `output_time_offset` for FWI).

### Formats

| `format` | What is written |
|---|---|
| `NETCDF` | One NetCDF-4 file per variable, `<path>/<name>.nc`, with `time`, `latitude` and `longitude` axes and one map per output time. Readable once the run has finished. |
| `GEOTIFF` | One GeoTIFF per variable and output time |
| `ZARR` | One Zarr v3 store per variable, `<path>/<name>.zarr`, same axes as NetCDF |
| `PNGWJSON` | One coloured PNG image per variable and output time, with a JSON file describing its georeference, for web maps. Needs a palette per variable. |
| `ZBIN` | The binary format of the previous RISICO implementation |

### PNG palettes

`PNGWJSON` outputs colour each variable with the palette named after the variable's `name`:

```yaml
palettes:
  V: palettes/RISICO_V.pal
  I: palettes/RISICO_I.pal
```

A palette file has one line per class: the lower bound of the class and the red, green, blue
and alpha values (0–255), separated by tabs:

```text
-9999	200	200	200	0
0	0	153	255	255
68	0	128	0	255
104	255	255	0	255
```

## Checking a configuration

`streaming-inspect` reads a configuration and its static data and warm state and reports what a
run would do, without reading weather or writing anything (see [CLI Usage](cli.md#streaming-inspect)).
