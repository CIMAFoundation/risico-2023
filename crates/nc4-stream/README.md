# nc4-stream

A pure-Rust streaming writer for NetCDF-4 (HDF5) files. No C libraries, no
build scripts; the only dependency is `flate2` with its Rust backend.

## Why

netcdf-c and HDF5 keep every chunk written through them in a cache, take one
global lock per process, and must be installed on the machine that builds and
runs the program. This crate exists for one write pattern they make hard to
keep cheap: a model that produces one grid per timestep, for many variables,
and wants each grid on disk as soon as it exists.

The writer appends every record to the file as it arrives, compressed, and
writes all metadata when the file is finished. Memory per open file is one
chunk. Compression can run on any thread, with only the append serialised.

## What it writes

The structures netcdf-c itself emits, so ncdump, xarray, netCDF4-python, GDAL
and every HDF5 reader open the result as an ordinary NetCDF-4 file:

- version 2 superblock, version 2 object headers;
- chunked datasets indexed by a version 1 B-tree, with shuffle and deflate;
- dimension scales with `DIMENSION_LIST` and `REFERENCE_LIST`, so dimensions
  and coordinate variables round-trip;
- one unlimited dimension, in first position, whose current extent is the
  number of records written.

Supported element types: `f32`, `f64`, `i32`, `i64`, `u8`. Attributes: text
and numeric scalars or arrays, on the file and on variables. Fixed-size
variables can be contiguous or chunked and are written in one call.

## Example

```rust
use nc4_stream::{AttrValue, DataType, Nc4Writer, VariableOptions};

let mut file = Nc4Writer::create("W.nc")?;
file.put_attr("title", "wind effect")?;
let time = file.add_unlimited_dimension("time")?;
let y = file.add_dimension("y", 2)?;
let x = file.add_dimension("x", 3)?;
let w = file.add_variable(
    "W",
    DataType::F32,
    &[time, y, x],
    VariableOptions::chunked(vec![1, 2, 3]).shuffle(true).deflate(7),
)?;
file.put_var_attr(w, "units", "-")?;
for step in 0..10 {
    file.append_record(w, &[step as f32; 6])?;
}
file.finish()?;
# Ok::<(), nc4_stream::Error>(())
```

To compress outside a lock, `encode_record` produces an `EncodedRecord` from a
shared reference and `append_encoded_record` appends it.

## Tests

`cargo test` runs unit tests, round trips through the pure-Rust
`netcdf-reader`, and oracle checks with `ncdump`, `h5dump` and Python
(`netCDF4`, `h5py`) when they are installed. Set `NC4_STREAM_PYTHON` to an
interpreter that has `netCDF4` and `h5py` to enable the Python oracle.

## Not supported

Groups, strings and compound types as variable data, more than one unlimited
dimension, appending to a file that already exists, and reading. For reading
use `netcdf-reader`.
