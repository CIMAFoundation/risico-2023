use chrono::{DateTime, Utc};

use log::debug;
use ndarray::{Array1, Zip};
use rayon::prelude::*;
use risico::{
    constants::NODATAVAL,
    models::output::{Output, OutputVariableName},
};
use serde_derive::{Deserialize, Serialize};

use crate::common::{
    config::{builder::OutputTypeConfig, models::PaletteMap},
    helpers::RISICOError,
    io::writers::{
        netcdf::NetcdfWriter, png::PngWriter, prelude::OutputSink, zarr::ZarrWriter,
        zbin::ZBinWriter,
    },
};

#[cfg(feature = "gdal")]
use crate::common::io::writers::geotiff::GeotiffWriter;

use super::grid::{ClusterMode, Grid, RegularGrid};

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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn internal_name(&self) -> OutputVariableName {
        self.internal_name
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
    writer: Box<dyn OutputSink>,
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

        let writer: Box<dyn OutputSink> = match format.as_str() {
            "ZBIN" => Box::new(ZBinWriter::new(path, name, run_date)),
            "PNGWJSON" => Box::new(PngWriter::new(path, name, palettes, run_date)),
            "NETCDF" => Box::new(NetcdfWriter::new(path)),
            "ZARR" => Box::new(ZarrWriter::new(path, name, run_date)),
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
