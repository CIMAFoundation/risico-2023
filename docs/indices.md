# Fire Danger Indices

Besides RISICO, `risico-2023` computes the Canadian Fire Weather Index System and eight more fire
danger indices used in different countries. They all read the same weather inputs, so one run can
compare them on the same domain. The [interactive demo](demo.md) runs all of them on one cell,
side by side, and [References](references.md) lists the publication behind each.

Each model is one entry of the configuration's `models` list; its `type` selects the model. The
variable names in the tables are the `internal_name`s an output lists (see
[Configuration](configuration.md#outputs)).

## Weather each model uses

| Model | Temperature | Humidity | Wind speed | Wind direction | Rain | Snow cover |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| RISICO | ✓ | ✓ | ✓ | ✓ | ✓ | optional |
| FWI | ✓ | ✓ | ✓ | | ✓ | |
| Mark 5 | ✓ | ✓ | ✓ | | ✓ | |
| KBDI | ✓ | | | | ✓ | |
| Angström | ✓ | ✓ | | | | |
| Fosberg | ✓ | ✓ | ✓ | | | |
| Nesterov | ✓ | ✓ (dew point) | | | ✓ | |
| Sharples | ✓ | ✓ | ✓ | | | |
| Orieux | ✓ | | ✓ | | ✓ | |
| Hot–Dry–Windy | ✓ | ✓ (vapour pressure deficit) | ✓ | | | |

Humidity can be given as relative humidity, dew point or specific humidity; the dew point and
the vapour pressure deficit are derived from what is given (see [Input Data](input-data.md#variables)).

## RISICO

`type: RISICO`. Developed by CIMA Research Foundation and used operationally by the Italian
Civil Protection. RISICO follows the moisture of the dead fine fuel hour by
hour, drying it with temperature, humidity and wind and wetting it with rain, and turns it into
the rate of spread and the intensity a fire would have in the vegetation of each cell, corrected
for wind and slope (see [How RISICO Works](concepts.md#what-risico-computes)). The model is
described by Fiorucci et al. (2008) and, in its current version, by Perello et al. (2025): see
[References](references.md#risico).

Model versions (`model_version`):

- `v2025`: the current formulation: the moisture equations of `v2023`, with a revised rate of
  spread and weather effect;
- `v2023`: revised dead fuel moisture equations and rate of spread;
- `legacy`: the original equations. An unknown version also selects them.

Static data: domain mask, slope, aspect, vegetation id and catalogue, and optionally the summer
and winter PPF layers. Warm state: yes.

| Variable | Meaning | Units |
|---|---|---|
| `dffm` | Dead fine fuel moisture | % |
| `V` | Rate of spread | m/h |
| `I` | Fireline intensity | kW/m |
| `W` | Wind effect on spread (multiplier) | – |
| `contrT` | Temperature effect on spread (multiplier) | – |
| `VPPF`, `IPPF` | Spread and intensity weighted by the seasonal PPF layer | m/h, kW/m |
| `NDVI`, `NDWI` | Satellite vegetation and water factors | – |
| `VNDWI`, `INDWI`, `VNDVI`, `INDVI` | Spread and intensity with the satellite factors | m/h, kW/m |
| `VPPFNDWI`, `IPPFNDWI`, `VPPFNDVI`, `IPPFNDVI` | With both PPF and satellite factors | m/h, kW/m |

The weather the model actually used can be written too: `temperature` (°C), `humidity` (%),
`windSpeed` (m/s), `windDir`, `rain` (mm) and `snowCover` (cm).

## Canadian Fire Weather Index System

`type: FWI`. The Canadian Forest Fire Weather Index System (Van Wagner, 1987), the most widely
used fire danger rating system in the world. Three moisture codes track the drying of fuel
layers of increasing depth, and three indices combine them with wind into a rating of fire
behaviour. FWI is a daily index, computed from the weather at noon. See
[References](references.md#canadian-fire-weather-index-system).

Model versions (`model_version`):

- `cffdrs`: the standard daily system, with the equations and constants of the
  [cffdrs](https://cran.r-project.org/package=cffdrs) R package and tested against its published
  values. It steps at each cell's local standard noon (time zone from the cell's position, no
  daylight saving time), with the noon weather interpolated from the hourly inputs and the rain
  accumulated since the previous noon.
- `legacy`: the formulation of the Italian operational chain: noon in local civil time, daily
  step computed when the output is written.
- `sliding_window`: computed on rolling 24-hour windows instead of at noon.

Static data: a domain mask. Warm state: yes. Write one map per day with
`output_time_resolution: 24`; with `cffdrs`, set `output_time_offset` to the hours from the
run date at which every cell of the domain has passed its noon (for example `12` for a domain
east of Greenwich).

| Variable | Meaning |
|---|---|
| `ffmc` | Fine Fuel Moisture Code: moisture of litter and fine fuel |
| `dmc` | Duff Moisture Code: moisture of loosely compacted organic layers |
| `dc` | Drought Code: moisture of deep, compact organic layers |
| `isi` | Initial Spread Index: expected rate of spread |
| `bui` | Build-Up Index: total fuel available |
| `fwi` | Fire Weather Index: overall fire intensity rating |
| `ifwi` | IFWI, a non-linear rescaling of `fwi` |

## Other indices

These indices read their cells from a text file (`cells_file_path`, see
[Input Data](input-data.md#cell-files-for-the-other-indices)) instead of GeoTIFF layers. Those
with memory keep their warm state in text files (`warm_state_path`).

| `type` | Index | Origin | Variables | Warm state |
|---|---|---|---|---|
| `KBDI` | Keetch–Byram Drought Index | United States | `kbdi` (mm of soil water deficit) | yes |
| `Mark5` | McArthur Forest Fire Danger Index, Mark 5 | Australia | `df` drought factor, `ffdi` | yes |
| `Angstrom` | Angström index | Sweden | `angstrom` | no |
| `Fosberg` | Fosberg Fire Weather Index | United States | `ffwi` | no |
| `Nesterov` | Nesterov ignition index | Russia | `nesterov` | yes |
| `Sharples` | Sharples fuel moisture and fire danger | Australia | `fmi`, `f` | no |
| `Orieux` | Orieux water reserve and danger class | France | `pet_t` (mm), `orieux_wr` (mm), `orieux_fd` (class 0–3) | yes |
| `Hdw` | Hot–Dry–Windy Index | United States | `hdw` | no |
