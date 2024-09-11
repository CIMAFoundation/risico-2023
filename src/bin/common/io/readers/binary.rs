use chrono::{DateTime, NaiveDateTime, Utc};
use libflate::gzip::{self, Decoder};
use log::warn;
use ndarray::Array1;
use risico::{constants::NODATAVAL, models::input::InputVariableName};

use std::{
    collections::HashMap,
    error::Error,
    fmt::{Display, Formatter},
    fs::File,
    io::{self, BufRead, Read},
    path::Path,
};

use crate::common::io::models::grid::Grid;

use crate::common::io::models::grid::{IrregularGrid, RegularGrid};
use rayon::prelude::*;

use super::prelude::InputHandler;

fn read_header_from_file<T>(decoder: &mut Decoder<T>) -> Result<(u32, u32, u32), io::Error>
where
    T: Read,
{
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

fn read_array_from_file<T>(decoder: &mut Decoder<T>, len: u32) -> Result<Array1<f32>, io::Error>
where
    T: Read,
{
    let mut buffer: Vec<u8> = vec![0; (len * 4) as usize];
    decoder.read_exact(&mut buffer)?;

    const CHUNK_SIZE: usize = 4;
    let values = buffer
        .chunks_exact(CHUNK_SIZE)
        .map(|chunk| {
            f32::from_le_bytes(
                chunk
                    .try_into()
                    .unwrap_or_else(|_| panic!("error loading data")),
            )
        })
        .collect::<Array1<f32>>();
    Ok(values)
}

pub fn read_grid_from_file(file: &str) -> Result<Box<dyn Grid>, io::Error> {
    let input = File::open(file).unwrap_or_else(|_| panic!("Can't open file: {}", file));

    let input = io::BufReader::new(input);
    let mut decoder = gzip::Decoder::new(input)?;

    let (is_regular, nrows, ncols) = read_header_from_file(&mut decoder)?;
    let len = nrows * ncols;

    let grid: Box<dyn Grid> = match is_regular {
        0 => {
            let lats = read_array_from_file(&mut decoder, len)?;
            let lons = read_array_from_file(&mut decoder, len)?;
            Box::new(IrregularGrid::new(
                nrows as usize,
                ncols as usize,
                lats,
                lons,
            ))
        }
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

            Box::new(RegularGrid::new(
                nrows as usize,
                ncols as usize,
                min_lat,
                min_lon,
                max_lat,
                max_lon,
            ))
        }
        _ => panic!("Unknown grid type"),
    };
    Ok(grid)
}

fn skip<T>(decoder: &mut Decoder<T>, len: usize) -> Result<(), io::Error>
where
    T: Read,
{
    decoder.bytes().take(len).for_each(drop);
    Ok(())
}

/// read a file and returns Grid and Vector of data
/// Grid is a struct with the following fields:
pub fn read_values_from_file(file: &str) -> Result<Array1<f32>, io::Error> {
    let input = File::open(file).unwrap_or_else(|_| panic!("Can't open file: {}", file));

    let input = io::BufReader::new(input);
    let mut decoder = gzip::Decoder::new(input)?;

    let (is_regular, nrows, ncols) = read_header_from_file(&mut decoder)?;
    let len = nrows * ncols;

    match is_regular {
        0 => skip(&mut decoder, (4 * 2 * len) as usize),
        1 => skip(&mut decoder, 4 * 4),
        _ => panic!("Unknown grid type"),
    }?;

    let values = read_array_from_file(&mut decoder, len)?;

    Ok(values)
}

#[derive(Debug, Clone)]
pub struct LegacyInputFileParseError {
    message: String,
}

impl From<std::io::Error> for LegacyInputFileParseError {
    fn from(err: std::io::Error) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}

impl From<&str> for LegacyInputFileParseError {
    fn from(err: &str) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}

impl From<String> for LegacyInputFileParseError {
    fn from(err: String) -> Self {
        Self { message: err }
    }
}

impl std::error::Error for LegacyInputFileParseError {
    fn description(&self) -> &str {
        &self.message
    }
}

impl Display for LegacyInputFileParseError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "LegacyInputFileParseError: {}", self.message)
    }
}

