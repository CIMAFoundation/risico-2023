use std::{
    collections::HashMap,
    path::PathBuf,
};

use chrono::{DateTime, Utc};
use netcdf::MutableFile;

use crate::library::{
    config::models::{ConfigError, PaletteMap},
    io::writers::{write_to_pngwjson, write_to_zbin_file},
    state::{models::Output, constants::NODATAVAL},
};

use super::grid::{ClusterMode, RegularGrid};


const COMPRESSION_RATE: i32 = 4;

#[derive(Debug)]
pub struct OutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: ClusterMode,
    precision: i32,
}



impl OutputVariable {
    pub fn get_variable_on_grid(
        &self,
        lats: &[f32],
        lons: &[f32],
        output: &Output,
        grid: &RegularGrid,
    ) -> Option<Vec<f32>> {
        let values = output.get(&self.internal_name);

        let values = if let Some(values) = values {
            values
        } else {
            return None;
        };

        let values = values.as_slice().unwrap();
        let values = grid.project_to_grid(&lats, &lons, values, &self.cluster_mode);
        // transform to desired number of decimal places precision
        let cutval = f32::powi(10.0, self.precision);
        let values = values
            .iter()
            .map(|val| f32::ceil(val / cutval - 0.5) * cutval)
            .collect();
        Some(values)
    }

    pub fn new(internal_name: &str, name: &str, cluster_mode: ClusterMode, precision: i32) -> Self {
        Self {
            internal_name: internal_name.to_string(),
            name: name.to_string(),
            cluster_mode,
            precision,
        }
    }
}

pub struct OutputType {
    name: String,
    path: String,
    grid: RegularGrid,
    format: String,
    variables: Vec<OutputVariable>,
    palettes: PaletteMap,
    run_date: DateTime<Utc>,
    writer: Box<dyn Writer>,
}

impl OutputType {
    pub fn new(
        name: &str,
        path: &str,
        grid_path: &str,
        format: &str,
        run_date: &DateTime<Utc>,
        palettes: PaletteMap,
    ) -> Result<Self, ConfigError> {
        let grid = RegularGrid::from_txt_file(grid_path).unwrap();

        let writer: Box<dyn Writer> = match format {
            "ZBIN" => Box::new(ZBinWriter::new(path, name, run_date)),
            "PNGWJSON" => Box::new(PngWriter::new(path, name, &palettes, run_date)),
            "NETCDF" => Box::new(NetcdfWriter::new(path, name, run_date)),
            _ => Box::new(ZBinWriter::new(path, name, run_date)),
        };

        Ok(Self {
            name: name.to_string(),
            path: path.to_string(),
            grid: grid,
            format: format.to_string(),
            variables: Vec::new(),
            palettes: palettes,
            run_date: run_date.clone(),
            writer: writer,
        })
    }

    pub fn add_variable(&mut self, variable: OutputVariable) {
        self.variables.push(variable);
    }

    pub fn write_variables(
        &mut self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
    ) -> Result<(), ConfigError> {
        self.writer
            .write(output, &self.grid, lats, lons, &self.variables)
    }
}

#[derive(Debug)]
struct NetcdfWriter {
    path: PathBuf,
    name: String,
    run_date: DateTime<Utc>,
    files: HashMap<String, MutableFile>,
}

impl NetcdfWriter {
    fn new(path: &str, name: &str, run_date: &DateTime<Utc>) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: run_date.clone(),
            files: HashMap::new(),
        }
    }
}
        

struct ZBinWriter {
    path: PathBuf,
    name: String,
    run_date: DateTime<Utc>,
}

impl ZBinWriter {
    fn new(path: &str, name: &str, run_date: &DateTime<Utc>) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: run_date.clone(),
        }
    }
}

struct PngWriter {
    path: PathBuf,
    name: String,
    palettes: PaletteMap,
    run_date: DateTime<Utc>,
}

impl PngWriter {
    fn new(path: &str, name: &str, palettes: &PaletteMap, run_date: &DateTime<Utc>) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: run_date.clone(),
            palettes: palettes.clone(),
        }
    }
}

trait Writer {
    fn write(
        &mut self,
        output: &Output,
        grid: &RegularGrid,
        lats: &[f32],
        lons: &[f32],
        variables: &[OutputVariable],
    ) -> Result<(), ConfigError>;
}

