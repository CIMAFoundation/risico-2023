#[allow(dead_code)]
mod common;

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use common::helpers::RISICOError;
use common::io::models::grid::RegularGrid;
use common::io::static_data::geotiff::{RasterGrid, STATIC_NODATA};
use common::io::writers::write_north_up_geotiff;
use rischio_cli::ResultExt;

mod rischio_cli {
    use crate::common::helpers::RISICOError;

    pub trait ResultExt<T> {
        fn cli(self) -> T;
    }

    impl<T> ResultExt<T> for Result<T, RISICOError> {
        fn cli(self) -> T {
            self.unwrap_or_else(|error| {
                eprintln!("error: {error}");
                std::process::exit(2);
            })
        }
    }
}

#[derive(Parser, Debug)]
#[command(about = "Convert legacy regular-grid static data to canonical GeoTIFF layers")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Convert RISICO's lon/lat/slope/aspect/vegetation cell file.
    Risico {
        #[arg(long)]
        cells: PathBuf,
        #[arg(long)]
        grid: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Optional legacy two-column summer/winter PPF file.
        #[arg(long)]
        ppf: Option<PathBuf>,
    },
    /// Convert FWI's lon/lat cell file to a domain mask.
    Fwi {
        #[arg(long)]
        cells: PathBuf,
        #[arg(long)]
        grid: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() {
    match Args::parse().command {
        Command::Risico {
            cells,
            grid,
            output,
            ppf,
        } => convert_risico(&cells, &grid, &output, ppf.as_deref()).cli(),
        Command::Fwi {
            cells,
            grid,
            output,
        } => convert_fwi(&cells, &grid, &output).cli(),
    }
}

fn convert_risico(
    cells_path: &Path,
    grid_path: &Path,
    output: &Path,
    ppf_path: Option<&Path>,
) -> Result<(), RISICOError> {
    let rows = read_numeric_rows::<5>(cells_path)?;
    let regular = RegularGrid::from_txt_file(path_string(grid_path)?)?;
    let grid = conversion_grid(&regular, &rows)?;
    let ppf = ppf_path.map(read_numeric_rows::<2>).transpose()?;
    if ppf
        .as_ref()
        .is_some_and(|values| values.len() != rows.len())
    {
        return Err(format!(
            "PPF row count does not match static cells ({} versus {})",
            ppf.as_ref().map_or(0, Vec::len),
            rows.len()
        )
        .into());
    }

    let size = grid.width * grid.height;
    let mut domain = vec![0.0; size];
    let mut slope = vec![STATIC_NODATA; size];
    let mut aspect = vec![STATIC_NODATA; size];
    let mut vegetation = vec![STATIC_NODATA; size];
    let mut ppf_summer = vec![STATIC_NODATA; size];
    let mut ppf_winter = vec![STATIC_NODATA; size];

    let mut previous = None;
    for (row_number, row) in rows.iter().enumerate() {
        let index = canonical_index(&grid, row[1], row[0], row_number, previous)?;
        previous = Some(index);
        domain[index] = 1.0;
        slope[index] = row[2] as f32;
        aspect[index] = row[3] as f32;
        vegetation[index] = row[4] as f32;
        let (summer, winter) = ppf
            .as_ref()
            .map(|values| (values[row_number][0] as f32, values[row_number][1] as f32))
            .unwrap_or((1.0, 1.0));
        ppf_summer[index] = summer;
        ppf_winter[index] = winter;
    }

    fs::create_dir_all(output).map_err(|error| {
        format!(
            "cannot create output directory {}: {error}",
            output.display()
        )
    })?;
    write_layer(&output.join("domain_mask.tif"), &grid, &domain)?;
    write_layer(&output.join("slope.tif"), &grid, &slope)?;
    write_layer(&output.join("aspect.tif"), &grid, &aspect)?;
    write_layer(&output.join("vegetation_id.tif"), &grid, &vegetation)?;
    write_layer(&output.join("ppf_summer.tif"), &grid, &ppf_summer)?;
    write_layer(&output.join("ppf_winter.tif"), &grid, &ppf_winter)?;
    println!(
        "converted {} active RISICO cells into {}",
        rows.len(),
        output.display()
    );
    Ok(())
}

fn convert_fwi(cells_path: &Path, grid_path: &Path, output: &Path) -> Result<(), RISICOError> {
    let rows = read_numeric_rows::<2>(cells_path)?;
    let regular = RegularGrid::from_txt_file(path_string(grid_path)?)?;
    let grid = conversion_grid(&regular, &rows)?;
    let mut domain = vec![0.0; grid.width * grid.height];
    let mut previous = None;
    for (row_number, row) in rows.iter().enumerate() {
        let index = canonical_index(&grid, row[1], row[0], row_number, previous)?;
        previous = Some(index);
        domain[index] = 1.0;
    }

    fs::create_dir_all(output).map_err(|error| {
        format!(
            "cannot create output directory {}: {error}",
            output.display()
        )
    })?;
    write_layer(&output.join("domain_mask.tif"), &grid, &domain)?;
    println!(
        "converted {} active FWI cells into {}",
        rows.len(),
        output.display()
    );
    Ok(())
}

fn write_layer(path: &Path, grid: &RasterGrid, values: &[f32]) -> Result<(), RISICOError> {
    let path = path
        .to_str()
        .ok_or_else(|| format!("GeoTIFF path is not valid UTF-8: {}", path.display()))?;
    write_north_up_geotiff(
        path,
        grid.width,
        grid.height,
        &grid.transform,
        grid.epsg,
        values,
        STATIC_NODATA,
    )
}

fn conversion_grid<const N: usize>(
    grid: &RegularGrid,
    rows: &[[f64; N]],
) -> Result<RasterGrid, RISICOError> {
    let edge_step_lon = (grid.max_lon - grid.min_lon) as f64 / grid.ncols as f64;
    let edge_step_lat = (grid.max_lat - grid.min_lat) as f64 / grid.nrows as f64;
    if edge_step_lon <= 0.0 || edge_step_lat <= 0.0 {
        return Err("legacy grid must have positive dimensions and extents".into());
    }

    // Legacy grids use bounds inconsistently: most axes store outer edges, while
    // some deployed FWI grids store a centre on one axis. Pick the phase that best
    // fits the actual cell lattice, independently for X and Y.
    let center_step_lon =
        (grid.ncols > 1).then(|| (grid.max_lon - grid.min_lon) as f64 / (grid.ncols - 1) as f64);
    let center_step_lat =
        (grid.nrows > 1).then(|| (grid.max_lat - grid.min_lat) as f64 / (grid.nrows - 1) as f64);
    let inferred_lat = infer_axis_from_ordered_rows(rows, 1, -1.0);
    let ordered_lat = infer_axis_from_level_transitions(rows, 1, -1.0);
    let inferred_lon = infer_axis_from_ordered_rows(rows, 0, 1.0);
    let (center_lat, step_lat, lat_from_cells) = choose_axis(
        rows.iter().map(|row| row[1]),
        grid.max_lat as f64,
        edge_step_lat,
        center_step_lat,
        -1.0,
        inferred_lat,
        ordered_lat,
    );
    let square_cell_lon = lat_from_cells.then(|| {
        (
            rows.iter()
                .map(|row| row[0])
                .min_by(f64::total_cmp)
                .expect("static rows are non-empty"),
            step_lat,
        )
    });
    let (center_lon, step_lon, lon_from_cells) = choose_axis(
        rows.iter().map(|row| row[0]),
        grid.min_lon as f64,
        edge_step_lon,
        center_step_lon,
        1.0,
        inferred_lon,
        square_cell_lon,
    );
    let max_col = rows
        .iter()
        .map(|row| ((row[0] - center_lon) / step_lon).round() as isize)
        .max()
        .unwrap_or(0);
    let max_row = rows
        .iter()
        .map(|row| ((center_lat - row[1]) / step_lat).round() as isize)
        .max()
        .unwrap_or(0);
    if max_col < 0 || max_row < 0 {
        return Err("legacy cells fall before the inferred grid origin".into());
    }
    // An output aggregation grid is often the only legacy grid referenced by a
    // deployment and can be coarser than the static-cell lattice. In that case
    // use the extent inferred from the cells instead of padding to unrelated
    // output dimensions.
    let width = if lon_from_cells {
        max_col as usize + 1
    } else {
        grid.ncols.max(max_col as usize + 1)
    };
    let height = if lat_from_cells {
        max_row as usize + 1
    } else {
        grid.nrows.max(max_row as usize + 1)
    };
    Ok(RasterGrid {
        width,
        height,
        epsg: 4326,
        transform: [
            center_lon - step_lon / 2.0,
            step_lon,
            0.0,
            center_lat + step_lat / 2.0,
            0.0,
            -step_lat,
        ],
    })
}

fn choose_axis(
    values: impl Iterator<Item = f64> + Clone,
    bound_center: f64,
    edge_step: f64,
    center_step: Option<f64>,
    direction: f64,
    inferred: Option<(f64, f64)>,
    alternate_inferred: Option<(f64, f64)>,
) -> (f64, f64, bool) {
    let score = |origin: f64, step: f64| {
        values
            .clone()
            .take(100_000)
            .map(|value| {
                let offset = (value - origin) / step;
                (offset - offset.round()).abs()
            })
            .sum::<f64>()
    };
    let edge_center = bound_center + direction * edge_step / 2.0;
    let mut candidates = vec![
        (edge_center, edge_step, false, score(edge_center, edge_step)),
        (
            bound_center,
            edge_step,
            false,
            score(bound_center, edge_step),
        ),
    ];
    if let Some(step) = center_step {
        candidates.push((bound_center, step, false, score(bound_center, step)));
    }
    if let Some((origin, step)) = inferred {
        candidates.push((origin, step, true, score(origin, step)));
    }
    if let Some((origin, step)) = alternate_inferred {
        candidates.push((origin, step, true, score(origin, step)));
    }
    candidates
        .into_iter()
        .min_by(|left, right| left.3.total_cmp(&right.3))
        .map(|(origin, step, from_cells, _)| (origin, step, from_cells))
        .expect("axis always has at least one candidate")
}

fn infer_axis_from_ordered_rows<const N: usize>(
    rows: &[[f64; N]],
    column: usize,
    direction: f64,
) -> Option<(f64, f64)> {
    let origin = if direction > 0.0 {
        rows.iter().map(|row| row[column]).min_by(f64::total_cmp)?
    } else {
        rows.iter().map(|row| row[column]).max_by(f64::total_cmp)?
    };
    let smallest_delta = rows
        .windows(2)
        .filter_map(|pair| {
            let delta = (pair[1][column] - pair[0][column]) * direction;
            (delta > f64::EPSILON).then_some(delta)
        })
        .min_by(f64::total_cmp)?;
    let opposite = if direction > 0.0 {
        rows.iter().map(|row| row[column]).max_by(f64::total_cmp)?
    } else {
        rows.iter().map(|row| row[column]).min_by(f64::total_cmp)?
    };
    let span = (opposite - origin).abs();
    let intervals = (span / smallest_delta).round().max(1.0);
    Some((origin, span / intervals))
}

fn infer_axis_from_level_transitions<const N: usize>(
    rows: &[[f64; N]],
    column: usize,
    direction: f64,
) -> Option<(f64, f64)> {
    let first = rows.first()?[column];
    let last = rows.last()?[column];
    let transitions = rows
        .windows(2)
        .filter(|pair| (pair[1][column] - pair[0][column]) * direction > f64::EPSILON)
        .count();
    (transitions > 0).then(|| (first, (last - first).abs() / transitions as f64))
}

fn canonical_index(
    grid: &RasterGrid,
    lat: f64,
    lon: f64,
    row_number: usize,
    previous: Option<usize>,
) -> Result<usize, RISICOError> {
    let col_position = (lon - (grid.transform[0] + grid.transform[1] / 2.0)) / grid.transform[1];
    let row_position = ((grid.transform[3] + grid.transform[5] / 2.0) - lat) / -grid.transform[5];
    let col = col_position.round() as isize;
    let row = row_position.round() as isize;
    if col < 0
        || row < 0
        || col as usize >= grid.width
        || row as usize >= grid.height
        || (col_position - col as f64).abs() > 0.1
        || (row_position - row as f64).abs() > 0.1
    {
        return Err(format!(
            "cell row {} ({lon}, {lat}) is not aligned with the inferred regular grid",
            row_number + 1
        )
        .into());
    }
    let index = row as usize * grid.width + col as usize;
    if previous.is_some_and(|value| value >= index) {
        return Err(format!(
            "legacy cells must be unique and ordered north-to-south, west-to-east; row {} is out of order",
            row_number + 1
        )
        .into());
    }
    Ok(index)
}

fn read_numeric_rows<const N: usize>(path: &Path) -> Result<Vec<[f64; N]>, RISICOError> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open legacy static file {}: {error}", path.display()))?;
    let mut rows = Vec::new();
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| {
            format!(
                "cannot read {} at line {}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('%') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let mut values = [0.0; N];
        for (column, value) in values.iter_mut().enumerate() {
            let raw = parts.next().ok_or_else(|| {
                RISICOError::from(format!(
                    "{} line {} has {column} columns, expected at least {N}",
                    path.display(),
                    line_index + 1
                ))
            })?;
            *value = raw.parse::<f64>().map_err(|error| {
                RISICOError::from(format!(
                    "invalid number in {} at line {}: {error}",
                    path.display(),
                    line_index + 1
                ))
            })?;
        }
        rows.push(values);
    }
    if rows.is_empty() {
        return Err(format!("legacy static file {} has no data rows", path.display()).into());
    }
    Ok(rows)
}

fn path_string(path: &Path) -> Result<&str, RISICOError> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::io::static_data::geotiff::RasterDomain;

    #[test]
    fn converts_north_to_south_cells_without_changing_active_order() {
        let directory = std::env::temp_dir().join(format!(
            "risico-static-converter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("valid system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("test directory should be created");
        let grid = directory.join("grid.txt");
        let cells = directory.join("cells.txt");
        let output = directory.join("output");
        fs::write(
            &grid,
            "GRIDNROWS=2\nGRIDNCOLS=3\nMINLAT=39.5\nMAXLAT=41.5\nMINLON=9.5\nMAXLON=12.5\n",
        )
        .expect("grid should be written");
        fs::write(
            &cells,
            "# lon lat slope aspect vegetation\n10 41 5 90 1\n12 41 6 180 2\n11 40 7 270 3\n",
        )
        .expect("cells should be written");

        convert_risico(&cells, &grid, &output, None).expect("conversion should succeed");
        let domain = RasterDomain::open(output.join("domain_mask.tif"))
            .expect("converted domain should be readable");
        assert_eq!(domain.cell_indexes, vec![0, 2, 4]);
        assert_eq!(
            domain.coordinates(),
            (vec![41.0, 41.0, 40.0], vec![10.0, 12.0, 11.0])
        );
        assert_eq!(
            domain
                .read_required_layer(output.join("slope.tif"), "slope")
                .expect("slope should be readable"),
            vec![5.0, 6.0, 7.0]
        );
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[test]
    fn infers_static_lattice_when_output_grid_is_coarser() {
        let output_grid = RegularGrid::new(2, 3, 39.5, 9.5, 41.5, 12.5);
        let rows = [
            [10.0, 41.0, 0.0, 0.0, 1.0],
            [10.1, 41.0, 0.0, 0.0, 1.0],
            [10.0, 40.9, 0.0, 0.0, 1.0],
        ];
        let grid = conversion_grid(&output_grid, &rows).expect("grid should be inferred");
        assert_eq!((grid.width, grid.height), (2, 2));
        assert!((grid.transform[1] - 0.1).abs() < 1.0e-6);
        assert!((grid.transform[5] + 0.1).abs() < 1.0e-6);
    }

    #[test]
    fn infers_lattice_from_coordinates_rounded_to_decimal_places() {
        let output_grid = RegularGrid::new(10, 10, 30.0, 30.0, 40.0, 40.0);
        let step = 0.000951297;
        let rows: Vec<[f64; 5]> = (0..=1_000)
            .flat_map(|row| {
                let lat = ((34.69137 - row as f64 * step) * 100_000.0).round() / 100_000.0;
                let columns: Box<dyn Iterator<Item = usize>> = if row == 0 {
                    Box::new(0..=1_000)
                } else {
                    Box::new([0, 1_000].into_iter())
                };
                columns.map(move |column| {
                    let lon = ((35.10337 + column as f64 * step) * 100_000.0).round() / 100_000.0;
                    [lon, lat, 0.0, 0.0, 1.0]
                })
            })
            .collect();

        let grid = conversion_grid(&output_grid, &rows).expect("grid should be inferred");
        assert_eq!((grid.width, grid.height), (1_001, 1_001));
        let mut previous = None;
        for (index, row) in rows.iter().enumerate() {
            previous = Some(
                canonical_index(&grid, row[1], row[0], index, previous)
                    .expect("rounded coordinate should stay on the inferred lattice"),
            );
        }
    }
}
