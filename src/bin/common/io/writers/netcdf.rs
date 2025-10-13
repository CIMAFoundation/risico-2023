use std::{collections::HashMap, path::PathBuf, sync::Mutex};

use log::debug;
use netcdf::{extent::Extents, MutableFile};
use rayon::prelude::*;
use risico::models::output::Output;

use crate::common::{
    helpers::RISICOError,
    io::models::{grid::RegularGrid, output::OutputVariable},
};

use super::{
    helpers::{create_nc_file, extract_errors},
    prelude::OutputSink,
};

pub struct NetcdfWriter {
    path: PathBuf,
    files: HashMap<String, Mutex<MutableFile>>,
}

impl NetcdfWriter {
    pub fn new(path: &str) -> Self {
        Self {
            path: PathBuf::from(path),
            files: HashMap::new(),
        }
    }
}

impl OutputSink for NetcdfWriter {
    fn write(
        &mut self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let n_lats = grid.nrows;
        let n_lons = grid.ncols;

        let path = self.path.as_os_str().to_str().expect("Invalid path");

        for variable in variables {
            if self.files.contains_key(variable.name()) {
                continue;
            }

            let file_name = format!("{}/{}.nc", path, variable.name());
            let file = create_nc_file(&file_name, grid, variable.name(), variable.internal_name())?;
            self.files
                .insert(variable.name().to_string(), Mutex::new(file));
        }

        let results: Vec<Result<(), RISICOError>> = variables
            .par_iter()
            .map(|variable| {
                let mutex = self
                    .files
                    .get(variable.name())
                    .expect("there should be a file");

                let mut file = mutex.lock().expect("unable to lock netcdf writer");

                debug!(
                    "[NC] Writing variable {} to {:?}",
                    variable.name(),
                    file.path().expect("Should have a path")
                );

                let mut time_var = file
                    .variable_mut("time")
                    .ok_or_else(|| "variable not found: time".to_string())?;
                let time: i64 = output.time.timestamp();
                let len = time_var.len();
                let extents: Extents = (&[len], &[1]).try_into().expect("Should convert");

                time_var
                    .put_values(&[time], extents)
                    .unwrap_or_else(|_| panic!("Add time failed"));

                let mut variable_var = file
                    .variable_mut(variable.name())
                    .ok_or_else(|| format!("variable not found: {}", variable.name()))?;

                let values = variable.get_variable_on_grid(output, lats, lons, grid);
                let extents: Extents = (&[len, 0, 0], &[1, n_lats, n_lons])
                    .try_into()
                    .expect("Should convert");
                if let Some(values) = values {
                    variable_var
                        .put_values(values.as_slice().expect("Should unwrap"), extents)
                        .unwrap_or_else(|err| panic!("Add variable failed: {err}"));

                    debug!(
                        "[NC] Done Writing variable {} to {:?}",
                        variable.name(),
                        file.path().expect("Should have a path")
                    );
                }
                Ok(())
            })
            .collect();

        extract_errors("NC Errors", results)
    }
}
