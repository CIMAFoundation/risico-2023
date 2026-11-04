use chrono::{DateTime, NaiveDateTime, Utc};
use libflate::gzip::{self, Decoder};
use log::warn;
use ndarray::Array1;
use risico::models::input::InputVariableName;

use std::{
    collections::HashMap,
    error::Error,
    fmt::{Display, Formatter},
    fs::File,
    io::{self, BufRead, Read},
    path::Path,
    sync::{Arc, Mutex},
};

use crate::common::io::models::grid::Grid;
use rayon::prelude::*;

use crate::common::io::models::grid::{CellIndexes, IrregularGrid, RegularGrid};

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
    pub grid_registry: HashMap<String, CellIndexes>,
    coordinate_registry: Vec<HashMap<String, CellIndexes>>,
    grids: HashMap<String, Box<dyn Grid>>,
    source_values: Mutex<HashMap<String, Arc<Array1<f32>>>>,
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
            coordinate_registry: Vec::new(),
            grids: HashMap::new(),
            source_values: Mutex::new(HashMap::new()),
            data_map,
        })
    }
}

impl InputHandler for BinaryInputHandler {
    /// Decode every file this timestamp refers to, in parallel.
    fn preload(&mut self, date: &DateTime<Utc>) {
        let Some(data_map) = self.data_map.get(date) else {
            return;
        };
        let pending: Vec<String> = {
            let cache = self
                .source_values
                .lock()
                .expect("binary source cache lock is poisoned");
            data_map
                .values()
                .map(|file| file.path.clone())
                .filter(|path| !cache.contains_key(path))
                .collect()
        };

        let decoded: Vec<(String, Arc<Array1<f32>>)> = pending
            .into_par_iter()
            .filter_map(|path| match read_values_from_file(path.as_str()) {
                Ok(values) => Some((path, Arc::new(values))),
                Err(error) => {
                    warn!("Error reading file {path}: {error}");
                    None
                }
            })
            .collect();

        let mut cache = self
            .source_values
            .lock()
            .expect("binary source cache lock is poisoned");
        cache.extend(decoded);
    }

    /// Returns the data for the given date and variable on the selected coordinates
    fn get_values(
        &self,
        selection: usize,
        var: InputVariableName,
        date: &DateTime<Utc>,
    ) -> Option<Array1<f32>> {
        let data_map = self.data_map.get(date)?;

        let file = data_map.get(&var)?;

        let cached = self
            .source_values
            .lock()
            .expect("binary source cache lock is poisoned")
            .get(&file.path)
            .cloned();
        let data = cached.unwrap_or_else(|| {
            let values = Arc::new(
                read_values_from_file(file.path.as_str())
                    .unwrap_or_else(|_| panic!("Error reading file {}", file.path)),
            );
            self.source_values
                .lock()
                .expect("binary source cache lock is poisoned")
                .insert(file.path.clone(), values.clone());
            values
        });

        let indexes = self
            .coordinate_registry
            .get(selection)
            .unwrap_or(&self.grid_registry)
            .get(&file.grid_name)
            .unwrap_or_else(|| panic!("there should be a grid named {}", file.grid_name));

        Some(indexes.gather(data.as_slice().expect("source values are contiguous")))
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

    fn register_coordinates(
        &mut self,
        lats: &[f32],
        lons: &[f32],
    ) -> Result<usize, Box<dyn Error>> {
        let grid_files: HashMap<String, String> = self
            .data_map
            .values()
            .flat_map(HashMap::values)
            .map(|input| (input.grid_name.clone(), input.path.clone()))
            .collect();

        let mut indexes_by_grid = HashMap::new();
        for (grid_name, input_path) in grid_files {
            if !self.grids.contains_key(&grid_name) {
                let grid = read_grid_from_file(&input_path)?;
                self.grids.insert(grid_name.clone(), grid);
            }
            let indexes = self
                .grids
                .get(&grid_name)
                .expect("input grid was just registered")
                .indexes(lats, lons);
            indexes_by_grid.insert(grid_name, indexes);
        }

        let selection = self.coordinate_registry.len();
        self.coordinate_registry.push(indexes_by_grid);
        Ok(selection)
    }

    fn clear_registered_coordinates(&mut self) {
        self.coordinate_registry.clear();
    }

    fn clear_cached_values(&mut self) {
        self.source_values
            .lock()
            .expect("binary source cache lock is poisoned")
            .clear();
    }

    fn info_input(&self) -> String {
        let mut info = String::new();
        for (date, input_files) in self.data_map.iter() {
            info.push_str(&format!("Date: {}\n", date));
            for (var, input_file) in input_files.iter() {
                info.push_str(&format!("Variable: {:?} File: {}\n", var, input_file.path));
            }
        }
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::io::models::grid::RegularGrid;

    #[test]
    fn registered_tile_coordinates_can_be_selected_without_remapping() {
        let date = Utc::now();
        let variable = InputVariableName::T;
        let mut files = HashMap::new();
        files.insert(
            variable,
            BinaryInputFile {
                grid_name: "grid".to_owned(),
                path: "unused".to_owned(),
            },
        );
        let mut grids: HashMap<String, Box<dyn Grid>> = HashMap::new();
        grids.insert(
            "grid".to_owned(),
            Box::new(RegularGrid::new(2, 2, 0.0, 0.0, 1.0, 1.0)),
        );
        let mut handler = BinaryInputHandler {
            grid_registry: HashMap::new(),
            coordinate_registry: Vec::new(),
            grids,
            source_values: Mutex::new(HashMap::new()),
            data_map: HashMap::from([(date, files)]),
        };

        let first = handler
            .register_coordinates(&[0.0], &[0.0])
            .expect("first tile coordinates should register");
        let second = handler
            .register_coordinates(&[1.0], &[1.0])
            .expect("second tile coordinates should register");

        assert_eq!(handler.coordinate_registry[first]["grid"].get(0), Some(0));
        assert_eq!(handler.coordinate_registry[second]["grid"].get(0), Some(3));
        handler.clear_registered_coordinates();
        assert!(handler.coordinate_registry.is_empty());
    }
}
