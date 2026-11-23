# How RISICO Works

This page explains the ideas behind a RISICO run: what goes in, what comes out, and how one
day's run hands over to the next. Read it before preparing your own data.

## Fire danger, not fire spread

RISICO does not simulate a fire. It estimates, for every cell of a map and every hour, how
easily a fire *would* start and spread there if one were ignited: how dry the dead
vegetation is, how fast a flame front would advance and how much energy it would release.
Civil protection services use these maps to decide where to raise alert levels, pre-position
resources or restrict activities.

The numbers come from two kinds of information:

- **weather**, which changes every hour: temperature, humidity, wind, rain and snow;
- **the land**, which does not: vegetation type, slope and aspect of the terrain.

## The pieces of a run

```text
                 ┌───────────────────────┐
 weather inputs ─▶                       ├─▶ outputs (maps of each variable, hour by hour)
 static data    ─▶   risico-2023 run     │
 warm state     ─▶                       ├─▶ new warm state (for the next run)
                 └───────────────────────┘
                     configuration.yml
```

Run date
:   The date and time a run is for, in UTC, written `YYYYMMDDHHMM`. It selects the warm state
    to start from and names some outputs. It does not filter the weather: every timestamp in
    the inputs is processed.

Weather inputs
:   Gridded fields, normally a weather forecast, one value per grid point and timestamp:
    temperature, relative humidity (or dew point), wind, rain and optionally snow cover. They
    are read from NetCDF files. Their grid can be coarser than the model's: each model cell
    takes the value of the nearest weather grid point. See [Input Data](input-data.md).

Static data
:   Maps that describe the land, all on the same grid: a **domain mask** saying which cells
    to compute, and for RISICO the **slope**, **aspect** and **vegetation type** of each cell.
    They are GeoTIFF files. The grid of the static data is the grid the model computes on:
    the Italian operational setup uses 1 km cells.

Warm state
:   The memory of the model. Dead vegetation dries and wets slowly, so the moisture of one
    hour depends on all the weather before it. At the end of each day, a run saves the state
    of every cell (a *snapshot*) and the next run starts from it. Without a snapshot the model
    starts from default values and needs a few days of weather before its results are
    meaningful: this is called *spin-up*.

Outputs
:   Maps of the variables you ask for, written at the times you ask for, on an output grid of
    your choice (the native grid, or a coarser one). NetCDF is the usual format; GeoTIFF,
    Zarr and PNG are also available.

Configuration
:   A YAML file listing the models to run and, for each, where its static data, warm state
    and outputs are. See [Configuration](configuration.md).

## The daily cycle

An operational chain runs once a day, when the new weather forecast arrives:

1. The forecast for the coming days is converted to NetCDF.
2. `risico-2023` runs with today's date. Each model loads the snapshot saved by yesterday's
   run, steps through the forecast hour by hour and writes its maps.
3. During the run, the model saves a new snapshot at the configured hour
   (`warm_state_hour`, midnight UTC by default) of each day it goes through.
4. Tomorrow's run starts from the snapshot saved today.

By default a run on date *D* looks for the snapshot of *D* minus one day, or the newest one
before it, up to five days old (`warm_state_lag_days` and `max_age_hours` change this). It
never starts from a snapshot written later than that, so running the same day twice gives
the same result.

## What RISICO computes

For each cell, every hour, RISICO:

1. updates the **dead fine fuel moisture** (`dffm`, %), the water content of dead leaves,
   needles and twigs: dry and warm air, wind and sun lower it, rain raises it;
2. computes the **rate of spread** (`V`, m/h) a fire front would have in that vegetation,
   higher on dry fuel, with the wind and uphill;
3. computes the **fireline intensity** (`I`, kW/m), the energy a fire front would release,
   from the rate of spread and the fuel load of the vegetation.

The vegetation types and their parameters (fuel load, heat content, base spread rate and
moisture response) come from a vegetation catalogue, a small text table shipped with the
static data. Two optional seasonal layers, **PPF** (summer and winter), weight spread and
intensity by how fire-prone each area is in each season.

The same run can also compute the [Canadian Fire Weather Index](indices.md#canadian-fire-weather-index-system)
and eight more fire danger indices used around the world, from the same weather.

## Cells, grids and tiles

The model computes on *cells*: the pixels of the domain mask that are active. Inactive pixels
(sea, outside the country) are skipped and get no value in the outputs.

To keep memory bounded on large domains, a run processes the domain in rectangular *tiles*,
timestamp by timestamp. This is automatic; [Large Domains and Performance](execution.md) shows
how to tune it.

## Glossary

| Term | Meaning |
| --- | --- |
| Aspect | Direction a slope faces, in degrees clockwise from north |
| Cell | One pixel of the domain mask where the model computes |
| Domain mask | GeoTIFF whose non-zero pixels are the cells to compute |
| `dffm` | Dead fine fuel moisture, % of dry weight |
| Fireline intensity | Energy released per metre of fire front, kW/m |
| FWI | Canadian Fire Weather Index System |
| PPF | Seasonal weighting layers of spread and intensity (summer and winter) |
| Rate of spread | Speed a fire front would advance, m/h |
| Run date | Date and time a run is for, UTC |
| Spin-up | Days of weather a model needs before its state is realistic |
| Snapshot | Saved warm state at one time, a NetCDF file |
| Static data | Maps that do not change during a run: mask, slope, aspect, vegetation |
| Warm state | Model memory carried from one run to the next |
