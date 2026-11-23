# Input Data

A run needs two kinds of data: weather inputs, which change at every run, and static data,
which you prepare once for your domain. This page describes the format of both. The
[sample data](getting-started.md#run-the-sample) is a complete, small example of each.

## Weather inputs

Weather inputs are NetCDF files (NetCDF-4 or classic) in one directory. Every `.nc` file in it
is read; a run can mix files, for example one per variable or one per forecast day, as long as
each file carries its own coordinates and time axis.

### File layout

Each file holds:

- a `time` variable: one value per timestamp, with a CF `units` attribute such as
  `hours since 2025-07-01 00:00:00` (without the attribute, seconds since 1970-01-01 are
  assumed);
- `latitude` and `longitude` variables with **two dimensions** `(y, x)`, giving the position
  of each grid point in degrees (WGS84);
- the weather variables, with dimensions `(time, y, x)`.

```text
dimensions:
    time = 744 ;  y = 11 ;  x = 24 ;
variables:
    int64 time(time) ;          time:units = "seconds since 1970-01-01 00:00:00" ;
    float latitude(y, x) ;
    float longitude(y, x) ;
    float T(time, y, x) ;       T:units = "degC" ;
    float H(time, y, x) ;
    ...
```

The grid can be regular or curvilinear and can have any resolution: each model cell takes the
value of the nearest grid point. Missing values are written as `-9999`.

!!! tip "Files with one-dimensional coordinates"
    Many datasets store `latitude(latitude)` and `longitude(longitude)` as 1-D axes. Expand
    them to 2-D before a run, for example with xarray:

    ```python
    import numpy as np, xarray as xr

    ds = xr.open_dataset("forecast.nc").rename({"latitude": "y", "longitude": "x"})
    lon, lat = np.meshgrid(ds.x.values, ds.y.values)
    ds = ds.assign(latitude=(("y", "x"), lat), longitude=(("y", "x"), lon))
    ds.to_netcdf("input/forecast.nc")
    ```

### Variables

Name each variable as in the first column. Give the ones your models need; the others can be
left out.

| Name | Meaning | Units |
|---|---|---|
| `T` | Air temperature at 2 m | °C or K (detected) |
| `H` | Relative humidity at 2 m | % |
| `R` | Dew point temperature at 2 m, instead of `H` | °C or K |
| `Q`, `PSFC` | Specific humidity and surface pressure, instead of `H` | kg/kg, Pa |
| `W`, `D` | Wind speed at 10 m and the direction it blows **from** | m/s, degrees |
| `U`, `V` | Wind components at 10 m, instead of `W` and `D` | m/s |
| `P` | Rain fallen since the previous timestamp | mm |
| `SNOW` | Snow cover depth | cm |
| `K`, `F`, `O` | Observed temperature, humidity and rain, alternatives to `T`, `H`, `P` | as above |
| `NDVI`, `NDWI`, `M`, `SWI` | Satellite vegetation, water, moisture and soil water indices | – |

Use **hourly** timestamps for RISICO: it is built for hourly steps, and rain must be the amount
of the step, not an accumulation since the start of the forecast. Which variables each model
uses is listed in [Fire Danger Indices](indices.md).

### Other variable names

If your files use other names, map them in the configuration with a
`netcdf_input_configuration` section at the top level:

```yaml
netcdf_input_configuration:
  lat_name: lat              # name of the latitude variable
  lon_name: lon              # name of the longitude variable
  time_name: valid_time      # name of the time variable
  variable_map:
    - { internal_name: T, name: t2m }
    - { internal_name: H, name: rh2m }
    - { internal_name: P, name: tp_hourly, offset: -3600 }
```

`offset`, in seconds, shifts the time axis of one variable: for example `-3600` when the rain of
the hour ending at 13:00 is stored at 13:00 but should be read at 12:00. Two optional keys handle
unusual files: `time_units` names the attribute that holds the time units, when it is not
`units`, and `coords_dims` (a pair of dimension names) selects the latitude and longitude
dimensions when the coordinate variables have more than two.

### Legacy `zbin` inputs

Instead of a directory, the input argument can be a text file listing `.zbin` files, one per
line: the binary format of the previous RISICO implementation. New setups should use NetCDF.

## Static data

Static data describe the land on the grid the model computes on. They are single-band GeoTIFF
files, all with exactly the same grid:

- geographic coordinates (EPSG:4326), north up, not rotated;
- the same number of rows and columns and the same pixel size and origin.

| Layer | Content | Needed by |
|---|---|---|
| Domain mask | Non-zero on the cells to compute, 0 or nodata elsewhere. It defines the model grid. | every model with `static_data` |
| Slope | Terrain slope, degrees | RISICO |
| Aspect | Direction the slope faces, degrees clockwise from north | RISICO |
| Vegetation id | Vegetation type of each cell, an id of the vegetation catalogue | RISICO |
| PPF summer, PPF winter | Seasonal weights of spread and intensity (optional, both or neither) | RISICO |

Slope and aspect are usually derived from a digital elevation model, for example with
`gdaldem slope` and `gdaldem aspect`; the vegetation id from a land-cover map reclassified to
the catalogue's ids.

### Vegetation catalogue

A whitespace-separated text file with one line per vegetation type. The first line is a header
and starts with `#`:

```text
#ID d0  d1  hhv    umid   v0   T0     sat  use_ndvi     decodifica
0   0   0   -9999  -9999  0    -9999 -9999 false        non vegetato
1   0.5 0   17000  -9999  84   12     40   true         praterie_bassa_susc
```

| Column | Meaning | Units |
|---|---|---|
| `ID` | Value used in the vegetation-id layer | – |
| `d0` | Dead fine fuel load | kg/m² |
| `d1` | Live fuel load | kg/m² |
| `hhv` | Higher heating value of the fuel | kJ/kg |
| `umid` | Moisture of the live fuel | % |
| `v0` | Base rate of spread, with no wind, slope or drying | m/h |
| `T0` | Time the dead fuel takes to respond to the weather | h |
| `sat` | Moisture of the dead fuel when saturated | % |
| `use_ndvi` | Whether the satellite NDVI modulates this type | `true`/`false` |
| last column | Description | – |

`-9999` marks a value that does not apply. The [sample data](getting-started.md#run-the-sample)
ships the catalogue used for Italy (`p_vegetazione.csv`).

## Cell files for the other indices

The indices other than RISICO and FWI (KBDI, Mark 5, Angström and others, see
[Fire Danger Indices](indices.md#other-indices)) read their cells from a text file instead of
GeoTIFF layers: one line per cell with longitude, latitude and a third value, separated by
spaces. The third value is the mean annual rainfall in mm for KBDI and Mark 5, the heat index
for Orieux, and is ignored by the others.

```text
8.900 44.400 1100
8.910 44.400 1100
```
