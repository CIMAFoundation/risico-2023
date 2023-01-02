use std::rc::Rc;
use std::sync::Arc;
use std::{io::BufRead, collections::HashMap};
use std::fs;
use std::io::BufReader;

use crate::library::state::models::Vegetation;

use super::models::ConfigError;

/// Read the cells from a file.
/// :param file_path: The path to the file.
/// :return: A list of cells.
pub fn read_cells_properties(file_path: &str) 
    -> Result<(
        Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<String>
    ), ConfigError> {
    let file = fs::File::open(file_path)
        .map_err(|err| format!("can't open file: {err}."))?;

    let mut lons: Vec<f32> = Vec::new();
    let mut lats: Vec<f32> = Vec::new();
    let mut slopes: Vec<f32> = Vec::new();
    let mut aspects: Vec<f32> = Vec::new();
    let mut vegetations: Vec<String> = Vec::new();

    let reader = BufReader::new(file);
    
    for line in reader.lines(){
        let line = line
            .map_err(|err| format!("can't read from file: {err}."))?;
        if line.starts_with("#") {
            // skip header
            continue;
        }

        let line_parts: Vec<&str> = line.trim().split(' ').collect();
        
        if line_parts.len() < 5 {
            let error_message = format!("Invalid line in file: {}", line);
            return Err(error_message.into());
        }

        //  [TODO] refactor this for using error handling
        let lon = line_parts[0].parse::<f32>().unwrap();
        let lat = line_parts[1].parse::<f32>().unwrap();
        
        let slope = line_parts[2].parse::<f32>().unwrap();
        let aspect = line_parts[3].parse::<f32>().unwrap();
        let vegetation = line_parts[4].to_string();
        lons.push(lon);
        lats.push(lat);
        slopes.push(slope);
        aspects.push(aspect);
        vegetations.push(vegetation);
        
    }

    Ok( 
        (lats, lons, slopes, aspects, vegetations)
    )
    
}   

/// Read the cells from a file.
/// :param file_path: The path to the file.
/// :return: A list of cells.
pub fn read_vegetation(file_path: &str) -> 
    Result<
        HashMap<String, Arc<Vegetation>>,
        std::io::Error
    > {
     
    let file = fs::File::open(file_path)?;        
    let mut vegetations: HashMap<String, Arc<Vegetation>> = HashMap::new();

    let reader = BufReader::new(file);
    
    for (i, line) in reader.lines().enumerate(){
        let line = line?;
        if i == 0 && line.starts_with("#") {
            // skip header
            continue;
        }
        let line_elements: Vec<&str> = line.trim()
                        .split_whitespace()
                        .collect::<Vec<&str>>();
        

        if line_elements.len() < 5 {
            let error_message = format!("Invalid line in file: {}", line);
            let error = std::io::Error::new(std::io::ErrorKind::InvalidData, error_message);
            return Err(error);
        }

        //  [TODO] refactor this for using error handling
        let id = line_elements[0].to_string();
        let d0 = line_elements[1].parse::<f32>().unwrap();
        let d1 = line_elements[2].parse::<f32>().unwrap();
        let hhv = line_elements[3].parse::<f32>().unwrap();
        let umid = line_elements[4].parse::<f32>().unwrap();
        let v0 = line_elements[5].parse::<f32>().unwrap();
        #[allow(non_snake_case)]
        let T0 = line_elements[6].parse::<f32>().unwrap();
        let sat = line_elements[7].parse::<f32>().unwrap();
        let name = line_elements[8].to_string();
        
        let veg_id = id.clone();
        
        let veg = Arc::new(Vegetation {
            id,
            d0,
            d1,
            hhv,
            umid,
            v0,
            T0,
            sat,
            name
        });
        
        vegetations.insert(veg_id, veg);
    }
    
    Result::Ok(vegetations)
    
}   
