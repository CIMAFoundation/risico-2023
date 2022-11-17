use std::{collections::HashMap, fs::File, u8::{self, MAX}};

use crate::library::state::models;

use super::data::{read_cells_properties, read_vegetation};

pub struct Config {
    pub cells: Vec<models::Properties>,
    pub vegetations: HashMap<String, models::Vegetation>,
}

impl Config {
    pub fn new(cells_file: &str, veg_file: &str) -> Config {
        let cells = match read_cells_properties(cells_file){
            Ok(cells) => cells,
            Err(error) => panic!("Error reading cells file {cells_file}: \n{error}")
        };
        
        let vegetations = match read_vegetation(veg_file) {
            Ok(vegetations) => vegetations,
            Err(error) => panic!("Error reading vegetation file {veg_file}: \n {error}")
        };
        
        Config {
            cells: cells,
            vegetations: vegetations
        }
    }

    pub fn init_state(&self) -> Vec<models::Cell> {
        let mut cells: Vec<models::Cell> = Vec::new();
        for cell in self.cells.iter() {
            let vegetation = match self.vegetations.get(&cell.vegetation){
                Some(vegetation) => vegetation,
                None => panic!("Vegetation not found: {}", cell.vegetation)
            };
            let cell = models::Cell::new(cell, vegetation);
            cells.push(cell);
        }
        cells
    }
}

pub trait Grid {
    fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize);
}

pub struct RegularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub min_lat: f32,
    pub min_lon: f32,
    pub max_lat: f32,
    pub max_lon: f32,
    pub step_lat: f32,
    pub step_lon: f32,
}

impl Grid for RegularGrid {
    fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize) {
        let i = ((lat - self.min_lat) / self.step_lat) as usize;
        let j = ((lon - self.min_lon) / self.step_lon) as usize;
        (i, j)
    }
}

pub struct IrregularGrid {
    pub nrows: usize,
    pub ncols: usize,
    pub lats : Vec<f32>,
    pub lons : Vec<f32>,
}

impl Grid for IrregularGrid {
    fn get_i_j(&self, lat: f32, lon: f32) -> (usize, usize) {
        let i = self.lats.iter().position(|&x| x == lat).unwrap();
        let j = self.lons.iter().position(|&x| x == lon).unwrap();
        (i, j)
    }
}

pub struct InputData {
    pub values: Vec<f32>,
    pub grid: Box<dyn Grid>
}

use std::io::{self, Read};
use libflate::{zlib::{Encoder, Decoder}, gzip};

pub fn read_input_from_file(file: &str){
    // Decoding
    // read file as binary
    let input: Box<dyn io::Read> =  
        Box::new(
            File::open(file)
                .expect(&format!("Can't open file: {}", file)
    ));
    let mut input = io::BufReader::new(input);
    let mut decoder = gzip::Decoder::new(input).expect("Read GZIP header failed");
    
    println!("HEADER: {:?}", decoder.header());

    let mut is_regular: [u8; 4] = [0; 4];
    decoder.read_exact(&mut is_regular).expect("Read is_regular failed");
    let is_regular = u32::from_be_bytes(is_regular);
    println!("{:?}", &is_regular);
    
    let mut nrows: [u8; 4] = [0; 4];
    decoder.read_exact(&mut nrows).expect("Read nrows failed");
    let nrows = u32::from_le_bytes(nrows);
    println!("{:?}", &nrows);
    
    let mut ncols: [u8; 4] = [0; 4];
    decoder.read_exact(&mut ncols).unwrap();
    let ncols = u32::from_le_bytes(ncols);
    println!("{:?}", &ncols);
    
    let (lats, lons) = match is_regular {
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
            (lats, lons)
        },
        1..=u32::MAX => {
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
            (lats, lons)
            
        }        
    };

    let mut values: Vec<f32> = Vec::new();
    for _ in 0..nrows*ncols {
        let mut value: [u8; 4] = [0; 4];
        decoder.read_exact(&mut value).unwrap();
        let value = f32::from_le_bytes(value);
        values.push(value);
    }

    print!("LATS: {:?}", lats);
    print!("LONS: {:?}", lons);

    // assert_eq!(decoded_data, b"Hello World!");    
}


