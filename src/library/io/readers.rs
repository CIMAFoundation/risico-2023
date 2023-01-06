
use std::{io::{self, Read}, fs::File};
use libflate::gzip::{self, Decoder};
use ndarray::Array1;

use crate::library::io::models::grid::Grid;

use super::models::grid::{RegularGrid, IrregularGrid};


fn read_header_from_file<T>(decoder: &mut Decoder<T>) -> Result<(u32, u32, u32), io::Error> where T: Read{   
    let mut is_regular: [u8; 4] = [0; 4];
    decoder.read_exact(&mut is_regular)?;
    let is_regular = u32::from_le_bytes(is_regular);
    
    let mut nrows: [u8; 4] = [0; 4];
    decoder.read_exact(&mut nrows)?;
    let nrows = u32::from_le_bytes(nrows);
    
    let mut ncols: [u8; 4] = [0; 4];
    decoder.read_exact(&mut ncols)?;
    let ncols = u32::from_le_bytes(ncols);

    Ok((is_regular, nrows, ncols))

}

fn read_array_from_file<T>(decoder: &mut Decoder<T>, len: u32) -> Result<Array1<f32>, io::Error> where T: Read{
    let mut buffer: Vec<u8> = vec![0; (len*4) as usize];
    decoder.read_exact(&mut buffer)?;

    const CHUNK_SIZE: usize = 4;
    let values = buffer
        .chunks_exact(CHUNK_SIZE)
        .map(|chunk| 
            f32::from_le_bytes(chunk.try_into()
            .expect(&format!("error loading data"))
        ))
        .collect::<Array1<f32>>();
    Ok(values)
}

pub fn read_grid_from_file(file: &str) -> Result<Box<dyn Grid>, io::Error> {
    let input =  File::open(file)
                .expect(&format!("Can't open file: {}", file));

    let input = io::BufReader::new(input);
    let mut decoder = gzip::Decoder::new(input)?;

    
    let (is_regular, nrows, ncols) = read_header_from_file(&mut decoder)?;
    let len = nrows * ncols;

    let grid: Box<dyn Grid> = match is_regular {
        0 => {
            let lats = read_array_from_file(&mut decoder, len)?;
            let lons = read_array_from_file(&mut decoder, len)?;
            Box::new(IrregularGrid::new(nrows as usize, ncols as usize, lats, lons))
        },
        1 => {
            let mut min_lat: [u8; 4] = [0; 4];
            let mut max_lat: [u8; 4] = [0; 4];
            decoder.read_exact(&mut min_lat)?;
            decoder.read_exact(&mut max_lat)?;
            let min_lat = f32::from_le_bytes(min_lat);
            let max_lat = f32::from_le_bytes(max_lat);
            
            let mut min_lon: [u8; 4] = [0; 4];
            let mut max_lon: [u8; 4] = [0; 4];
            decoder.read_exact(&mut min_lon)?;
            decoder.read_exact(&mut max_lon)?;
            let min_lon = f32::from_le_bytes(min_lon);
            let max_lon = f32::from_le_bytes(max_lon);

            Box::new(RegularGrid::new(nrows as usize, ncols as usize,  min_lat, min_lon, max_lat, max_lon))
        },
        _ => panic!("Unknown grid type"),
    };
    Ok(grid)
}

fn skip<T>(decoder: &mut Decoder<T>, len: usize) -> Result<(), io::Error> 
where T: Read {
    decoder.bytes().take(len).for_each(drop);
    Ok(())
}

/// read a file and returns Grid and Vector of data
/// Grid is a struct with the following fields:
pub fn read_values_from_file(file: &str) -> Result<Array1<f32>, io::Error> {
    let input =  File::open(file)
                .expect(&format!("Can't open file: {}", file));

    let input = io::BufReader::new(input);
    let mut decoder = gzip::Decoder::new(input)?;

    
    let (is_regular, nrows, ncols) = read_header_from_file(&mut decoder)?;
    let len = nrows*ncols;
    
    match is_regular {
        0 => skip(&mut decoder, (4*2*len) as usize),
        1 => skip(&mut decoder, 4*4),
        _ => panic!("Unknown grid type"),
    }?;
    

    let values = read_array_from_file(&mut decoder, len)?;

    Ok(values)
}
