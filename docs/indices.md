# Fire Danger Indices

One configuration can run any of the models below. Each is a `models` entry whose `type` selects the
model; all share the input handler, the tiled runner and the output writers. The variable names in
the tables are the `internal_name`s an output type lists.

## RISICO

`type: RISICO`, `model_version: legacy | v2023 | v2025`. Static data: GeoTIFF domain mask, slope,
aspect, vegetation id (with its catalogue) and, optionally, summer and winter PPF. Warm state: NetCDF
snapshots.

RISICO follows the moisture of the dead fine fuel hour by hour, drying it with temperature, humidity
and wind and wetting it with rain, and turns it into a rate of spread and a fireline intensity for the
vegetation of each cell, corrected for wind and slope.

| Variable | Meaning | Units |
|---|---|---|
| `dffm` | Dead fine fuel moisture | % |
| `V` | Rate of spread | m/h |
| `I` | Fireline intensity | kW/m |
| `W` | Wind effect on spread | – |
| `contrT` | Temperature effect on spread | – |
| `VPPF`, `IPPF` | Spread and intensity weighted by the seasonal PPF layer | m/h, kW/m |
| `NDVI`, `NDWI` | Satellite vegetation and water factors | – |
| `VNDWI`, `INDWI`, `VNDVI`, `INDVI` | Spread and intensity with the satellite factors | m/h, kW/m |
| `VPPFNDWI`, `IPPFNDWI`, `VPPFNDVI`, `IPPFNDVI` | Both PPF and satellite factors | m/h, kW/m |

The weather actually used is available as output too: `temperature`, `humidity`, `windSpeed`,
`windDir`, `rain` and `snowCover`.

## Canadian Fire Weather Index System

`type: FWI`, `model_version: legacy | cffdrs | sliding_window`. Static data: a GeoTIFF domain mask.
Warm state: NetCDF snapshots. See [Configuration](configuration.md) for the three versions.

| Variable | Meaning |
|---|---|
| `ffmc` | Fine Fuel Moisture Code |
| `dmc` | Duff Moisture Code |
| `dc` | Drought Code |
| `isi` | Initial Spread Index |
| `bui` | Build-Up Index |
| `fwi` | Fire Weather Index |
| `ifwi` | IFWI |

## Cell-based indices

These models read their cells from a text file (`cells_file_path`), one `lon lat value` line per
cell, where the third column is the mean annual rainfall (KBDI, Mark 5), the heat index (Orieux) or
unused. Those with memory keep a text warm state (`warm_state_path`).

| `type` | Index | Variables | Warm state |
|---|---|---|---|
| `KBDI` | Keetch–Byram Drought Index | `kbdi` (mm) | yes |
| `Mark5` | McArthur Forest Fire Danger Index, Mark 5 | `df` drought factor, `ffdi` | yes |
| `Angstrom` | Angström index | `angstrom` | no |
| `Fosberg` | Fosberg Fire Weather Index | `ffwi` | no |
| `Nesterov` | Nesterov ignition index | `nesterov` | yes |
| `Sharples` | Sharples fuel moisture and fire danger | `fmi`, `f` | no |
| `Orieux` | Orieux water reserve and danger class | `pet_t` (mm), `orieux_wr` (mm), `orieux_fd` (0–3) | yes |
| `Hdw` | Hot–Dry–Windy Index | `hdw` | no |

The [interactive demo](demo.md) runs all of them on one cell, side by side.
