use std::{collections::HashMap, path::PathBuf, sync::Mutex};

use chrono::{DateTime, Utc};

use log::debug;
use ndarray::{Array1, Zip};
use netcdf::{extent::Extents, MutableFile};
use rayon::prelude::*;
use risico::{
    constants::NODATAVAL,
    models::output::{Output, OutputVariableName},
};
use serde_derive::{Deserialize, Serialize};

#[cfg(feature = "gdal")]
use crate::common::io::writers::write_to_geotiff;

use crate::common::{
    config::{builder::OutputTypeConfig, models::PaletteMap},
    helpers::RISICOError,
    io::writers::{create_nc_file, write_to_pngwjson, write_to_zbin_file},
};

use super::grid::{ClusterMode, Grid, RegularGrid};

/// Extract error message generated from writing variables to files
fn extract_errors(
    error_message: &str,
    results: Vec<Result<(), RISICOError>>,
) -> Result<(), RISICOError> {
    let error_messages: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        })
        .collect();

    if !error_messages.is_empty() {
        let all_messages = error_messages.join("\n");
        Err(RISICOError::from(format!(
            "{}: {}",
            error_message, all_messages
        )))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputVariable {
    internal_name: OutputVariableName,
    name: String,
    cluster_mode: ClusterMode,
    precision: i32,
}

impl OutputVariable {
    pub fn new(
        internal_name: OutputVariableName,
        name: &str,
        cluster_mode: ClusterMode,
        precision: i32,
    ) -> Self {
        Self {
            internal_name,
            name: name.to_string(),
            cluster_mode,
            precision,
        }
    }

    pub fn get_variable_on_grid(
        &self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
    ) -> Option<Array1<f32>> {
        let values = output.get(&self.internal_name);

        let values = values?;
        let cutval = f32::powi(10.0, self.precision);

        let n_pixels = grid.nrows * grid.ncols;
        let mut grid_values: Array1<f32> = Array1::ones(n_pixels) * NODATAVAL;

        let indexes_and_values: Vec<(usize, f32)> = Zip::from(lats)
            .and(lons)
            .and(&values)
            .into_par_iter()
            .filter_map(|(lat, lon, value)| {
                if *value == NODATAVAL {
                    return None;
                }

                grid.index(lat, lon).map(|idx| (idx, *value))
            })
            .collect();

        if indexes_and_values.is_empty() {
            return Some(grid_values);
        }

        indexes_and_values.iter().for_each(|(idx, value)| {
            let idx = *idx;
            let value = *value;
            let prev_value = grid_values[idx];

            if prev_value == NODATAVAL {
                grid_values[idx] = value;
            } else {
                match self.cluster_mode {
                    ClusterMode::Mean => grid_values[idx] += value,
                    ClusterMode::Min => grid_values[idx] = f32::min(prev_value, value),
                    ClusterMode::Max => grid_values[idx] = f32::max(prev_value, value),
                    _ => unimplemented!("Median mode not implemented yet"),
                }
            }
        });

        if let ClusterMode::Mean = self.cluster_mode {
            let mut grid_count: Array1<f32> = Array1::zeros(n_pixels);
            indexes_and_values
                .iter()
                .for_each(|(idx, _)| grid_count[*idx] += 1.0);

            let grid_count = grid_count.mapv(|v| if v == 0.0 { 1.0 } else { v });
            grid_values = grid_values / grid_count;
        }

        // apply cutval
        let grid_values = grid_values.mapv(|v| {
            if v == NODATAVAL {
                NODATAVAL
            } else {
                (v / cutval).round() * cutval
            }
        });

        Some(grid_values)
    }
}

pub struct OutputType {
    // pub internal_name: String,
    name: String,
    // path: String,
    grid: RegularGrid,
    format: String,
    variables: Vec<OutputVariable>,
    // palettes: PaletteMap,
    // run_date: DateTime<Utc>,
    writer: Box<dyn Writer>,
}

unsafe impl Send for OutputType {}

impl OutputType {
    pub fn new(
        output_type_def: &OutputTypeConfig,
        run_date: &DateTime<Utc>,
        palettes: &PaletteMap,
    ) -> Result<Self, RISICOError> {
        let grid_path = &output_type_def.grid_path;
        // let internal_name = &output_type_def.internal_name;
        let name = &output_type_def.name;
        let path = &output_type_def.path;
        let format = &output_type_def.format;

        let grid = RegularGrid::from_txt_file(grid_path)?;

        let writer: Box<dyn Writer> = match format.as_str() {
            "ZBIN" => Box::new(ZBinWriter::new(path, name, run_date)),
            "PNGWJSON" => Box::new(PngWriter::new(path, name, palettes, run_date)),
            "NETCDF" => Box::new(NetcdfWriter::new(path)),
            #[cfg(feature = "gdal")]
            "GEOTIFF" => Box::new(GeotiffWriter::new(path, name, run_date)),
            _ => Box::new(ZBinWriter::new(path, name, run_date)),
        };

        let variables = output_type_def
            .variables
            .iter()
            .map(|var| {
                OutputVariable::new(
                    var.internal_name,
                    &var.name,
                    var.cluster_mode,
                    var.precision,
                )
            })
            .collect();

        Ok(Self {
            // internal_name: internal_name.to_string(),
            name: name.to_string(),
            // path: path.to_string(),
            grid,
            format: format.to_string(),
            variables,
            // palettes: palettes.clone(),
            // run_date: *run_date,
            writer,
        })
    }

    // pub fn add_variable(&mut self, variable: OutputVariable) {
    //     self.variables.push(variable);
    // }

    pub fn write_variables(
        &mut self,
        lats: &[f32],
        lons: &[f32],
        output: &Output,
    ) -> Result<(), RISICOError> {
        debug!("Writing variables for {}, {}", self.name, self.format);
        let res = self
            .writer
            .write(output, lats, lons, &self.grid, &self.variables);
        debug!("Done Writing variables for {}, {}", self.name, self.format);
        res
    }
}

#[derive(Debug)]
struct NetcdfWriter {
    path: PathBuf,
    // name: String,
    // run_date: DateTime<Utc>,
    files: HashMap<String, Mutex<MutableFile>>,
}

impl NetcdfWriter {
    fn new(path: &str) -> Self {
        Self {
            path: PathBuf::from(path),
            // name: name.to_string(),
            // run_date: *run_date,
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
            run_date: *run_date,
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
            run_date: *run_date,
            palettes: palettes.clone(),
        }
    }
}

trait Writer {
    fn write(
        &mut self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError>;
}

impl Writer for NetcdfWriter {
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

        for variable in variables {
            if self.files.contains_key(&variable.name) {
                continue;
            }
            let path = self.path.as_os_str().to_str().expect("Invalid path");

            let file_name = format!("{}/{}.nc", path, variable.name);
            let file = create_nc_file(&file_name, grid, &variable.name, variable.internal_name)?;
            self.files.insert(variable.name.clone(), Mutex::new(file));
        }

        let results: Vec<Result<(), RISICOError>> = variables
            .par_iter()
            .map(|variable| {
                let mutex = self
                    .files
                    .get(&variable.name)
                    .expect("there should be a file");

                let mut file = mutex.lock().expect("");

                debug!(
                    "[NC] Writing variable {} to {:?}",
                    variable.name,
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
                    .variable_mut(&variable.name)
                    .ok_or_else(|| format!("variable not found: {}", variable.name))?;

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
                        variable.name,
                        file.path().expect("Should have a path")
                    );
                }
                Ok(())
            })
            .collect();

        extract_errors("NC Errors", results)
    }
}