/// Parse an input filename and return a tuple with grid_name, variable and datetime
fn parse_line(line: &str) -> Result<(String, String, DateTime<Utc>), LegacyInputFileParseError> {
    let filename = Path::new(&line)
        .file_name()
        .ok_or(format!("Invalid line in input file list: {line}"))?
        .to_str()
        .expect("Should be a valid string");

    let name_and_ext = filename.split('.').collect::<Vec<&str>>();

    if name_and_ext.is_empty() || name_and_ext.len() > 2 {
        return Err(format!("Error parsing filename {line}").into());
    }

    let name = name_and_ext[0];
    let components: Vec<&str> = name.split('_').collect();

    if components.len() != 3 {
        return Err(format!("Error parsing filename {name}").into());
    }

    let date = components[0];
    let grid_name = components[1].to_string();
    let variable = components[2].to_string();

    // parse the date

    let date = NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M")
        .map_err(|error| format!("Error parsing date: {error}"))?;

    let date = DateTime::from_naive_utc_and_offset(date, Utc);

    Ok((grid_name, variable, date))
}

#[derive(Debug)]
pub struct BinaryInputFile {
    pub grid_name: String,
    pub path: String,
}

#[derive(Debug)]
pub struct BinaryInputHandler {
    pub grid_registry: HashMap<String, Array1<Option<usize>>>,
    pub data_map: HashMap<DateTime<Utc>, HashMap<InputVariableName, BinaryInputFile>>,
}

impl BinaryInputHandler {
    pub fn new(file_path: &str) -> Result<Self, Box<dyn Error>> {
        let grid_registry = HashMap::new();
        let mut data_map = HashMap::new();

        let file = File::open(file_path)?;

        // file is a text file in which each line is a file with the following structure:
        // directory/<YYYYmmDDHHMM>_<grid_name>_<variable>.<extension>
        // read the file and parse the lines
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(e) => {
                    warn!("Error reading line: {}", e);
                    continue;
                }
            };

            if !line.ends_with(".zbin") {
                continue;
            }

            let (grid_name, variable, date) = match parse_line(&line) {
                Ok(parsed) => parsed,
                Err(err) => {
                    warn!("Error parsing filename {line}: {err:?}");
                    continue;
                }
            };

            let date = date.with_timezone(&Utc);
            let input_file = BinaryInputFile {
                grid_name,
                path: line,
            };

            // add the data to the data map
            data_map.entry(date).or_insert_with(HashMap::new);

            if let Some(data_map_for_date) = data_map.get_mut(&date) {
                if let Ok(var) = variable.parse::<InputVariableName>() {
                    data_map_for_date.insert(var, input_file);
                } else {
                    warn!("Error parsing variable {variable}");
                }
            }
        }

        Ok(BinaryInputHandler {
            grid_registry,
            data_map,
        })
    }
}

impl InputHandler for BinaryInputHandler {
    /// Returns the data for the given date and variable on the selected coordinates
    fn get_values(&self, var: InputVariableName, date: &DateTime<Utc>) -> Option<Array1<f32>> {
        let data_map = match self.data_map.get(date) {
            Some(data_map) => data_map,
            None => return None,
        };

        let file = match data_map.get(&var) {
            Some(file) => file,
            None => return None,
        };

        let data = read_values_from_file(file.path.as_str())
            .unwrap_or_else(|_| panic!("Error reading file {}", file.path));

        let indexes = self
            .grid_registry
            .get(&file.grid_name)
            .unwrap_or_else(|| panic!("there should be a grid named {}", file.grid_name));

        let data: Vec<f32> = indexes
            .par_iter()
            .map(|index| index.and_then(|idx| Some(data[idx])).unwrap_or(NODATAVAL))
            .collect();
        let data = Array1::from(data);
        Some(data)
    }

    /// Returns the timeline
    fn get_timeline(&self) -> Vec<DateTime<Utc>> {
        let mut timeline: Vec<DateTime<Utc>> = Vec::new();
        for date in self.data_map.keys() {
            timeline.push(*date);
        }
        // sort the timeline
        timeline.sort();
        timeline
    }

    fn set_coordinates(&mut self, lats: &[f32], lons: &[f32]) -> Result<(), Box<dyn Error>> {
        for (_, input_files) in self.data_map.iter() {
            for (_, input_file) in input_files.iter() {
                if !self.grid_registry.contains_key(&input_file.grid_name) {
                    let mut grid = match read_grid_from_file(input_file.path.as_str()) {
                        Ok(grid) => grid,
                        Err(e) => return Err(e.into()),
                    };

                    let indexes = grid.indexes(lats, lons);
                    self.grid_registry
                        .insert(input_file.grid_name.clone(), indexes);
                }
            }
        }

        Ok(())
    }
}
