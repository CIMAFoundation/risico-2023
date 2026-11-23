# Building the Web Demo

The [interactive demo](demo.md) runs the unmodified `risico-2023` binary, compiled for the WASI target:

```bash
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1 --bin risico-2023
cp target/wasm32-wasip1/release/risico-2023.wasm docs/demo/
```

The docs workflow does the same on every push to `dev` and `main`, so the bundle is never committed.

## What differs on WASI

The WASI build is the same program with a few platform switches:

- **Zarr output is not available**: `zarrs` and its zstd dependency do not build for WASI.
  The demo writes NetCDF only.
- **NetCDF files are read into memory** where the reader has no positioned file access
  (`open_netcdf` in `src/bin/common/io/mod.rs`).
- **No threads**: there is no tile pool and rayon runs on the calling thread.
- The temporary directory is `$TMPDIR` (or `/tmp`) and the process id is 0.

## How the page runs it

`docs/demo/` holds a static page and a few ES modules:

| File | Role |
|---|---|
| `app.html`, `app.js` | The page: map, controls, charts, map rendering |
| `worker.js` | Runs the binary in a Web Worker |
| `lib/runner.js` | Builds an in-memory filesystem with [`@bjorn3/browser_wasi_shim`](https://github.com/bjorn3/browser_wasi_shim), runs the binary, collects its files |
| `lib/scenario.js` | Turns a scenario into `configuration.yml`, static layers, cell lists and arguments |
| `lib/netcdf3.js` | Writes the weather input as a NetCDF classic file |
| `lib/geotiff-write.js` | Writes the domain mask and static layers as GeoTIFFs |
| `lib/openmeteo.js` | Fetches hourly weather from Open-Meteo |

The outputs are the binary's own NetCDF-4 files, read with [h5wasm](https://github.com/usnistgov/h5wasm).
The shim grows a file by reallocating it on every write; `runner.js` replaces that with a doubling
buffer, which cut the time spent writing outputs by a factor of five on a run over the whole of Italy.

The static layers the page loads (`docs/demo/data/static/`) are a cut-out of the 1 km Italian domain
around Liguria (7.40–10.20 °E, 43.70–44.75 °N, 274 × 103 cells): domain mask, slope, aspect,
vegetation id, PPF and the vegetation catalogue, plus the colour palettes in `docs/demo/data/palettes/`.
`docs/demo/data/sample/` holds the July 2025 weather the page uses by default (a NetCDF input,
rebuilt by `docs/demo/tools/make_sample_weather.py`) and a configuration to run it with the
command-line model. The page reads whatever grid the layers define, so a larger domain only needs other files; for one
the size of Italy (317 000 cells) set a coarser Open-Meteo grid (`COARSE`, `REFINE` in `app.js`) and
expect about a minute and 1.1 GB of browser memory.

## Testing locally

Serve `docs/` with any static server that sends `application/wasm` for `.wasm` files:

```bash
python3 -m http.server --directory docs 8765
```

and open <http://127.0.0.1:8765/demo/app.html>. `mkdocs serve` works too and shows the page inside the
documentation.
