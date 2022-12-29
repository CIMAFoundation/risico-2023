use crate::library::{state::models::{Output}, config::models::ConfigError, io::writers::write_to_zbin_file};

use super::grid::{ClusterMode, RegularGrid };

#[derive(Debug)]
pub struct OutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: ClusterMode,
    precision: i32,
}

impl OutputVariable {
   pub fn get_variable_on_grid(&self, lats: &[f32], lons: &[f32], output: &Output, grid: &RegularGrid) -> Vec<f32>{
        let values = output.get(&self.internal_name);
        let values = values.as_slice().unwrap();
        let values = grid.project_to_grid(&lats, &lons, values, &self.cluster_mode);
        // transform to desired number of decimal places precision
        let cutval = f32::powi(10.0, self.precision);
        let values = values.iter().map(|val| {
            f32::ceil(val / cutval - 0.5) * cutval
        }).collect();
        values
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

#[derive(Debug)]
pub struct OutputType {
    name: String,
    path: String,
    grid: RegularGrid,
    format: String,
    variables: Vec<OutputVariable>,
}

impl OutputType {
    pub fn new(name: &str, path: &str, grid_path: &str, format: &str) -> Result<Self, ConfigError> {
        let grid = RegularGrid::from_txt_file(grid_path).unwrap();
        Ok(Self {
            name: name.to_string(),
            path: path.to_string(),
            grid: grid,
            format: format.to_string(),
            variables: Vec::new(),
        })
    }

    pub fn add_variable(&mut self, variable: OutputVariable) {
        self.variables.push(variable);
    }

    fn write_zbin(&self, output: &Output, lats: &[f32], lons: &[f32]) -> Result<(), ConfigError> {
        let grid = self.grid;
        for variable in &self.variables {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = "202102010000";
            let file = format!("{}/{}_{}_{}_{}.zbin", self.path, self.name, run_date, date_string, variable.name);
            let values = variable.get_variable_on_grid(&lats, &lons, &output, &grid);
            
            write_to_zbin_file(&file, &grid, values)
                .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
        }
        Ok(())
    }

    pub fn write_variables(&self, output: &Output, lats: &[f32], lons:&[f32]) -> Result<(), ConfigError>{
        match self.format.as_str() {
            "ZBIN" => self.write_zbin(output, lats, lons),
            _ => Ok(())
        }
    }
}
