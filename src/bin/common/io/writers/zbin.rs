use std::path::PathBuf;

use chrono::{DateTime, Utc};
use log::debug;
use rayon::prelude::*;
use risico::models::output::Output;

use crate::common::{
    helpers::RISICOError,
    io::models::{grid::RegularGrid, output::OutputVariable},
};

use super::{helpers::write_to_zbin_file, prelude::OutputSink};

pub struct ZBinWriter {
    path: PathBuf,
    name: String,
    run_date: DateTime<Utc>,
}

impl ZBinWriter {
    pub fn new(path: &str, name: &str, run_date: &DateTime<Utc>) -> Self {
        Self {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: *run_date,
        }
    }
}

impl OutputSink for ZBinWriter {
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
                    "{}/{}_{}_{}_{}.zbin",
                    path,
                    self.name,
                    run_date,
                    date_string,
                    variable.name()
                );

                debug!("[ZBIN] Writing variable {} to {:?}", variable.name(), file);
                let values = variable.get_variable_on_grid(output, lats, lons, grid);

                if let Some(values) = values {
                    write_to_zbin_file(&file, grid, values.as_slice().expect("Should unwrap"))
                        .map_err(|err| format!("Cannot write file {}: error {err}", file))?;

                    debug!(
                        "[ZBIN] Done writing variable {} to {:?}",
                        variable.name(),
                        file
                    );
                }
                Ok(())
            })
            .collect();

        super::helpers::extract_errors("ZBIN Errors", results)
    }
}
