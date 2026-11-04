//! Oracle tests against the reference implementations, when they are
//! installed: `ncdump` and `h5dump` from the command line, and the Python
//! netCDF4 / h5py bindings through the interpreter named by the
//! `NC4_STREAM_PYTHON` environment variable. A missing tool skips its test
//! with a message rather than failing, so the suite runs anywhere.

use std::{path::Path, process::Command};

use nc4_stream::{AttrValue, DataType, Nc4Writer, VariableOptions};

fn write_sample(path: &Path) {
    let rows = 3;
    let cols = 4;
    let steps = 5;
    let mut file = Nc4Writer::create(path).unwrap();
    file.put_attr("risico_version", AttrValue::Str("v-test".into())).unwrap();
    file.put_attr("missing_value", AttrValue::F32(-9999.0)).unwrap();
    let lat = file.add_dimension("latitude", rows).unwrap();
    let lon = file.add_dimension("longitude", cols).unwrap();
    let time = file.add_unlimited_dimension("time").unwrap();
    let lat_var = file
        .add_variable("latitude", DataType::F32, &[lat], VariableOptions::contiguous())
        .unwrap();
    file.put_values(lat_var, &[40.0f32, 40.5, 41.0]).unwrap();
    let lon_var = file
        .add_variable("longitude", DataType::F32, &[lon], VariableOptions::contiguous())
        .unwrap();
    file.put_values(lon_var, &[8.0f32, 8.5, 9.0, 9.5]).unwrap();
    let time_var = file
        .add_variable("time", DataType::I64, &[time], VariableOptions::chunked(vec![512]))
        .unwrap();
    file.put_var_attr(time_var, "units", AttrValue::Str("seconds since 1970-01-01 00:00:00.0".into()))
        .unwrap();
    file.put_var_attr(time_var, "calendar", AttrValue::Str("proleptic_gregorian".into()))
        .unwrap();
    let w = file
        .add_variable(
            "W",
            DataType::F32,
            &[time, lat, lon],
            VariableOptions::chunked(vec![1, rows, cols]).shuffle(true).deflate(7),
        )
        .unwrap();
    file.put_var_attr(w, "units", AttrValue::Str("-".into())).unwrap();
    file.put_var_attr(w, "long_name", AttrValue::Str("Wind Effect on Fire Spread".into()))
        .unwrap();
    file.put_var_attr(w, "missing_value", AttrValue::F32(-9999.0)).unwrap();
    for step in 0..steps {
        let values: Vec<f32> = (0..rows * cols).map(|i| step as f32 * 100.0 + i as f32).collect();
        file.append_record(w, &values).unwrap();
        file.append_record(time_var, &[1_700_000_000i64 + 3600 * step as i64]).unwrap();
    }
    file.finish().unwrap();
}