impl Writer for ZBinWriter {
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
                //todo!("get run date from config");
                let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
                let file = format!(
                    "{}/{}_{}_{}_{}.zbin",
                    path, self.name, run_date, date_string, variable.name
                );

                debug!("[ZBIN] Writing variable {} to {:?}", variable.name, file);
                let values = variable.get_variable_on_grid(output, lats, lons, grid);

                if let Some(values) = values {
                    write_to_zbin_file(&file, grid, values.as_slice().expect("Should unwrap"))
                        .map_err(|err| format!("Cannot write file {}: error {err}", file))?;

                    debug!(
                        "[ZBIN] Done writing variable {} to {:?}",
                        variable.name, file
                    );
                }
                Ok(())
            })
            .collect();
        extract_errors("ZBIN Errors", results)
    }
}

impl Writer for PngWriter {
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
                //todo!("get run date from config");
                let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
                let file = format!(
                    "{}/{}_{}_{}_{}.png",
                    path, self.name, run_date, date_string, variable.name
                );

                debug!("[PNG] Writing variable {} to {:?}", variable.name, file);

                let values = variable.get_variable_on_grid(output, lats, lons, grid);
                let palette = self
                    .palettes
                    .get(&variable.name)
                    .ok_or(format!("No palette found for variable {}", variable.name))?;

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
                        variable.name, file
                    );
                }
                Ok(())
            })
            .collect();
        // check if there are any errors
        extract_errors("PNG Errors", results)
    }
}

#[cfg(feature = "gdal")]
pub struct GeotiffWriter {
    path: PathBuf,
    name: String,
    run_date: DateTime<Utc>,
}
#[cfg(feature = "gdal")]
impl GeotiffWriter {
    pub fn new(path: &str, name: &str, run_date: &DateTime<Utc>) -> Self {
        GeotiffWriter {
            path: PathBuf::from(path),
            name: name.to_string(),
            run_date: run_date.clone(),
        }
    }
}
#[cfg(feature = "gdal")]
impl Writer for GeotiffWriter {
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

        let results: Vec<Result<(), RISICOError>> = variables.par_iter().map(|variable| {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
            let file = format!(
                "{}/{}_{}_{}_{}.tif",
                path, self.name, run_date, date_string, variable.name
            );

            debug!("[GEOTIFF] Writing variable {} to {:?}", variable.name, file);
            let values = variable.get_variable_on_grid(&output, lats, lons, grid);

            if let Some(values) = values {
                write_to_geotiff(&file, &grid, values.as_slice().expect("Should unwrap"))
                    .map_err(|err| format!("Cannot write file {}: error {err}", file))?;

                debug!(
                    "[GEOTIFF] Done writing variable {} to {:?}",
                    variable.name, file
                );
            }
            Ok(())
        });
        extract_errors("GEOTiff Errors", results)
    }
}
