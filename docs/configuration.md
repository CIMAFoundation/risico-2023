# Configuration

RISICO and FWI configure their static data and warm state as aligned, single-band
GeoTIFF layers and NetCDF snapshots. This is the only supported layout: the older
cell/PPF/vegetation-ID text files and plain-text warm-state files are no longer
readable directly by the model binary, and configuration files must be YAML (the
older whole-file `.txt` key/value format is no longer supported). Existing
deployments convert once with `static-converter` and `warm-state-converter`
(see [Converting legacy data](conversion.md)) before switching their configuration over.

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

Existing legacy snapshots can be converted before cutover: see [Converting legacy data](conversion.md).

## Outputs

Each model lists its `output_types`: where to write, on which regular grid, in which format,
and which variables (see [Fire danger indices](indices.md) for the variable names of each model).
Values are combined over the native cells of each output pixel with `cluster_mode` (`Mean`, `Max`,
`Min`, ...) and rounded to a multiple of 10<sup>`precision`</sup> (`-1` keeps one decimal).

```yaml
    output_time_resolution: 1
    output_types:
      - internal_name: RISICO2023
        name: RISICO2023
        path: /opt/risico/output
        format: NETCDF
        compression_level: 7
        grid:
          nrows: 1200
          ncols: 1378
          min_lat: 35.139
          max_lat: 47.427
          min_lon: 5.975
          max_lon: 20.085
        variables:
          - internal_name: dffm
            name: UMB
            cluster_mode: Min
            precision: -1
          - internal_name: V
            name: V
            cluster_mode: Max
            precision: 0
```

The legacy `grid_path` (a `GRIDNROWS=` / `MINLAT=` text file) is still accepted in place of `grid`.

Output types are written as `ZBIN`, `NETCDF`, `ZARR`, `PNGWJSON` or `GEOTIFF`;
any other `format` is a configuration error.
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
