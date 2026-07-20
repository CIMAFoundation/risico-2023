#[allow(dead_code)]
mod common;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use clap::{Args as ClapArgs, Parser, Subcommand};
use common::config::builder::{
    ConfigBuilderType, ConfigContainer, StaticDataConfig, WarmStateConfig,
};
use common::helpers::RISICOError;
use common::io::static_data::geotiff::RasterDomain;
use common::io::warm_state::legacy::{read_fwi, read_risico};
use common::io::warm_state::netcdf::{
    load_latest_fwi, load_latest_risico, write_fwi_snapshot, write_risico_snapshot,
};
use risico::modules::fwi::{config::FWIModelConfig, models::FWIState};
use risico::modules::risico::{config::RISICOModelConfig, models::RISICOState};

#[derive(Parser, Debug)]
#[command(about = "Convert existing legacy warm-state files to validated NetCDF snapshots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Convert warm states using paths and model metadata from a migrated YAML config.
    Config(ConfigArgs),
    /// Convert RISICO warm-state files.
    Risico(ConvertArgs),
    /// Convert FWI warm-state files, including deployed four-column scalar states.
    Fwi(ConvertArgs),
}

#[derive(ClapArgs, Debug)]
struct ConfigArgs {
    /// Migrated YAML configuration containing GeoTIFF and NetCDF warm-state settings.
    #[arg(long)]
    config: PathBuf,
    /// Convert only the newest legacy snapshot.
    #[arg(long)]
    latest_only: bool,
    /// Replace an existing NetCDF snapshot with the same timestamp.
    #[arg(long)]
    overwrite: bool,
}

#[derive(ClapArgs, Debug)]
struct ConvertArgs {
    /// Legacy filename prefix, or a directory when files are named only by timestamp.
    #[arg(long)]
    legacy_prefix: PathBuf,
    /// GeoTIFF domain mask used to validate cell count and define the output grid.
    #[arg(long)]
    domain_mask: PathBuf,
    /// Destination directory for NetCDF snapshots.
    #[arg(long)]
    output: PathBuf,
    /// Model version stored in and validated against each snapshot.
    #[arg(long)]
    model_version: String,
    /// Convert only the newest legacy snapshot.
    #[arg(long)]
    latest_only: bool,
    /// Replace an existing NetCDF snapshot with the same timestamp.
    #[arg(long)]
    overwrite: bool,
}

fn main() -> Result<(), RISICOError> {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Config(args) => convert_config(args),
        Command::Risico(args) => convert_risico(args),
        Command::Fwi(args) => convert_fwi(args),
    };
    result
}

fn convert_config(args: ConfigArgs) -> Result<(), RISICOError> {
    let path = args
        .config
        .to_str()
        .ok_or_else(|| format!("config path is not valid UTF-8: {}", args.config.display()))?;
    let config = ConfigContainer::from_file(path)?;
    for model in config.models {
        match model {
            ConfigBuilderType::RISICO(model) => {
                convert_risico(arguments_from_config(
                    &model.model_name,
                    model.static_data,
                    model.warm_state,
                    model.model_version,
                    args.latest_only,
                    args.overwrite,
                )?)?;
            }
            ConfigBuilderType::FWI(model) => {
                convert_fwi(arguments_from_config(
                    &model.model_name,
                    model.static_data,
                    model.warm_state,
                    model.model_version,
                    args.latest_only,
                    args.overwrite,
                )?)?;
            }
            other => {
                return Err(format!(
                    "warm-state conversion is not implemented for {}",
                    other.get_model_name()
                )
                .into())
            }
        }
    }
    Ok(())
}

