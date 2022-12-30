
use std::{io::{self, Write}, fs::File};
use libflate::gzip;
use std::path::Path;
use std::io::BufWriter;


use super::models::{grid::{RegularGrid }, palette::Palette};


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



pub fn write_to_pngwjson(file: &str, grid: &RegularGrid, values: Vec<f32>, palette: &Palette) -> Result<(), io::Error> {
    let output = File::create(file)?;
                
    let output = io::BufWriter::new(output);
    
    let ref mut w = BufWriter::new(output);

    let nrows = grid.nrows as u32;
    let ncols = grid.ncols as u32;

    let mut encoder = png::Encoder::new(w, ncols, nrows); // Width is 2 pixels and height is 1.
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::NoFilter);
    
    let mut writer = encoder.write_header()?;


    let mut data:Vec<u8> = Vec::new();
    for index in 0..nrows*ncols{
        let index = index as usize;
        let val = values[index] as f32;
        let color = palette.get_color(val);
        let pixel_data = [color.r, color.g, color.b, color.a];
        for i in 0..4{
            data.push(pixel_data[i]);
        }
    }
    writer.write_image_data(&data)?;

    // create a new file name replacing the extension .png with .json
    let path = Path::new(file);
    let base_path = path.parent().expect("should have a parent");
    
    let file_name = path.file_stem().expect("should have a file name");
    let json_file = format!("{}/{}.json", base_path.to_str().unwrap(), file_name.to_str().unwrap());



    //convert last comment to valid rust
    let json = format!(
        "{{\n  
            \"west\":{},\n  
            \"east\":{},\n  
            \"south\":{},\n  
            \"north\":{} 
    \n}}", grid.min_lon, grid.max_lon, grid.min_lat, grid.max_lat);
    let mut json_file = File::create(json_file)?;
    json_file.write_all(json.as_bytes())?;
    
    
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