fn run(tool: &str, args: &[&str]) -> Option<String> {
    let output = match Command::new(tool).args(args).output() {
        Ok(output) => output,
        Err(_) => {
            eprintln!("skipping: {tool} is not installed");
            return None;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "{tool} {args:?} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Some(stdout)
}

#[test]
fn ncdump_reports_a_netcdf4_file_with_the_expected_structure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("W.nc");
    write_sample(&path);
    let path = path.to_str().unwrap();

    let Some(kind) = run("ncdump", &["-k", path]) else { return };
    assert_eq!(kind.trim(), "netCDF-4");

    let header = run("ncdump", &["-hs", path]).unwrap();
    for expected in [
        "time = UNLIMITED ; // (5 currently)",
        "latitude = 3 ;",
        "longitude = 4 ;",
        "float W(time, latitude, longitude) ;",
        "W:_ChunkSizes = 1, 3, 4 ;",
        "W:_Shuffle = \"true\" ;",
        "W:_DeflateLevel = 7 ;",
        "W:long_name = \"Wind Effect on Fire Spread\" ;",
        "W:missing_value = -9999.f ;",
        "int64 time(time) ;",
        "time:units = \"seconds since 1970-01-01 00:00:00.0\" ;",
        "float latitude(latitude) ;",
        "latitude:_Storage = \"contiguous\" ;",
        ":risico_version = \"v-test\" ;",
        ":missing_value = -9999.f ;",
        ":_Format = \"netCDF-4\" ;",
    ] {
        assert!(header.contains(expected), "ncdump header lacks {expected:?}:\n{header}");
    }
    assert!(!header.contains("_Netcdf4Dimid"), "dimension scale bookkeeping must stay hidden");

    let data = run("ncdump", &["-v", "W,time,latitude", path]).unwrap();
    assert!(data.contains("400, 401, 402, 403,"), "last record values:\n{data}");
    assert!(data.contains("1700000000, 1700003600, 1700007200, 1700010800, 1700014400"), "{data}");
    assert!(data.contains("40, 40.5, 41"), "{data}");
}

#[test]
fn h5dump_sees_the_same_layout_as_a_netcdf_c_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("W.nc");
    write_sample(&path);
    let Some(dump) = run("h5dump", &["-pBH", path.to_str().unwrap()]) else { return };
    for expected in [
        "SUPERBLOCK_VERSION 2",
        "DATASPACE  SIMPLE { ( 5, 3, 4 ) / ( H5S_UNLIMITED, 3, 4 ) }",
        "CHUNKED ( 1, 3, 4 )",
        "PREPROCESSING SHUFFLE",
        "COMPRESSION DEFLATE { LEVEL 7 }",
        "VALUE  9.96921e+36",
        "H5D_ALLOC_TIME_INCR",
        "ATTRIBUTE \"DIMENSION_LIST\"",
        "H5T_VLEN { H5T_REFERENCE { H5T_STD_REF_OBJECT } }",
        "ATTRIBUTE \"REFERENCE_LIST\"",
        "H5T_REFERENCE { H5T_STD_REF_OBJECT } \"dataset\";",
        "H5T_STD_I32LE \"dimension\";",
        "ATTRIBUTE \"_Netcdf4Dimid\"",
        "ATTRIBUTE \"_NCProperties\"",
        "DATASPACE  SIMPLE { ( 5 ) / ( H5S_UNLIMITED ) }",
        "CHUNKED ( 512 )",
    ] {
        assert!(dump.contains(expected), "h5dump lacks {expected:?}:\n{dump}");
    }
}

#[test]
fn python_netcdf4_and_h5py_read_values_and_dimension_scales() {
    let Ok(python) = std::env::var("NC4_STREAM_PYTHON") else {
        eprintln!("skipping: NC4_STREAM_PYTHON is not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("W.nc");
    write_sample(&path);
    let script = r#"
import sys
import numpy as np
import netCDF4, h5py
path = sys.argv[1]
ds = netCDF4.Dataset(path)
assert ds.data_model == "NETCDF4", ds.data_model
assert ds.dimensions["time"].isunlimited()
assert len(ds.dimensions["time"]) == 5
assert ds.risico_version == "v-test"
assert ds.missing_value == np.float32(-9999.0)
w = ds.variables["W"]
assert w.dimensions == ("time", "latitude", "longitude"), w.dimensions
assert w.chunking() == [1, 3, 4], w.chunking()
f = w.filters()
assert f["zlib"] and f["shuffle"] and f["complevel"] == 7, f
values = w[:]
assert values.shape == (5, 3, 4)
expected = (np.arange(5)[:, None] * 100 + np.arange(12)[None, :]).reshape(5, 3, 4).astype("f4")
assert np.array_equal(np.asarray(values), expected)
assert np.array_equal(ds.variables["time"][:], 1_700_000_000 + 3600 * np.arange(5))
assert np.array_equal(ds.variables["latitude"][:], np.array([40.0, 40.5, 41.0], "f4"))
assert ds.variables["time"].units == "seconds since 1970-01-01 00:00:00.0"
ds.close()
h = h5py.File(path)
assert h["latitude"].attrs["CLASS"] == b"DIMENSION_SCALE"
assert h["W"].dims[1][0].name == "/latitude"
assert h["W"].dims[0][0].name == "/time"
assert h["W"].fillvalue == np.float32(9.96921e36)
assert h["time"].maxshape == (None,)
print("python oracle ok")
"#;
    let output = Command::new(&python)
        .args(["-I", "-c", script, path.to_str().unwrap()])
        .output()
        .expect("python interpreter runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "python oracle failed\n{stdout}\n{stderr}");
    assert!(stdout.contains("python oracle ok"));
}
