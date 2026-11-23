# Converting Legacy Data

!!! info "Only for existing RISICO installations"
    This page is for operators of the previous RISICO implementation, whose static data,
    grids and warm states are text files. New setups prepare GeoTIFF static data directly
    (see [Input Data](input-data.md#static-data)) and can skip it.

The previous implementation describes the land as a text file of cells (`lon lat slope aspect
vegetation` per line) with a separate grid file, and saves its warm state as text files. The
two converters below turn them, once, into the GeoTIFF layers and NetCDF snapshots this version
reads.

## Static data

Convert existing regular-grid static files with the offline `static-converter`
utility, which writes single-band EPSG:4326 GeoTIFF layers. The converter
detects, per axis, whether legacy bounds represent outer edges or cell centres;
this accommodates the conventions used by the deployed grids.

```console
cargo run --release --bin static-converter -- risico \
  --cells /share/risico/RISICO2023/STATIC/risico2023_input_1km.txt \
  --grid /share/risico/RISICO2023/GRID/input_1km_GRID.txt \
  --output /opt/risico/RISICO2023/STATIC/geotiff

cargo run --release --bin static-converter -- fwi \
  --cells /share/risico/FWIWORLD/STATIC/FWI_world.txt \
  --grid /share/risico/FWIWORLD/GRID/GRID.txt \
  --output /opt/risico/FWIWORLD/STATIC/geotiff
```

The converter checks that cells are unique and in the canonical north-to-south,
west-to-east order. This keeps legacy state rows aligned during a progressive
migration.

For `warm-state-converter`, `--legacy-prefix` may name a prefix or a directory
whose files are bare `YYYYMMDDHHMM` timestamps. Every written snapshot is read
back and validated before conversion is reported as successful. A malformed
legacy file is reported without preventing other discovered snapshots from
being checked.

## Warm state

Existing legacy snapshots can be converted before cutover. The domain mask defines
the output NetCDF grid and places each text row at its corresponding active pixel.
By default all timestamped files matching the prefix are converted; existing
NetCDF files are skipped unless `--overwrite` is used.

```console
cargo run --bin warm-state-converter -- config \
  --config /opt/risico/configuration.yml \
  --model-name RISICO2023 \
  --legacy-prefix /opt/risico/STATE0/state0RISICO_ \
  --latest-only

# The explicit form is useful outside a migrated deployment:
cargo run --bin warm-state-converter -- risico \
  --legacy-prefix /opt/risico/STATE0/state0RISICO_ \
  --domain-mask /opt/risico/static/domain_mask.tif \
  --output /opt/risico/state-nc \
  --model-version v2025

cargo run --bin warm-state-converter -- fwi \
  --legacy-prefix /opt/risico/STATE0/state0FWI_ \
  --domain-mask /opt/risico/static/domain_mask.tif \
  --output /opt/risico/state-nc \
  --model-version legacy \
  --latest-only
```
