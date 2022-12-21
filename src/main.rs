#![allow(dead_code)]



use chrono::{DateTime, Utc};
use library::{config::models::{ConfigBuilder, Config}, io::models::grid::{ClusterMode, RegularGrid, GridFunctions}, state::models::{State, CellOutput}};
mod library;

#[derive(Debug)]
struct OutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: ClusterMode,
    precision: i32,
}

impl OutputVariable {
   fn get_variable_on_grid(&self, state: &State, grid: &mut RegularGrid) -> Vec<f32>{
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
}

#[derive(Debug)]
pub struct OutputType {
    name: String,
    path: String,
    grid: Box<dyn GridFunctions>,
    format: String,
    variables: Vec<OutputVariable>,
}

struct OutputWriter {
    pub outputTypes: Vec<OutputType>,
}

impl OutputWriter {
    fn write_output(&self, time: &DateTime<Utc>, state: &State) {
        
        let file = format!("data/output/dffm_{}.zbin", time.format("%Y%m%d%H%M%S"));

    }
}
fn main(){
    let config_builder = ConfigBuilder::new("data/config.txt")
        .unwrap();

    print!("{:#?}", config_builder)
}
