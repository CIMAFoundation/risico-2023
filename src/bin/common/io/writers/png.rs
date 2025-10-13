use std::path::PathBuf;

use chrono::{DateTime, Utc};
use log::debug;
use rayon::prelude::*;
use risico::models::output::Output;

use crate::common::{
    config::models::PaletteMap,
    helpers::RISICOError,
    io::models::{grid::RegularGrid, output::OutputVariable},
};

use super::{helpers::write_to_pngwjson, prelude::OutputSink};

pub struct PngWriter {
    path: PathBuf,
    name: String,
    palettes: PaletteMap,
    run_date: DateTime<Utc>,
}

impl PngWriter {
    pub fn new(path: &str, name: &str, palettes: &PaletteMap, run_date: &DateTime<Utc>) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            palettes: palettes.clone(),
            run_date: *run_date,
        }
    }
}

impl OutputSink for PngWriter {
    fn write(
        &mut self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let path = self
            .path
            .as_os_str()
            .to_str()
            .expect("Should be a valid path");

        let results: Vec<Result<(), RISICOError>> = variables
            .par_iter()
            .map(|variable| {
                let date_string = output.time.format("%Y%m%d%H%M").to_string();
                let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
                let file = format!(
                    "{}/{}_{}_{}_{}.png",
                    path,
                    self.name,
                    run_date,
                    date_string,
                    variable.name()
                );

                debug!("[PNG] Writing variable {} to {:?}", variable.name(), file);

                let values = variable.get_variable_on_grid(output, lats, lons, grid);
                let palette = self
                    .palettes
                    .get(variable.name())
                    .ok_or(format!("No palette found for variable {}", variable.name()))?;

                if let Some(values) = values {
                    write_to_pngwjson(
                        &file,
                        grid,
                        values.as_slice().expect("Should unwrap"),
                        palette,
                    )
                    .map_err(|err| format!("Cannot write file {}: error {err}", file))?;

                    debug!(
                        "[PNG] Done writing variable {} to {:?}",
                        variable.name(),
                        file
                    );
                }
                Ok(())
            })
            .collect();

        super::helpers::extract_errors("PNG Errors", results)
    }
}
