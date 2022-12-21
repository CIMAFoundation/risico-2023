use crate::library::state::models::{CellOutput, State};

use super::grid::{ClusterMode, RegularGrid, GridFunctions};

#[derive(Debug)]
pub struct OutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: ClusterMode,
    precision: i32,
}

impl OutputVariable {
   pub fn get_variable_on_grid(&self, state: &State, grid: &mut RegularGrid) -> Vec<f32>{
        let fun = CellOutput::get(&self.internal_name);
        let values = state.cells.iter().map(|cell| {
            fun(&cell.output.as_ref().unwrap()) as f32
        }).collect();
        let cells = &state.cells;
        let lats = cells.iter().map(
            |cell| cell.properties.lat as f32
        ).collect();

        let lons = cells.iter().map(|cell| {
            cell.properties.lon as f32
        }).collect();


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
    grid: Box<dyn GridFunctions>,
    format: String,
    variables: Vec<OutputVariable>,
}

impl OutputType {
    pub fn new(name: &str, path: &str, grid_path: &str, format: &str) -> Self {
        let grid = RegularGrid::from_txt_file(grid_path).unwrap();
        Self {
            name: name.to_string(),
            path: path.to_string(),
            grid: Box::new(grid),
            format: format.to_string(),
            variables: Vec::new(),
        }
    }

    pub fn add_variable(&mut self, variable: OutputVariable) {
        self.variables.push(variable);
    }
}

// struct OutputWriter {
//     pub outputTypes: Vec<OutputType>,
// }

// impl OutputWriter {
//     fn write_output(&self, time: &DateTime<Utc>, state: &State) {
//         let file = format!("data/output/dffm_{}.zbin", time.format("%Y%m%d%H%M%S"));
//     }
// }