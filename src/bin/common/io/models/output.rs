use std::{collections::HashMap, fs, path::PathBuf, sync::RwLock};

use chrono::{DateTime, Utc};

use log::debug;
use ndarray::Array1;
use rayon::prelude::*;
use risico::{
    constants::NODATAVAL,
    models::output::{Output, OutputVariableName},
};

#[cfg(feature = "gdal")]
use crate::common::io::writers::write_to_geotiff;

use crate::common::{
    config::{builder::OutputTypeConfig, models::PaletteMap},
    helpers::RISICOError,
    io::{
        streaming::TiledNativeOutputs,
        writers::{create_nc4_file, write_to_pngwjson, write_to_zbin_file, Nc4Output},
        zarr::ZarrWriter,
    },
};

use super::grid::{ClusterMode, Grid, RegularGrid};

pub use risico::config::OutputVariable;

/// Resampling of an output variable onto its output grid.
pub trait OutputVariableExt {
    fn get_variable_on_grid(
        &self,
        output: &dyn NativeOutputSource,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        parallelism: usize,
    ) -> Result<Option<Array1<f32>>, RISICOError>;
}

/// Resample a chunk of native values onto the output grid.
///
/// The values are read in native order and combined in place, so nothing
/// domain-sized is allocated: the accumulator is one output grid, whatever
/// the domain's size.
fn accumulate_chunk(
    cluster_mode: ClusterMode,
    accumulator: &mut GridAccumulator,
    values: &[f32],
    positions: ChunkPositions<'_>,
    lats: &[f32],
    lons: &[f32],
    grid: &RegularGrid,
) {
    let mut accumulate = |value: f32, position: usize| {
        if value == NODATAVAL {
            return;
        }
        if let Some(index) = grid.index(&lats[position], &lons[position]) {
            accumulator.add(index, value, cluster_mode);
        }
    };

    match positions {
        ChunkPositions::Contiguous(offset) => {
            for (cell, &value) in values.iter().enumerate() {
                accumulate(value, offset + cell);
            }
        }
        ChunkPositions::Indexed(positions) => {
            for (&value, &position) in values.iter().zip(positions) {
                accumulate(value, position);
            }
        }
    }
}

impl OutputVariableExt for OutputVariable {
    /// Resample this variable's whole native output onto the output grid.
    ///
    /// Chunks are shared out over `parallelism` accumulators, so the memory
    /// this costs is bounded by the output grid and the largest chunk rather
    /// than by the domain.
    fn get_variable_on_grid(
        &self,
        output: &dyn NativeOutputSource,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        parallelism: usize,
    ) -> Result<Option<Array1<f32>>, RISICOError> {
        if !output.has_variable(self.internal_name()) {
            return Ok(None);
        }
        let n_pixels = grid.nrows * grid.ncols;
        let chunks = output.chunk_count();
        let slots = parallelism.clamp(1, chunks.max(1));

        let accumulator = (0..slots)
            .into_par_iter()
            .map(|slot| -> Result<GridAccumulator, RISICOError> {
                let mut accumulator = GridAccumulator::new(n_pixels, self.cluster_mode());
                let mut buffer = Vec::new();
                for chunk in (slot..chunks).step_by(slots) {
                    let positions = output.read_chunk(self.internal_name(), chunk, &mut buffer)?;
                    accumulate_chunk(self.cluster_mode(), &mut accumulator, &buffer, positions, lats, lons, grid);
                }
                Ok(accumulator)
            })
            .try_reduce(GridAccumulator::empty, |left, right| {
                Ok(left.merge(right, self.cluster_mode()))
            })?;

        Ok(Some(accumulator.finish(
            n_pixels,
            self.cluster_mode(),
            f32::powi(10.0, self.precision()),
        )))
    }
}

/// One output grid under construction, with the counts a mean needs.
///
/// `NODATAVAL` marks a pixel no cell has reached yet, which is what makes the
/// accumulator mergeable: two halves of the domain can be combined pixel by
/// pixel without knowing which cells contributed to either.
struct GridAccumulator {
    values: Vec<f32>,
    counts: Vec<u32>,
}

impl GridAccumulator {
    fn new(n_pixels: usize, cluster_mode: ClusterMode) -> Self {
        Self {
            values: vec![NODATAVAL; n_pixels],
            counts: match cluster_mode {
                ClusterMode::Mean => vec![0; n_pixels],
                _ => Vec::new(),
            },
        }
    }

    /// The identity of a merge: it allocates nothing.
    fn empty() -> Self {
        Self {
            values: Vec::new(),
            counts: Vec::new(),
        }
    }