fn arguments_from_config(
    model_name: &str,
    static_data: Option<StaticDataConfig>,
    warm_state: Option<WarmStateConfig>,
    model_version: String,
    latest_only: bool,
    overwrite: bool,
) -> Result<ConvertArgs, RISICOError> {
    let domain_mask = match static_data {
        Some(StaticDataConfig::GeoTiff { domain_mask, .. }) => domain_mask.into(),
        None => {
            return Err(format!(
                "model {model_name} requires GeoTIFF static_data before warm-state conversion"
            )
            .into())
        }
    };
    let (output, legacy_prefix) = match warm_state {
        Some(WarmStateConfig::NetCdf {
            directory,
            legacy_fallback: Some(legacy_fallback),
            ..
        }) => (directory.into(), legacy_fallback.into()),
        Some(WarmStateConfig::NetCdf {
            legacy_fallback: None,
            ..
        }) => {
            return Err(
                format!("model {model_name} has no warm_state.legacy_fallback to convert").into(),
            )
        }
        None => {
            return Err(format!(
                "model {model_name} requires NetCDF warm_state configuration before conversion"
            )
            .into())
        }
    };
    Ok(ConvertArgs {
        legacy_prefix,
        domain_mask,
        output,
        model_version,
        latest_only,
        overwrite,
    })
}

fn convert_risico(args: ConvertArgs) -> Result<(), RISICOError> {
    let domain = RasterDomain::open(&args.domain_mask)?;
    let snapshots = selected_snapshots(&args)?;
    convert_all(&snapshots, |time, source| {
        let destination = args
            .output
            .join(format!("RISICO_{}.nc", time.format("%Y%m%dT%H%M%SZ")));
        if destination.exists() && !args.overwrite {
            return Ok(Conversion::Skipped(destination));
        }
        let file = File::open(source)
            .map_err(|error| format!("cannot open {}: {error}", source.display()))?;
        let warm_state = read_risico(BufReader::new(file), &source.display().to_string())?;
        validate_cell_count("RISICO", source, warm_state.len(), &domain)?;
        let state = RISICOState::new(
            &warm_state,
            time,
            RISICOModelConfig::new(&args.model_version),
        );
        let written = write_risico_snapshot(
            &args.output,
            &state,
            &args.model_version,
            &domain.grid,
            &domain.cell_indexes,
        )?;
        let verified = load_latest_risico(
            &args.output,
            *time,
            0,
            &args.model_version,
            &domain.grid,
            &domain.cell_indexes,
        )?
        .is_some_and(|(_, loaded_time)| loaded_time == *time);
        if !verified {
            return Err(format!("written snapshot {} failed validation", written.display()).into());
        }
        Ok(Conversion::Written(written))
    })
}

fn convert_fwi(args: ConvertArgs) -> Result<(), RISICOError> {
    let domain = RasterDomain::open(&args.domain_mask)?;
    let snapshots = selected_snapshots(&args)?;
    convert_all(&snapshots, |time, source| {
        let destination = args
            .output
            .join(format!("FWI_{}.nc", time.format("%Y%m%dT%H%M%SZ")));
        if destination.exists() && !args.overwrite {
            return Ok(Conversion::Skipped(destination));
        }
        let file = File::open(source)
            .map_err(|error| format!("cannot open {}: {error}", source.display()))?;
        let warm_state = read_fwi(BufReader::new(file), &source.display().to_string(), *time)?;
        validate_cell_count("FWI", source, warm_state.len(), &domain)?;
        let state = FWIState::new(&warm_state, time, FWIModelConfig::new(&args.model_version));
        let written = write_fwi_snapshot(
            &args.output,
            &state,
            &args.model_version,
            &domain.grid,
            &domain.cell_indexes,
        )?;
        let verified = load_latest_fwi(
            &args.output,
            *time,
            0,
            &args.model_version,
            &domain.grid,
            &domain.cell_indexes,
        )?
        .is_some_and(|(_, loaded_time)| loaded_time == *time);
        if !verified {
            return Err(format!("written snapshot {} failed validation", written.display()).into());
        }
        Ok(Conversion::Written(written))
    })
}

