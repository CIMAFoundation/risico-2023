//! Round-trip tests: files written by `nc4-stream` are read back with the
//! pure-Rust `netcdf-reader`, which decodes NetCDF-4 (HDF5) independently of
//! this crate and of netcdf-c.

use nc4_stream::{AttrValue, DataType, Nc4Writer, VariableOptions};
use netcdf_reader::{NcAttrValue, NcFile, NcSliceInfo, NcSliceInfoElem};

fn attr_floats(attrs: &[netcdf_reader::NcAttribute], name: &str) -> Vec<f32> {
    match &attrs.iter().find(|a| a.name == name).expect(name).value {
        NcAttrValue::Floats(values) => values.clone(),
        other => panic!("{name} is not a float attribute: {other:?}"),
    }
}

fn attr_string(attrs: &[netcdf_reader::NcAttribute], name: &str) -> String {
    match &attrs.iter().find(|a| a.name == name).expect(name).value {
        NcAttrValue::Chars(value) => value.clone(),
        NcAttrValue::Strings(values) => values.join(""),
        other => panic!("{name} is not a string attribute: {other:?}"),
    }
}

/// The shape the model writes: one file per variable with an unlimited time
/// axis, one chunk per timestep, shuffle + deflate, coordinate variables.
fn write_model_like_file(path: &std::path::Path, steps: usize, deflate: u32) -> (usize, usize) {
    let rows = 3;
    let cols = 4;
    let mut file = Nc4Writer::create(path).unwrap();
    file.put_attr("risico_version", AttrValue::Str("v-test".into()))
        .unwrap();
    file.put_attr("missing_value", AttrValue::F32(-9999.0))
        .unwrap();
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
        .add_variable(
            "time",
            DataType::I64,
            &[time],
            VariableOptions::chunked(vec![512]),
        )
        .unwrap();
    file.put_var_attr(
        time_var,
        "units",
        AttrValue::Str("seconds since 1970-01-01 00:00:00.0".into()),
    )
    .unwrap();

    let mut options = VariableOptions::chunked(vec![1, rows, cols]).shuffle(true);
    if deflate > 0 {
        options = options.deflate(deflate);
    }
    let w = file
        .add_variable("W", DataType::F32, &[time, lat, lon], options)
        .unwrap();
    file.put_var_attr(w, "units", AttrValue::Str("-".into()))
        .unwrap();
    file.put_var_attr(w, "missing_value", AttrValue::F32(-9999.0))
        .unwrap();

    for step in 0..steps {
        let values: Vec<f32> = (0..rows * cols)
            .map(|i| step as f32 * 100.0 + i as f32)
            .collect();
        file.append_record(w, &values).unwrap();
        file.append_record(time_var, &[1_700_000_000i64 + 3600 * step as i64])
            .unwrap();
    }
    file.finish().unwrap();
    (rows, cols)
}

#[test]
fn model_like_file_reads_back_with_netcdf_reader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("W.nc");
    let steps = 5;
    let (rows, cols) = write_model_like_file(&path, steps, 4);

    let file = NcFile::open(&path).unwrap();
    assert_eq!(file.format(), netcdf_reader::NcFormat::Nc4);

    let dims = file.dimensions().unwrap();
    let time = dims.iter().find(|d| d.name == "time").unwrap();
    assert!(time.is_unlimited, "time must be the unlimited dimension");
    assert_eq!(time.size, steps as u64);
    assert_eq!(file.dimension("latitude").unwrap().size, rows as u64);
    assert_eq!(file.dimension("longitude").unwrap().size, cols as u64);

    let global = file.global_attributes().unwrap();
    assert_eq!(attr_string(global, "risico_version"), "v-test");
    assert_eq!(attr_floats(global, "missing_value"), vec![-9999.0]);

    let w = file.variable("W").unwrap();
    assert_eq!(w.shape(), vec![steps as u64, rows as u64, cols as u64]);
    let dim_names: Vec<&str> = w.dimensions().iter().map(|d| d.name.as_str()).collect();
    assert_eq!(dim_names, ["time", "latitude", "longitude"]);
    assert_eq!(attr_string(w.attributes(), "units"), "-");
    assert_eq!(attr_floats(w.attributes(), "missing_value"), vec![-9999.0]);

    let values: ndarray::ArrayD<f32> = file.read_variable("W").unwrap();
    assert_eq!(values.shape(), &[steps, rows, cols]);
    for step in 0..steps {
        for i in 0..rows * cols {
            let expected = step as f32 * 100.0 + i as f32;
            assert_eq!(values[[step, i / cols, i % cols]], expected);
        }
    }

    // One timestep hyperslab, which is how the model reads inputs.
    let slice: ndarray::ArrayD<f32> = file
        .read_variable_slice(
            "W",
            &NcSliceInfo {
                selections: vec![
                    NcSliceInfoElem::Index(3),
                    NcSliceInfoElem::Slice { start: 0, end: u64::MAX, step: 1 },
                    NcSliceInfoElem::Slice { start: 0, end: u64::MAX, step: 1 },
                ],
            },
        )
        .unwrap();
    assert_eq!(slice.iter().copied().next(), Some(300.0));

    let times: ndarray::ArrayD<i64> = file.read_variable("time").unwrap();
    assert_eq!(times.len(), steps);
    assert_eq!(times[[4]], 1_700_000_000 + 4 * 3600);
    assert_eq!(attr_string(file.variable("time").unwrap().attributes(), "units"), "seconds since 1970-01-01 00:00:00.0");

    let lats: ndarray::ArrayD<f32> = file.read_variable("latitude").unwrap();
    assert_eq!(lats.as_slice().unwrap(), &[40.0, 40.5, 41.0]);
    let lons: ndarray::ArrayD<f32> = file.read_variable("longitude").unwrap();
    assert_eq!(lons.as_slice().unwrap(), &[8.0, 8.5, 9.0, 9.5]);
}