    fn add(&mut self, index: usize, value: f32, cluster_mode: ClusterMode) {
        let slot = &mut self.values[index];
        if *slot == NODATAVAL {
            *slot = value;
        } else {
            match cluster_mode {
                ClusterMode::Mean => *slot += value,
                ClusterMode::Min => *slot = f32::min(*slot, value),
                ClusterMode::Max => *slot = f32::max(*slot, value),
                ClusterMode::Median => unimplemented!("Median mode not implemented yet"),
            }
        }
        if let ClusterMode::Mean = cluster_mode {
            self.counts[index] += 1;
        }
    }

    fn merge(mut self, other: Self, cluster_mode: ClusterMode) -> Self {
        if other.values.is_empty() {
            return self;
        }
        if self.values.is_empty() {
            return other;
        }
        for (slot, value) in self.values.iter_mut().zip(&other.values) {
            if *value == NODATAVAL {
                continue;
            }
            if *slot == NODATAVAL {
                *slot = *value;
            } else {
                match cluster_mode {
                    ClusterMode::Mean => *slot += *value,
                    ClusterMode::Min => *slot = f32::min(*slot, *value),
                    ClusterMode::Max => *slot = f32::max(*slot, *value),
                    ClusterMode::Median => unimplemented!("Median mode not implemented yet"),
                }
            }
        }
        for (slot, count) in self.counts.iter_mut().zip(&other.counts) {
            *slot += *count;
        }
        self
    }

    /// Turn the sums into the configured statistic, rounded to the variable's
    /// precision, in place.
    fn finish(mut self, n_pixels: usize, cluster_mode: ClusterMode, cutval: f32) -> Array1<f32> {
        if self.values.is_empty() {
            // No chunk contributed anything: the grid is entirely nodata.
            return Array1::ones(n_pixels) * NODATAVAL;
        }
        for (index, value) in self.values.iter_mut().enumerate() {
            if *value == NODATAVAL {
                continue;
            }
            if let ClusterMode::Mean = cluster_mode {
                let count = self.counts[index];
                if count > 1 {
                    *value /= count as f32;
                }
            }
            *value = (*value / cutval).round() * cutval;
        }
        Array1::from(self.values)
    }
}

/// Where a chunk's values sit in the model domain.
#[derive(Clone, Copy, Debug)]
pub enum ChunkPositions<'a> {
    /// The chunk covers `offset..offset + len` in domain order.
    Contiguous(usize),
    /// The chunk covers these domain positions, in the order it stores them.
    Indexed(&'a [usize]),
}

/// Native-grid values consumed by the output postprocessor.
///
/// The postprocessor reads a variable one chunk at a time rather than as a
/// whole domain, so a timestep's peak memory follows the chunk size instead of
/// the domain. Model tiles and the whole-domain `Output` both implement it;
/// production execution writes through `TiledNativeOutputs`, whose chunks are
/// its tiles.
pub trait NativeOutputSource: Sync {
    fn time(&self) -> DateTime<Utc>;

    /// Whether this source carries the variable at all.
    fn has_variable(&self, variable: OutputVariableName) -> bool;

    fn chunk_count(&self) -> usize;

    /// Read one chunk of a variable into `destination`, replacing its contents,
    /// and report which domain cells the values belong to.
    fn read_chunk<'a>(
        &'a self,
        variable: OutputVariableName,
        chunk: usize,
        destination: &mut Vec<f32>,
    ) -> Result<ChunkPositions<'a>, RISICOError>;
}

impl NativeOutputSource for Output {
    fn time(&self) -> DateTime<Utc> {
        self.time
    }

    fn has_variable(&self, variable: OutputVariableName) -> bool {
        self.get(&variable).is_some()
    }

    fn chunk_count(&self) -> usize {
        1
    }

    fn read_chunk<'a>(
        &'a self,
        variable: OutputVariableName,
        chunk: usize,
        destination: &mut Vec<f32>,
    ) -> Result<ChunkPositions<'a>, RISICOError> {
        if chunk != 0 {
            return Err(format!("whole-domain output has no chunk {chunk}").into());
        }
        let values = self
            .get(&variable)
            .ok_or_else(|| RISICOError::from(format!("output does not expose {variable}")))?;
        destination.clear();
        destination.extend_from_slice(values.as_slice().expect("model output is contiguous"));
        Ok(ChunkPositions::Contiguous(0))
    }
}

/// Presents per-tile output scratches to the writers as one domain array.
pub struct TiledOutputSource<'a> {
    time: DateTime<Utc>,
    output: &'a TiledNativeOutputs<'a>,
}

