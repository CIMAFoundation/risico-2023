
use std::{io::{self, Read}, fs::File};
use libflate::gzip;

use crate::library::io::models::grid::GridFunctions;

use super::models::grid::{RegularGrid, IrregularGrid};

/// read a file and returns Grid and Vector of data
/// Grid is a struct with the following fields:
pub fn read_input_from_file(file: &str) -> Result<(Box<dyn GridFunctions>, Vec<f32>), io::Error> {
    
    let input: Box<dyn io::Read> =  
        Box::new(
            File::open(file)
                .expect(&format!("Can't open file: {}", file)
    ));
    let input = io::BufReader::new(input);
    let mut decoder = gzip::Decoder::new(input)?;
    
    let mut is_regular: [u8; 4] = [0; 4];
    decoder.read_exact(&mut is_regular)?;
    let is_regular = u32::from_le_bytes(is_regular);
    
    let mut nrows: [u8; 4] = [0; 4];
    decoder.read_exact(&mut nrows).expect("Read nrows failed");
    let nrows = u32::from_le_bytes(nrows);
    
    let mut ncols: [u8; 4] = [0; 4];
    decoder.read_exact(&mut ncols).unwrap();
    let ncols = u32::from_le_bytes(ncols);
    
    
    let grid: Box<dyn GridFunctions> = match is_regular {
        0 => {
            let mut lats: Vec<f32> = Vec::new();
            for _ in 0..nrows*ncols {
                let mut lat: [u8; 4] = [0; 4];
                decoder.read_exact(&mut lat).unwrap();
                let lat = f32::from_le_bytes(lat);
                lats.push(lat);
            }

            let mut lons: Vec<f32> = Vec::new();
            for _ in 0..nrows*ncols {
                let mut lon: [u8; 4] = [0; 4];
                decoder.read_exact(&mut lon).unwrap();
                let lon = f32::from_le_bytes(lon);
                lons.push(lon);
            }
            Box::new(IrregularGrid::new(nrows as usize, ncols as usize, lats, lons))
        },
        1 => {
            let mut min_lat: [u8; 4] = [0; 4];
            let mut max_lat: [u8; 4] = [0; 4];
            decoder.read_exact(&mut min_lat).unwrap();
            decoder.read_exact(&mut max_lat).unwrap();
            let min_lat = f32::from_le_bytes(min_lat);
            let max_lat = f32::from_le_bytes(max_lat);
            
            let mut min_lon: [u8; 4] = [0; 4];
            let mut max_lon: [u8; 4] = [0; 4];
            decoder.read_exact(&mut min_lon).unwrap();
            decoder.read_exact(&mut max_lon).unwrap();
            let min_lon = f32::from_le_bytes(min_lon);
            let max_lon = f32::from_le_bytes(max_lon);

            let mut lats: Vec<f32> = Vec::new();
            for i in 0..nrows {
                let lat = min_lat + (max_lat - min_lat) * (i as f32) / (nrows as f32);
                lats.push(lat);
            }
            let mut lons: Vec<f32> = Vec::new();
            for i in 0..ncols {
                let lon = min_lon + (max_lon - min_lon) * (i as f32) / (ncols as f32);
                lons.push(lon);
            }
            Box::new(RegularGrid::new(nrows as usize, ncols as usize,  min_lat, min_lon, max_lat, max_lon))
        },
        _ => panic!("Unknown grid type"),
    };

    

    let mut values: Vec<f32> = Vec::new();
    for _ in 0..nrows*ncols {
        let mut value: [u8; 4] = [0; 4];
        decoder.read_exact(&mut value).unwrap();
        let value = f32::from_le_bytes(value);
        values.push(value);
    }

    Ok((grid, values))
}
