<div class="hero">
  <img class="hero__logo" src="img/risico.svg" alt="RISICO wildfire danger model logo" />
  <h1 class="hero__title">RISICO</h1>
  <p class="hero__lead">
    RISICO (<em>Rischio Incendi E Coordinamento</em>) is the wildfire danger forecasting model
    developed by <a href="https://www.cimafoundation.org" target="_blank" rel="noopener">CIMA Research Foundation</a>
    and used operationally by the Italian Civil Protection. This Rust implementation runs RISICO, the Canadian
    Fire Weather Index and eight more fire danger indices on gridded weather forecasts, from a single cell up
    to continental domains.
  </p>
  <div class="hero__actions">
    <a class="md-button md-button--primary" href="demo/">Try it in your browser</a>
    <a class="md-button" href="getting-started/">Get started</a>
    <a class="md-button" href="indices/">Fire danger indices</a>
    <a class="md-button" href="configuration/">Configuration</a>
  </div>
</div>

## What's included

- **The RISICO model**: dead fine fuel moisture, rate of spread and fireline intensity on every cell,
  driven by temperature, humidity, wind, rain and snow cover, with vegetation, slope and aspect from
  static layers.
- **More indices from the same inputs**: the Canadian FWI System, Keetch–Byram, McArthur Mark 5,
  Angström, Fosberg, Nesterov, Sharples, Orieux and Hot–Dry–Windy (see [Fire danger indices](indices.md)).
- **A streaming runner**: every model is processed in spatial tiles, timestamp by timestamp, so
  memory stays bounded however large the domain ([Tiled streaming execution](execution.md)).
- **Pure-Rust I/O**: NetCDF and legacy `zbin` inputs, NetCDF / GeoTIFF / Zarr / PNG outputs,
  GeoTIFF static data and NetCDF warm state, with no native C libraries.
- **A WebAssembly build**: the same binary compiled for `wasm32-wasip1`, which powers the
  [interactive demo](demo.md).

## Typical workflow

1. Prepare the static data as aligned GeoTIFF layers: a domain mask, slope, aspect and vegetation
   (see [Configuration](configuration.md)).
2. Write a `configuration.yml` listing the models, their warm-state directory and their outputs.
3. Run `risico-2023 <run date> configuration.yml <input>` on a directory of NetCDF weather inputs
   (or a list of legacy `zbin` files).
4. Read the NetCDF (or GeoTIFF, Zarr, PNG) outputs; the warm state written at the configured hour
   seeds the next day's run.
