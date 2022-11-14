
use std::io::BufRead;
use std::fs;
use std::io::BufReader;

use crate::library::state::models::{CellProperties};


pub fn read_cells_properties(file_path: &str) -> Result<Vec<CellProperties>, std::io::Error> {
    //* Read the cells from a file.
    //* :param file_path: The path to the file.
    //* :return: A list of cells.
     
    let file = fs::File::open(file_path)?;        
    let mut cells: Vec<CellProperties> = Vec::new();

    let reader = BufReader::new(file);
    
    for line in reader.lines(){
        let line = line?;
        let line_parts: Vec<&str> = line.trim().split(' ').collect();
        
        if line_parts.len() < 5 {
            let error_message = format!("Invalid line in file: {}", line);
            let error = std::io::Error::new(std::io::ErrorKind::InvalidData, error_message);
            return Err(error);
        }

        //  [TODO] refactor this for using error handling
        let lat = line_parts[0].parse::<f64>().unwrap();
        let lon = line_parts[1].parse::<f64>().unwrap();
        let slope = line_parts[2].parse::<f64>().unwrap();
        let aspect = line_parts[3].parse::<f64>().unwrap();
        let vegetation = line_parts[4].parse::<i16>().unwrap();
        
        let cell = CellProperties {
            lat,
            lon,
            slope,
            aspect,
            vegetation
        };
        cells.push(cell);
    }
    
    Result::Ok(cells)
    
}   
