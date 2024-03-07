use std::{collections::HashMap, path::PathBuf};

use chrono::{DateTime, Utc};
use itertools::izip;
use ndarray::Array1;
use netcdf::{extent::Extents, MutableFile};

use crate::library::{
    config::models::{PaletteMap, RISICOError},
    io::writers::{create_nc_file, write_to_pngwjson, write_to_zbin_file},
    state::{constants::NODATAVAL, models::Output},
};

#[cfg(feature = "gdal")]
use crate::library::io::writers::write_to_geotiff;

use super::grid::{ClusterMode, Grid, RegularGrid};

#[derive(Debug)]
pub struct OutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: ClusterMode,
    precision: i32,
}

impl OutputVariable {
    pub fn new(internal_name: &str, name: &str, cluster_mode: ClusterMode, precision: i32) -> Self {
        Self {
            internal_name: internal_name.to_string(),
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

        let values = if let Some(values) = values {
            values
        } else {
            return None;
        };
        let cutval = f32::powi(10.0, self.precision);

        let n_pixels = grid.nrows * grid.ncols as usize;
        let mut grid_values: Array1<f32> = Array1::ones(n_pixels) * NODATAVAL;
        let mut grid_count: Array1<f32> = Array1::ones(n_pixels);

        izip!(lats, lons, values).for_each(|(lat, lon, value)| {
            if let Some(idx) = grid.index(&lat, &lon) {
                if value == NODATAVAL {
                    return;
                }

                let prev_value = grid_values[idx];

                if prev_value == NODATAVAL {
                    grid_values[idx] = value;
                } else {
                    match self.cluster_mode {
                        ClusterMode::Mean => {
                            grid_values[idx] += value;
                            grid_count[idx] += 1.0;
                        }
                        ClusterMode::Min => grid_values[idx] = f32::min(prev_value, value),
                        ClusterMode::Max => grid_values[idx] = f32::max(prev_value, value),
                        _ => unimplemented!("Median mode not implemented yet"),
                    }
                }
            }
        });

        let grid_values = grid_values / grid_count;
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
    pub internal_name: String,
    name: String,
    path: String,
    grid: RegularGrid,
    format: String,
    variables: Vec<OutputVariable>,
    palettes: PaletteMap,
    run_date: DateTime<Utc>,
    writer: Box<dyn Writer>,
}

unsafe impl Send for OutputType {}

impl OutputType {
    pub fn new(
        internal_name: &str,
        name: &str,
        path: &str,
        grid_path: &str,
        format: &str,
        run_date: &DateTime<Utc>,
        palettes: PaletteMap,
    ) -> Result<Self, RISICOError> {
        let grid = RegularGrid::from_txt_file(grid_path)?;

        let writer: Box<dyn Writer> = match format {
            "ZBIN" => Box::new(ZBinWriter::new(path, name, run_date)),
            "PNGWJSON" => Box::new(PngWriter::new(path, name, &palettes, run_date)),
            "NETCDF" => Box::new(NetcdfWriter::new(path, name, run_date)),
            #[cfg(feature = "gdal")]
            "GEOTIFF" => Box::new(GeotiffWriter::new(path, name, run_date)),
            _ => Box::new(ZBinWriter::new(path, name, run_date)),
        };

        Ok(Self {
            internal_name: internal_name.to_string(),
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
        lats: &[f32],
        lons: &[f32],
        output: &Output,
    ) -> Result<(), RISICOError> {
        self.writer
            .write(output, lats, lons, &self.grid, &self.variables)
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
        for variable in variables {
            let n_lats = grid.nrows;
            let n_lons = grid.ncols;

            if !self.files.contains_key(&variable.name) {
                let path = self.path.as_os_str().to_str().expect("Invalid path");

                let file_name = format!("{}/{}.nc", path, variable.name);
                let file = create_nc_file(&file_name, grid, &variable.name)?;
                self.files.insert(variable.name.clone(), file);
            }

            let file = self
                .files
                .get_mut(&variable.name)
                .expect("there should be file");

            let mut time_var = file
                .variable_mut("time")
                .ok_or_else(|| format!("variable not found: time"))?;
            let time: i64 = output.time.timestamp() as i64;
            let len = time_var.len();
            let extents: Extents = (&[len], &[1]).try_into().unwrap();

            time_var
                .put_values(&[time], extents)
                .unwrap_or_else(|_| panic!("Add time failed"));

            let mut variable_var = file
                .variable_mut(&variable.name)
                .ok_or_else(|| format!("variable not found: {}", variable.name))?;

            let values = variable.get_variable_on_grid(&output, lats, lons, &grid);
            let extents: Extents = (&[len, 0, 0], &[1, n_lats, n_lons]).try_into().unwrap();
            if let Some(values) = values {
                variable_var
                    .put_values(values.as_slice().expect("Should unwrap"), extents)
                    .unwrap_or_else(|err| panic!("Add variable failed: {err}"));
            } else {
                continue;
            }
        }
        Ok(())
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
        for variable in variables {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
            let file = format!(
                "{}/{}_{}_{}_{}.zbin",
                path, self.name, run_date, date_string, variable.name
            );
            let values = variable.get_variable_on_grid(&output, lats, lons, grid);

            if let Some(values) = values {
                write_to_zbin_file(&file, &grid, values.as_slice().expect("Should unwrap"))
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
        for variable in variables {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
            let file = format!(
                "{}/{}_{}_{}_{}.png",
                path, self.name, run_date, date_string, variable.name
            );
            let values = variable.get_variable_on_grid(&output, lats, lons, grid);
            let palette = self
                .palettes
                .get(&variable.name)
                .ok_or(format!("No palette found for variable {}", variable.name))?;

            if let Some(values) = values {
                write_to_pngwjson(
                    &file,
                    &grid,
                    values.as_slice().expect("Should unwrap"),
                    &palette,
                )
                .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
            }
        }
        Ok(())
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
        for variable in variables {
            let date_string = output.time.format("%Y%m%d%H%M").to_string();
            //todo!("get run date from config");
            let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
            let file = format!(
                "{}/{}_{}_{}_{}.tif",
                path, self.name, run_date, date_string, variable.name
            );
            let values = variable.get_variable_on_grid(&output, lats, lons, grid);

            if let Some(values) = values {
                write_to_geotiff(&file, &grid, values.as_slice().expect("Should unwrap"))
                    .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
            }
        }
        Ok(())
    }
}
