<div class="hero">
  <img class="hero__logo" src="img/risico.svg" alt="RISICO wildfire danger model logo" />
  <h1 class="hero__title">RISICO</h1>
  <p class="hero__lead">
    RISICO (<em>Rischio Incendi E Coordinamento</em>) is the wildfire danger forecasting model
    developed by <a href="https://www.cimafoundation.org" target="_blank" rel="noopener">CIMA Research Foundation</a>
    and used operationally by the Italian Civil Protection. From a weather forecast and maps of
    vegetation and terrain, it computes hour by hour how dry the vegetation is and how fast and
    how intensely a fire would spread on every cell of a map.
  </p>
  <div class="hero__actions">
    <a class="md-button md-button--primary" href="demo/">Try it in your browser</a>
    <a class="md-button" href="getting-started/">Get started</a>
    <a class="md-button" href="concepts/">How it works</a>
    <a class="md-button" href="programmatic/">Developer guide</a>
    <a class="md-button" href="api/risico/index.html">API reference</a>
  </div>
</div>

## What you can do with it

- **Forecast fire danger** over a region or a country: run it every day on a weather forecast
  and get maps of fuel moisture, rate of spread and fireline intensity for the coming days.
- **Compare indices**: the same run computes the Canadian Fire Weather Index and eight more
  indices used around the world (Keetch–Byram, McArthur Mark 5, Angström, Fosberg, Nesterov,
  Sharples, Orieux, Hot–Dry–Windy). See [Fire Danger Indices](indices.md).
- **Study past seasons**: run it on weather reanalyses or archived forecasts.
- **Embed the models** in your own Rust program, without the files: see the
  [Developer Guide](programmatic.md).

## What it is

`risico-2023` is the current implementation of RISICO, written in Rust:

- one command-line program, with no external libraries to install;
- NetCDF weather inputs and outputs, GeoTIFF land data, plus GeoTIFF, Zarr and PNG outputs;
- domains of any size in bounded memory, from one cell to a continent
  ([Large Domains and Performance](execution.md));
- a Rust library with the models alone ([API reference](api-reference.md));
- a WebAssembly build of the same program, which runs the [interactive demo](demo.md) in your
  browser.

The source code is on [GitHub](https://github.com/CIMAFoundation/risico-2023).

## Where to start

1. **See it**: the [interactive demo](demo.md) runs the real model on one cell or on a map of
   Liguria, in your browser, with nothing to install.
2. **Understand it**: [How RISICO Works](concepts.md) explains weather inputs, land data, warm
   state and outputs.
3. **Run it**: [Getting Started](getting-started.md) installs the program and runs it on the
   sample data shipped with the repository.
4. **Use your data**: [Input Data](input-data.md) and [Configuration](configuration.md)
   describe how to set up your own domain.