impl<'a> TiledOutputSource<'a> {
    pub fn new(time: DateTime<Utc>, output: &'a TiledNativeOutputs<'a>) -> Self {
        Self { time, output }
    }
}

impl NativeOutputSource for TiledOutputSource<'_> {
    fn time(&self) -> DateTime<Utc> {
        self.time
    }

    fn has_variable(&self, variable: OutputVariableName) -> bool {
        self.output.has_variable(variable)
    }

    fn chunk_count(&self) -> usize {
        self.output.tile_count()
    }

    fn read_chunk<'a>(
        &'a self,
        variable: OutputVariableName,
        chunk: usize,
        destination: &mut Vec<f32>,
    ) -> Result<ChunkPositions<'a>, RISICOError> {
        let positions = self.output.read_tile(chunk, variable, destination)?;
        Ok(ChunkPositions::Indexed(positions))
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
            "NETCDF" => Box::new(NetcdfWriter::new(path, output_type_def.compression_level)),
            "ZARR" => Box::new(ZarrWriter::new(
                path,
                name,
                run_date,
                output_type_def.compression_level,
            )),
            #[cfg(feature = "gdal")]
            "GEOTIFF" => Box::new(GeotiffWriter::new(path, name, run_date)),
            other => {
                return Err(format!(
                    "output type {name} has unknown format {other}; \
                     expected ZBIN, NETCDF, ZARR, PNGWJSON or GEOTIFF"
                )
                .into())
            }
        };

        let variables = output_type_def
            .variables
            .iter()
            .map(|var| {
                OutputVariable::new(
                    var.internal_name(),
                    var.output_name(),
                    var.cluster_mode(),
                    var.precision(),
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

    pub fn variables(&self) -> &[OutputVariable] {
        &self.variables
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn grid(&self) -> &RegularGrid {
        &self.grid
    }

    /// Open whatever this output type writes into before any variable is
    /// resampled, so the writes themselves need no exclusive access and can run
    /// alongside each other.
    pub fn prepare(&mut self) -> Result<(), RISICOError> {
        self.writer.prepare(&self.grid, &self.variables)
    }

    /// Complete this output type after the last timestep.
    pub fn finish(&mut self) -> Result<(), RISICOError> {
        self.writer.finish()
    }

    /// Resample one variable onto this output type's grid and write it.
    pub fn write_variable(
        &self,
        variable: &OutputVariable,
        output: &dyn NativeOutputSource,
        lats: &[f32],
        lons: &[f32],
        parallelism: usize,
    ) -> Result<(), RISICOError> {
        debug!(
            "Writing variable {} for {}, {}",
            variable.output_name(), self.name, self.format
        );
        let values = variable.get_variable_on_grid(output, lats, lons, &self.grid, parallelism)?;
        let Some(values) = values else {
            return Ok(());
        };
        let result = self.writer.write_variable(
            variable,
            values.as_slice().expect("grid values are contiguous"),
            &self.grid,
            output.time(),
        );
        debug!(
            "Done writing variable {} for {}, {}",
            variable.output_name(), self.name, self.format
        );
        result
    }
}

/// One NetCDF-4 file per variable, written by the pure-Rust streaming
/// writer: each timestep's grid is compressed on the calling thread and
/// appended as one chunk, and the file's metadata is written by `finish`.
struct NetcdfWriter {
    path: PathBuf,
    compression_level: i32,
    files: HashMap<String, RwLock<Nc4Output>>,
}

impl NetcdfWriter {
    fn new(path: &str, compression_level: i32) -> Self {
        Self {
            path: PathBuf::from(path),
            compression_level,
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

/// A destination for already-resampled output grids.
///
/// Writers take `&self` so that a timestep's variables can be written side by
/// side while each one is resampled on its own, holding a single output grid
/// rather than every variable at once.
pub(crate) trait Writer: Send + Sync {
    /// Open whatever the writer needs before the first variable arrives.
    fn prepare(
        &mut self,
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let _ = (grid, variables);
        Ok(())
    }

    fn write_variable(
        &self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError>;

    /// Complete the output after the last timestep. Writers that keep files
    /// open across the run write their metadata here.
    fn finish(&mut self) -> Result<(), RISICOError> {
        Ok(())
    }
}

impl Writer for NetcdfWriter {
    fn prepare(
        &mut self,
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        fs::create_dir_all(&self.path).map_err(|error| {
            format!(
                "cannot create NetCDF output directory {}: {error}",
                self.path.display()
            )
        })?;
        for variable in variables {
            if self.files.contains_key(&variable.name) {
                continue;
            }
            let file_name = self.path.join(format!("{}.nc", variable.name));
            let file = create_nc4_file(
                &file_name,
                grid,
                &variable.name,
                variable.internal_name,
                self.compression_level,
            )?;
            self.files.insert(variable.name.clone(), RwLock::new(file));
        }
        Ok(())
    }

    fn write_variable(
        &self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let file = self
            .files
            .get(&variable.name)
            .ok_or_else(|| format!("no open file for variable {}", variable.name))?;
        if values.len() != grid.nrows * grid.ncols {
            return Err(format!(
                "NetCDF sample for {} has {} values, the grid has {}",
                variable.name,
                values.len(),
                grid.nrows * grid.ncols
            )
            .into());
        }
        debug!("[NC] Writing variable {} for {time}", variable.name);
        // Compression needs only the file's definition, so it runs under a
        // shared lock on the calling thread; only the append is exclusive.
        let record = file
            .read()
            .expect("netcdf output file lock is poisoned")
            .encode(values)?;
        file.write()
            .expect("netcdf output file lock is poisoned")
            .append(record, time)?;
        debug!("[NC] Done writing variable {} for {time}", variable.name);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RISICOError> {
        for (name, file) in &mut self.files {
            file.get_mut()
                .expect("netcdf output file lock is poisoned")
                .finish()
                .map_err(|error| format!("cannot finish NetCDF file {name}.nc: {error}"))?;
        }
        Ok(())
    }
}

impl Writer for ZBinWriter {
    fn prepare(
        &mut self,
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let _ = (grid, variables);
        fs::create_dir_all(&self.path).map_err(|error| {
            format!(
                "cannot create output directory {}: {error}",
                self.path.display()
            )
        })?;
        Ok(())
    }

    fn write_variable(
        &self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let path = self
            .path
            .as_os_str()
            .to_str()
            .expect("Should be a valid path");
        let date_string = time.format("%Y%m%d%H%M").to_string();
        //todo!("get run date from config");
        let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
        let file = format!(
            "{}/{}_{}_{}_{}.zbin",
            path, self.name, run_date, date_string, variable.name
        );

        debug!("[ZBIN] Writing variable {} to {:?}", variable.name, file);
        write_to_zbin_file(&file, grid, values)
            .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
        debug!(
            "[ZBIN] Done writing variable {} to {:?}",
            variable.name, file
        );
        Ok(())
    }
}

impl Writer for PngWriter {
    fn prepare(
        &mut self,
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let _ = (grid, variables);
        fs::create_dir_all(&self.path).map_err(|error| {
            format!(
                "cannot create output directory {}: {error}",
                self.path.display()
            )
        })?;
        Ok(())
    }

    fn write_variable(
        &self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let path = self
            .path
            .as_os_str()
            .to_str()
            .expect("Should be a valid path");
        let date_string = time.format("%Y%m%d%H%M").to_string();
        //todo!("get run date from config");
        let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
        let file = format!(
            "{}/{}_{}_{}_{}.png",
            path, self.name, run_date, date_string, variable.name
        );

        debug!("[PNG] Writing variable {} to {:?}", variable.name, file);
        let palette = self
            .palettes
            .get(&variable.name)
            .ok_or(format!("No palette found for variable {}", variable.name))?;
        write_to_pngwjson(&file, grid, values, palette)
            .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
        debug!(
            "[PNG] Done writing variable {} to {:?}",
            variable.name, file
        );
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
    fn prepare(
        &mut self,
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError> {
        let _ = (grid, variables);
        fs::create_dir_all(&self.path).map_err(|error| {
            format!(
                "cannot create output directory {}: {error}",
                self.path.display()
            )
        })?;
        Ok(())
    }

    fn write_variable(
        &self,
        variable: &OutputVariable,
        values: &[f32],
        grid: &RegularGrid,
        time: DateTime<Utc>,
    ) -> Result<(), RISICOError> {
        let path = self
            .path
            .as_os_str()
            .to_str()
            .expect("Should be a valid path");
        let date_string = time.format("%Y%m%d%H%M").to_string();
        //todo!("get run date from config");
        let run_date = &self.run_date.format("%Y%m%d%H%M").to_string();
        let file = format!(
            "{}/{}_{}_{}_{}.tif",
            path, self.name, run_date, date_string, variable.name
        );

        debug!("[GEOTIFF] Writing variable {} to {:?}", variable.name, file);
        write_to_geotiff(&file, grid, values)
            .map_err(|err| format!("Cannot write file {}: error {err}", file))?;
        debug!(
            "[GEOTIFF] Done writing variable {} to {:?}",
            variable.name, file
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Two chunks of a four cell domain, so every clustering result below has
    /// to survive a merge across chunks rather than one accumulation pass.
    struct FakeSource {
        chunks: Vec<(Vec<usize>, Vec<f32>)>,
        variable: OutputVariableName,
    }

    impl NativeOutputSource for FakeSource {
        fn time(&self) -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2026, 7, 25, 0, 0, 0).unwrap()
        }

        fn has_variable(&self, variable: OutputVariableName) -> bool {
            variable == self.variable
        }

        fn chunk_count(&self) -> usize {
            self.chunks.len()
        }

        fn read_chunk<'a>(
            &'a self,
            variable: OutputVariableName,
            chunk: usize,
            destination: &mut Vec<f32>,
        ) -> Result<ChunkPositions<'a>, RISICOError> {
            assert_eq!(variable, self.variable);
            let (positions, values) = &self.chunks[chunk];
            destination.clear();
            destination.extend_from_slice(values);
            Ok(ChunkPositions::Indexed(positions))
        }
    }

    /// Two cells share the first pixel from different chunks, one lands on the
    /// last pixel, and the fourth sits outside the grid entirely.
    fn fixture(first: f32, second: f32) -> (FakeSource, Vec<f32>, Vec<f32>, RegularGrid) {
        let source = FakeSource {
            variable: OutputVariableName::dffm,
            chunks: vec![
                (vec![0, 2], vec![first, 5.0]),
                (vec![1, 3], vec![second, 7.0]),
            ],
        };
        let lats = vec![0.0, 0.0, 1.0, 90.0];
        let lons = vec![0.0, 0.0, 1.0, 90.0];
        (
            source,
            lats,
            lons,
            RegularGrid::new(2, 2, 0.0, 0.0, 1.0, 1.0),
        )
    }

    fn resample(cluster_mode: ClusterMode, precision: i32, first: f32, second: f32) -> Vec<f32> {
        let (source, lats, lons, grid) = fixture(first, second);
        let variable =
            OutputVariable::new(OutputVariableName::dffm, "UMB", cluster_mode, precision);
        variable
            .get_variable_on_grid(&source, &lats, &lons, &grid, 2)
            .unwrap()
            .unwrap()
            .to_vec()
    }

    #[test]
    fn clustering_combines_cells_that_share_a_pixel_across_chunks() {
        assert_eq!(
            resample(ClusterMode::Max, 0, 10.0, 20.0),
            vec![20.0, NODATAVAL, NODATAVAL, 5.0]
        );
        assert_eq!(
            resample(ClusterMode::Min, 0, 10.0, 20.0),
            vec![10.0, NODATAVAL, NODATAVAL, 5.0]
        );
        assert_eq!(
            resample(ClusterMode::Mean, 0, 10.0, 20.0),
            vec![15.0, NODATAVAL, NODATAVAL, 5.0]
        );
    }

    #[test]
    fn a_nodata_cell_contributes_to_neither_the_value_nor_the_mean() {
        // The surviving cell must read as itself, not as half of itself.
        assert_eq!(
            resample(ClusterMode::Mean, 0, 10.0, NODATAVAL),
            vec![10.0, NODATAVAL, NODATAVAL, 5.0]
        );
        assert_eq!(
            resample(ClusterMode::Max, 0, NODATAVAL, 20.0),
            vec![20.0, NODATAVAL, NODATAVAL, 5.0]
        );
    }

    #[test]
    fn values_are_rounded_to_the_configured_precision() {
        assert_eq!(resample(ClusterMode::Max, -1, 10.24, 10.26)[0], 10.3);
        assert_eq!(resample(ClusterMode::Max, 0, 10.24, 10.26)[0], 10.0);
        assert_eq!(resample(ClusterMode::Max, 1, 14.0, 16.0)[0], 20.0);
    }

    #[test]
    fn splitting_the_chunks_over_more_slots_changes_nothing() {
        let (source, lats, lons, grid) = fixture(10.0, 20.0);
        let variable = OutputVariable::new(OutputVariableName::dffm, "UMB", ClusterMode::Mean, -2);
        let serial = variable
            .get_variable_on_grid(&source, &lats, &lons, &grid, 1)
            .unwrap();
        let split = variable
            .get_variable_on_grid(&source, &lats, &lons, &grid, 8)
            .unwrap();
        assert_eq!(serial, split);
    }

    #[test]
    fn a_variable_the_source_does_not_carry_is_not_written() {
        let (source, lats, lons, grid) = fixture(10.0, 20.0);
        let variable = OutputVariable::new(OutputVariableName::ffmc, "FFMC", ClusterMode::Max, 0);
        assert!(variable
            .get_variable_on_grid(&source, &lats, &lons, &grid, 2)
            .unwrap()
            .is_none());
    }
}