#[test]
fn uncompressed_records_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.nc");
    write_model_like_file(&path, 2, 0);
    let file = NcFile::open(&path).unwrap();
    let values: ndarray::ArrayD<f32> = file.read_variable("W").unwrap();
    assert_eq!(values[[1, 0, 1]], 101.0);
}

#[test]
fn more_chunks_than_one_btree_leaf_holds() {
    // A v1 B-tree leaf holds 64 chunks; 150 records need three leaves and a
    // root node, so every level of the index is exercised.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.nc");
    let steps = 150;
    write_model_like_file(&path, steps, 1);
    let file = NcFile::open(&path).unwrap();
    assert_eq!(file.dimension("time").unwrap().size, steps as u64);
    let values: ndarray::ArrayD<f32> = file.read_variable("W").unwrap();
    assert_eq!(values.shape()[0], steps);
    for step in [0usize, 63, 64, 127, 128, 149] {
        assert_eq!(values[[step, 2, 3]], step as f32 * 100.0 + 11.0);
    }
}

#[test]
fn zero_records_is_a_valid_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.nc");
    write_model_like_file(&path, 0, 4);
    let file = NcFile::open(&path).unwrap();
    assert_eq!(file.dimension("time").unwrap().size, 0);
    let values: ndarray::ArrayD<f32> = file.read_variable("W").unwrap();
    assert_eq!(values.shape(), &[0, 3, 4]);
}

#[test]
fn fixed_size_chunked_grid_with_fill_value_and_every_type() {
    // The warm-state snapshot shape: 2-D grids chunked smaller than the grid,
    // NaN fill, u8 mask, f64 coordinates, i32/i64 attributes.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.nc");
    let height = 5;
    let width = 7;
    {
        let mut file = Nc4Writer::create(&path).unwrap();
        file.put_attr("state_schema_version", AttrValue::I32(1))
            .unwrap();
        file.put_attr("state_time", AttrValue::I64(1_700_000_000))
            .unwrap();
        file.put_attr("epsg", AttrValue::I32(4326)).unwrap();
        file.put_attr(
            "geotransform",
            AttrValue::F64s(vec![8.0, 0.01, 0.0, 41.0, 0.0, -0.01]),
        )
        .unwrap();
        let y = file.add_dimension("y", height).unwrap();
        let x = file.add_dimension("x", width).unwrap();
        let yv = file
            .add_variable("y", DataType::F64, &[y], VariableOptions::contiguous())
            .unwrap();
        file.put_values(yv, &[41.0f64, 40.99, 40.98, 40.97, 40.96])
            .unwrap();
        let xv = file
            .add_variable("x", DataType::F64, &[x], VariableOptions::contiguous())
            .unwrap();
        file.put_values(xv, &[8.0f64, 8.01, 8.02, 8.03, 8.04, 8.05, 8.06])
            .unwrap();

        let dffm = file
            .add_variable(
                "dffm",
                DataType::F32,
                &[y, x],
                VariableOptions::chunked(vec![2, 3])
                    .deflate(4)
                    .shuffle(true)
                    .fill_value_f32(f32::NAN),
            )
            .unwrap();
        let values: Vec<f32> = (0..height * width).map(|i| i as f32).collect();
        file.put_values(dffm, &values).unwrap();

        let active = file
            .add_variable(
                "active",
                DataType::U8,
                &[y, x],
                VariableOptions::chunked(vec![2, 3]).deflate(4),
            )
            .unwrap();
        let mask: Vec<u8> = (0..height * width).map(|i| (i % 2) as u8).collect();
        file.put_values(active, &mask).unwrap();

        file.put_attr("epsg_u32", AttrValue::U32(4326)).unwrap();
        file.put_attr("active_cell_count", AttrValue::U64(317_455))
            .unwrap();
        let counts = file
            .add_variable(
                "history_count",
                DataType::U32,
                &[y, x],
                VariableOptions::chunked(vec![5, 7]).deflate(1),
            )
            .unwrap();
        let count_values: Vec<u32> = (0..height * width).map(|i| i as u32 * 1000).collect();
        file.put_values(counts, &count_values).unwrap();
        file.finish().unwrap();
    }

    let file = NcFile::open(&path).unwrap();
    let global = file.global_attributes().unwrap();
    assert!(matches!(
        &global.iter().find(|a| a.name == "state_schema_version").unwrap().value,
        NcAttrValue::Ints(v) if v == &[1]
    ));
    assert!(matches!(
        &global.iter().find(|a| a.name == "state_time").unwrap().value,
        NcAttrValue::Int64s(v) if v == &[1_700_000_000]
    ));
    assert!(matches!(
        &global.iter().find(|a| a.name == "geotransform").unwrap().value,
        NcAttrValue::Doubles(v) if v.len() == 6 && v[3] == 41.0
    ));

    let dffm: ndarray::ArrayD<f32> = file.read_variable("dffm").unwrap();
    assert_eq!(dffm.shape(), &[height, width]);
    for i in 0..height * width {
        assert_eq!(dffm[[i / width, i % width]], i as f32);
    }
    let fill = file.variable("dffm").unwrap().attribute("_FillValue").unwrap();
    assert!(matches!(&fill.value, NcAttrValue::Floats(v) if v.len() == 1 && v[0].is_nan()));

    let active: ndarray::ArrayD<u8> = file.read_variable("active").unwrap();
    assert_eq!(active[[0, 1]], 1);
    assert_eq!(active[[0, 2]], 0);

    let ys: ndarray::ArrayD<f64> = file.read_variable("y").unwrap();
    assert_eq!(ys[[4]], 40.96);

    assert!(matches!(
        &global.iter().find(|a| a.name == "epsg_u32").unwrap().value,
        NcAttrValue::UInts(v) if v == &[4326]
    ));
    assert!(matches!(
        &global.iter().find(|a| a.name == "active_cell_count").unwrap().value,
        NcAttrValue::UInt64s(v) if v == &[317_455]
    ));
    let counts: ndarray::ArrayD<u32> = file.read_variable("history_count").unwrap();
    assert_eq!(counts[[4, 6]], 34_000);
}

