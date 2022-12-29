
use std::{io::{self, Write}, fs::File};
use libflate::gzip;

use super::models::grid::{RegularGrid };


pub fn write_to_zbin_file(file: &str, grid: &RegularGrid, values: Vec<f32>) -> Result<(), io::Error> {
    let output = File::create(file).expect(&format!("Can't create file: {}", file));
    
    let output = io::BufWriter::new(output);
    let mut encoder = gzip::Encoder::new(output)?;
    
    
    let buf = [1u8, 0u8, 0u8, 0u8 ];
    encoder.write(&buf)?;
    let nrows = grid.nrows as u32;
    let ncols = grid.ncols as u32;
    let buf = nrows.to_le_bytes();
    encoder.write(&buf)?;
    let buf = ncols.to_le_bytes();
    encoder.write(&buf)?;

    let buf = grid.min_lat.to_le_bytes();
    encoder.write(&buf)?;
    let buf = grid.max_lat.to_le_bytes();
    encoder.write(&buf)?;

    let buf = grid.min_lon.to_le_bytes();
    encoder.write(&buf)?;

    let buf = grid.max_lon.to_le_bytes();
    encoder.write(&buf)?;


    for index in 0..nrows*ncols{
        let index = index as usize;
        let val = values[index] as f32;
        let buf = val.to_le_bytes();
        encoder.write(&buf)?;        
    }

    encoder.finish();
    
    Ok(())
}


// #[allow(dead_code)]
// pub fn write_netcdf(){
//     // Create a new file with default settings
//     let mut file = netcdf::create("crabs.nc").unwrap();

//     // We must create a dimension which corresponds to our data
//     file.add_dimension("ncrabs", 10).unwrap();
//     // These dimensions can also be unlimited and will be resized when writing
//     file.add_unlimited_dimension("time").unwrap();

//     // A variable can now be declared, and must be created from the dimension names.
//     let mut var = file.add_variable::<i32>(
//                 "crab_coolness_level",
//                 &["time", "ncrabs"],
//     ).unwrap();
//     // Metadata can be added to the variable
//     var.add_attribute("units", "Kelvin");
//     var.add_attribute("add_offset", 273.15_f32);

//     // Data can then be created and added to the variable
//     let data : Vec<i32> = vec![42; 10];
//     var.put_values(&data, Some(&[0, 0]), None);
//     // (This puts data at offset (0, 0) until all the data has been consumed)

//     // Values can be added along the unlimited dimension, which
//     // resizes along the `time` axis
//     var.put_values(&data, Some(&[1, 0]), None);
// }