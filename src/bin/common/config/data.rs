use std::f32::consts::PI;
use std::fs;
use std::io::BufReader;
use std::sync::Arc;
use std::{collections::HashMap, io::BufRead};

use risico::modules::risico::models::{CellPropertiesContainer, Vegetation};

use crate::common::helpers::RISICOError;

/// Read the cells from a file.
/// :param file_path: The path to the file.
/// :return: A list of cells.
pub fn from_file(file_path: &str) -> Result<CellPropertiesContainer, RISICOError> {
    let file = fs::File::open(file_path).map_err(|err| format!("can't open file: {err}."))?;

    let mut lons: Vec<f32> = Vec::new();
    let mut lats: Vec<f32> = Vec::new();
    let mut slopes: Vec<f32> = Vec::new();
    let mut aspects: Vec<f32> = Vec::new();
    let mut vegetations: Vec<String> = Vec::new();

    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.map_err(|err| format!("can't read from file: {err}."))?;
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
        let lon = line_parts[0]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));

        let lat = line_parts[1]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));

        let slope = line_parts[2]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        let aspect = line_parts[3]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));

        let vegetation = line_parts[4].to_string();

        let slope = slope * PI / 180.0;
        let aspect = aspect * PI / 180.0;

        lons.push(lon);
        lats.push(lat);
        slopes.push(slope);
        aspects.push(aspect);
        vegetations.push(vegetation);
    }

    let props = CellPropertiesContainer {
        lats,
        lons,
        slopes,
        aspects,
        vegetations,
    };
    Ok(props)
}

/// Read the cells from a file.
/// :param file_path: The path to the file.
/// :return: A list of cells.
pub fn read_vegetation(
    file_path: &str,
) -> Result<HashMap<String, Arc<Vegetation>>, std::io::Error> {
    let file = fs::File::open(file_path)?;
    let mut vegetations: HashMap<String, Arc<Vegetation>> = HashMap::new();

    let reader = BufReader::new(file);

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if i == 0 && line.starts_with("#") || line.is_empty() {
            // skip header and empty lines
            continue;
        }
        let line_elements: Vec<&str> = line.split_whitespace().collect::<Vec<&str>>();

        let n_elements = line_elements.len();
        if n_elements < 9 {
            let error_message = format!("Invalid line in file: {}", line);
            let error = std::io::Error::new(std::io::ErrorKind::InvalidData, error_message);
            return Err(error);
        }

        //  [TODO] refactor this for using error handling
        let id = line_elements[0].to_string();
        let d0 = line_elements[1]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        let d1 = line_elements[2]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        let hhv = line_elements[3]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        let umid = line_elements[4]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        let v0 = line_elements[5]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        #[allow(non_snake_case)]
        let T0 = line_elements[6]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));
        let sat = line_elements[7]
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("Invalid line in file: {}", line));

        let use_ndvi = match n_elements {
            10.. => line_elements[8]
                .parse::<bool>()
                .unwrap_or_else(|_| panic!("Invalid line in file: {}", line)),
            _ => false,
        };
        let name = line_elements[n_elements - 1].to_string();

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
            name,
            use_ndvi,
        });

        vegetations.insert(veg_id, veg);
    }

    Result::Ok(vegetations)
}