impl Writer for NetcdfWriter {
    fn write(
        &mut self,
        output: &Output,
        grid: &RegularGrid,
        lats: &[f32],
        lons: &[f32],
        variables: &[OutputVariable],
    ) -> Result<(), ConfigError> {
        for variable in variables {
            let n_lats = grid.nrows;
            let n_lons = grid.ncols;

            if !self.files.contains_key(&variable.name) {
                let path = self.path.as_os_str().to_str().unwrap();
                //let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
                let file_name = format!(
                    "{}/{}.nc",
                    path, variable.name
                );

                let options = netcdf::Options::NETCDF4;

                let mut file = netcdf::create_with(&file_name, options)
                    .map_err(|err| format!("can't create file {file_name}: {err}"))?;

                file.add_attribute("missing_value", NODATAVAL)
                    .expect("Should add attribute");

                // We must create a dimension which corresponds to our data
                file.add_dimension("latitude", n_lats).unwrap();
                file.add_dimension("longitude", n_lons).unwrap();

                file.add_unlimited_dimension("time").map_err(|err| format!("Add time dimension failed {err}"))?;
                let lats: Vec<f32> = (0..n_lats)
                    .map(|i| {
                        grid.min_lat
                            + (grid.max_lat - grid.min_lat) * (i as f32) / (grid.nrows as f32)
                    })
                    .collect();
                let lons: Vec<f32> = (0..n_lons)
                    .map(|i| {
                        grid.min_lon
                            + (grid.max_lon - grid.min_lon) * (i as f32) / (grid.ncols as f32)
                    })
                    .collect();

                let mut var = file
                    .add_variable::<f32>("latitude", &["latitude"])
                    .expect("Add latitude failed");

                var.put_values(&lats, None, None)
                    .expect("Add longitude failed");

                let mut var = file
                    .add_variable::<f32>("longitude", &["longitude"])
                    .expect("Add longitude failed");

                var.put_values(&lons, None, None)
                    .expect("Add longitude failed");
        
                let mut time_var = file
                    .add_variable::<u32>("time", &["time"])
                    .expect("Add time failed");
        
                time_var.add_attribute("units", "seconds since 1970-01-01 00:00:00")
                    .unwrap_or_else(|_| panic!("Add time units failed"));

                let mut variable_var = file
                            .add_variable::<f32>(&variable.name, &["time", "latitude", "longitude"])
                            .unwrap_or_else(|_| panic!("Add {} failed", variable.name));

                variable_var.compression(COMPRESSION_RATE)
                            .expect("Set compression failed");

                variable_var.add_attribute("missing_value", NODATAVAL)
                    .expect("Should add attribute");
                
                

                self.files.insert(variable.name.clone(), file);

            }

            let file = self.files.get_mut(&variable.name).unwrap();

            
            let mut time_var = file
                .variable_mut("time")
                .ok_or_else(|| format!("variable not found: time"))?;
            let time: u32 = output.time.timestamp() as u32;
            let len = time_var.len();
            time_var.put_values(&[time], Some(&[len]), Some(&[1 as usize]))
                .unwrap_or_else(|_| panic!("Add time failed"));                    

            let mut variable_var = file
                .variable_mut(&variable.name)
                .ok_or_else(|| format!("variable not found: {}", variable.name))?;
            
            
            let values = variable.get_variable_on_grid(&lats, &lons, &output, &grid);
            
            if let Some(values) = values {
                variable_var.put_values(values.as_slice(), Some(&[len, 0, 0]), Some(&[1, n_lats, n_lons]) )
                   .unwrap_or_else(|err| panic!("Add variable failed: {err}"));
            }else{
                continue
            }

            
        
        }
        Ok(())
    }
}

impl Writer for ZBinWriter {
    fn write(
        &mut self,
        output: &Output,
        grid: &RegularGrid,
        lats: &[f32],
        lons: &[f32],
        variables: &[OutputVariable],
    ) -> Result<(), ConfigError> {
        let path = self.path.as_os_str().to_str().unwrap();
        for variable in variables {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
            let file = format!(
                "{}/{}_{}_{}_{}.zbin",
                path, self.name, run_date, date_string, variable.name
            );
            let values = variable.get_variable_on_grid(&lats, &lons, &output, &grid);

            if let Some(values) = values {
                write_to_zbin_file(&file, &grid, values)
                    .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
            }

        }
        Ok(())
    }
}

impl Writer for PngWriter {
    fn write(
        &mut self,
        output: &Output,
        grid: &RegularGrid,
        lats: &[f32],
        lons: &[f32],
        variables: &[OutputVariable],
    ) -> Result<(), ConfigError> {
        let path = self.path.as_os_str().to_str().unwrap();
        for variable in variables {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
            let file = format!(
                "{}/{}_{}_{}_{}.png",
                path, self.name, run_date, date_string, variable.name
            );
            let values = variable.get_variable_on_grid(&lats, &lons, &output, &grid);
            let palette = self
                .palettes
                .get(&variable.name)
                .ok_or(format!("No palette found for variable {}", variable.name))?;

            if let Some(values) = values {
                write_to_pngwjson(&file, &grid, values, &palette)
                    .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
            }
            
        }
        Ok(())
    }
}