fn validate_cell_count(
    model: &str,
    source: &Path,
    state_cells: usize,
    domain: &RasterDomain,
) -> Result<(), RISICOError> {
    if state_cells != domain.cell_indexes.len() {
        return Err(format!(
            "{model} warm state {} has {state_cells} cells but domain mask has {} active cells",
            source.display(),
            domain.cell_indexes.len()
        )
        .into());
    }
    Ok(())
}

enum Conversion {
    Written(PathBuf),
    Skipped(PathBuf),
}

fn convert_all(
    snapshots: &[(DateTime<Utc>, PathBuf)],
    mut convert: impl FnMut(&DateTime<Utc>, &Path) -> Result<Conversion, RISICOError>,
) -> Result<(), RISICOError> {
    let mut written = 0;
    let mut skipped = 0;
    let mut failures = Vec::new();
    for (time, source) in snapshots {
        match convert(time, source) {
            Ok(Conversion::Written(path)) => {
                written += 1;
                println!("converted {} -> {}", source.display(), path.display());
            }
            Ok(Conversion::Skipped(path)) => {
                skipped += 1;
                println!("skipped existing {}", path.display());
            }
            Err(error) => failures.push(format!("{}: {error}", source.display())),
        }
    }
    println!(
        "warm-state conversion complete: {written} written, {skipped} skipped, {} failed",
        failures.len()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("warm-state conversion failures:\n{}", failures.join("\n")).into())
    }
}

fn selected_snapshots(args: &ConvertArgs) -> Result<Vec<(DateTime<Utc>, PathBuf)>, RISICOError> {
    let mut snapshots = discover_snapshots(&args.legacy_prefix)?;
    if args.latest_only {
        snapshots = snapshots.into_iter().next_back().into_iter().collect();
    }
    Ok(snapshots)
}

fn discover_snapshots(prefix: &Path) -> Result<Vec<(DateTime<Utc>, PathBuf)>, RISICOError> {
    let (directory, filename_prefix) = if prefix.is_dir() {
        (prefix, "")
    } else {
        (
            prefix.parent().unwrap_or_else(|| Path::new(".")),
            prefix
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("legacy prefix is not valid UTF-8: {}", prefix.display()))?,
        )
    };
    let entries = std::fs::read_dir(directory).map_err(|error| {
        format!(
            "cannot read legacy warm-state directory {}: {error}",
            directory.display()
        )
    })?;
    let mut snapshots = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot read an entry in legacy warm-state directory {}: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(timestamp) = name.strip_prefix(filename_prefix) else {
            continue;
        };
        let Ok(naive) = NaiveDateTime::parse_from_str(timestamp, "%Y%m%d%H%M") else {
            continue;
        };
        snapshots.push((DateTime::from_naive_utc_and_offset(naive, Utc), path));
    }
    snapshots.sort_by_key(|(time, _)| *time);
    if snapshots.is_empty() {
        return Err(format!(
            "no legacy warm-state files matching {} were found",
            prefix.display()
        )
        .into());
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "risico-warm-converter-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("valid system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn discovers_prefixed_snapshots_in_time_order() {
        let directory = temporary_directory("prefixed");
        fs::create_dir_all(&directory).expect("test directory should be created");
        fs::write(directory.join("state0_202607200000"), "state").expect("state should be written");
        fs::write(directory.join("state0_202607190000"), "state").expect("state should be written");
        fs::write(directory.join("unrelated"), "state").expect("file should be written");

        let snapshots =
            discover_snapshots(&directory.join("state0_")).expect("snapshots should be discovered");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(
            snapshots[0].0.format("%Y%m%d%H%M").to_string(),
            "202607190000"
        );
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[test]
    fn discovers_bare_timestamp_files_in_a_directory() {
        let directory = temporary_directory("directory");
        fs::create_dir_all(&directory).expect("test directory should be created");
        fs::write(directory.join("202607200000"), "state").expect("state should be written");

        let snapshots = discover_snapshots(&directory).expect("snapshot should be discovered");
        assert_eq!(snapshots.len(), 1);
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }
}