#[test]
fn encoded_records_can_be_prepared_outside_the_writer_lock() {
    // Compression happens on the caller's thread; only the append is serial.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encoded.nc");
    let mut file = Nc4Writer::create(&path).unwrap();
    let time = file.add_unlimited_dimension("time").unwrap();
    let x = file.add_dimension("x", 4).unwrap();
    let v = file
        .add_variable(
            "v",
            DataType::F32,
            &[time, x],
            VariableOptions::chunked(vec![1, 4]).deflate(6).shuffle(true),
        )
        .unwrap();
    let first = file.encode_record(v, &[1.0f32, 2.0, 3.0, 4.0]).unwrap();
    let second = file.encode_record(v, &[5.0f32, 6.0, 7.0, 8.0]).unwrap();
    file.append_encoded_record(v, first).unwrap();
    file.append_encoded_record(v, second).unwrap();
    file.finish().unwrap();

    let file = NcFile::open(&path).unwrap();
    let values: ndarray::ArrayD<f32> = file.read_variable("v").unwrap();
    assert_eq!(values.as_slice().unwrap(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
}

#[test]
fn wrong_record_length_and_bad_definitions_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("errors.nc");
    let mut file = Nc4Writer::create(&path).unwrap();
    let time = file.add_unlimited_dimension("time").unwrap();
    let x = file.add_dimension("x", 4).unwrap();
    assert!(file.add_dimension("x", 4).is_err(), "duplicate dimension");
    assert!(file.add_unlimited_dimension("t2").is_err(), "one unlimited dimension");
    let v = file
        .add_variable("v", DataType::F32, &[time, x], VariableOptions::chunked(vec![1, 4]))
        .unwrap();
    assert!(file.add_variable("v", DataType::F32, &[x], VariableOptions::contiguous()).is_err());
    assert!(
        file.add_variable("c", DataType::F32, &[time], VariableOptions::contiguous()).is_err(),
        "an unlimited variable must be chunked"
    );
    assert!(
        file.add_variable("d", DataType::F32, &[x], VariableOptions::chunked(vec![1, 1])).is_err(),
        "chunk rank must match"
    );
    assert!(file.append_record(v, &[1.0f32, 2.0]).is_err());
    assert!(file.append_record(v, &[1.0f64, 2.0, 3.0, 4.0]).is_err(), "type mismatch");
    assert!(file.put_values(v, &[1.0f32; 4]).is_err(), "unlimited variables take records");
    file.append_record(v, &[1.0f32, 2.0, 3.0, 4.0]).unwrap();
    file.finish().unwrap();
}
