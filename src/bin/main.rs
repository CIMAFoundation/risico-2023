mod common;

use std::{
    collections::BTreeMap,
    env::{set_var, var},
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use chrono::prelude::*;
use clap::Parser;
use log::{info, warn};
use rischio_runtime_imports::*;
use risico::version::LONG_VERSION;

mod rischio_runtime_imports {
    pub use super::common::{
        config::{
            builder::{ConfigBuilderType, ConfigContainer, StreamingExecutionConfig},
            models::{TileModelRuntime, TiledModelConfig},
        },
        helpers::{get_input, RISICOError},
        io::{
            readers::{
                binary::BinaryInputHandler,
                netcdf::{NetCdfInputConfiguration, NetCdfInputHandler},
                prelude::InputHandler,
            },
            streaming::MappedNativeOutputs,
        },
    };
}

#[derive(Parser, Debug)]
#[command(
    author="Mirko D'Andrea <mirko.dandrea@cimafoundation.org>, Nicolò Perello <nicolo.perello@cimafoundation.org>",
    version,
    long_version=LONG_VERSION,
    about="risico-2023 Wildfire Risk Assessment Model by CIMA Research Foundation",
)]
struct Args {
    #[arg(
        required = true,
        help = "Model date in the format YYYYMMDDHHMM",
        index = 1
    )]
    date: String,

    #[arg(required = true, help = "Path to the configuration file", index = 2)]
    config_path: String,

    #[arg(required = true, help = "Path to the input data file", index = 3)]
    input_path: String,
}

static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn create_run_scratch(
    execution: &StreamingExecutionConfig,
    model_name: &str,
    run_date: DateTime<Utc>,
) -> Result<PathBuf, RISICOError> {
    let base = execution
        .scratch_directory
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("risico-streaming"));
    fs::create_dir_all(&base).map_err(|error| {
        format!(
            "cannot create streaming scratch directory {}: {error}",
            base.display()
        )
    })?;

    let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let run = base.join(format!(
        "{}-{}-{}-{}",
        model_name.to_ascii_lowercase(),
        run_date.format("%Y%m%dT%H%M%S"),
        std::process::id(),
        sequence
    ));
    fs::create_dir(&run).map_err(|error| {
        format!(
            "cannot create model scratch directory {}: {error}",
            run.display()
        )
    })?;
    Ok(run)
}

fn run_tiled_model<C>(
    model_name: &str,
    config: &C,
    handler: &mut dyn InputHandler,
    execution: &StreamingExecutionConfig,
) -> Result<(), RISICOError>
where
    C: TileModelRuntime,
{
    let plan = config.tile_plan(execution)?;
    let cell_count = plan.cell_count();
    let tiles = plan.tiles();
    let timeline = handler.get_timeline();
    let native_variables = config.native_output_variables();
    let (lats, lons) = config.coordinates();
    if lats.len() != cell_count || lons.len() != cell_count {
        return Err(format!(
            "{model_name} has {cell_count} planned cells but {} latitudes and {} longitudes",
            lats.len(),
            lons.len()
        )
        .into());
    }

    info!(
        "{model_name}: processing {cell_count} cells in {} spatial tiles",
        tiles.len()
    );

    let run_date = timeline.first().copied().unwrap_or_else(Utc::now);
    let scratch = create_run_scratch(execution, model_name, run_date)?;
    let mut outputs: BTreeMap<DateTime<Utc>, MappedNativeOutputs> = BTreeMap::new();
    let mut warm_states: BTreeMap<DateTime<Utc>, Vec<Option<C::WarmState>>> = BTreeMap::new();

    for tile in tiles {
        let properties = config.tile_properties(tile);
        let mut state = config.tile_state(tile);
        let tile_lats: Vec<f32> = tile
            .model_positions
            .iter()
            .map(|&position| lats[position])
            .collect();
        let tile_lons: Vec<f32> = tile
            .model_positions
            .iter()
            .map(|&position| lons[position])
            .collect();
        handler
            .set_coordinates(&tile_lats, &tile_lons)
            .map_err(|error| format!("cannot map input coordinates for tile: {error}"))?;

        info!(
            "{model_name}: tile {}/{} ({} active cells)",
            tile.ordinal + 1,
            tiles.len(),
            tile.len()
        );

        for time in &timeline {
            let input = get_input(handler, time, tile.len());
            let step = config.step(&mut state, &properties, &input);

            if let Some(output) = step.output {
                if native_variables.is_empty() {
                    return Err(format!(
                        "{model_name} produced output but no output variables are configured"
                    )
                    .into());
                }
                if !outputs.contains_key(&output.time) {
                    let path = scratch.join(format!(
                        "output-{}.native-f32",
                        output.time.format("%Y%m%dT%H%M%SZ")
                    ));
                    outputs.insert(
                        output.time,
                        MappedNativeOutputs::create(path, native_variables.clone(), cell_count)?,
                    );
                }
                outputs
                    .get_mut(&output.time)
                    .expect("output scratch was just registered")
                    .write_tile(&tile.model_positions, &output)?;
            }

            if step.write_warm_state {
                let records = config.tile_warm_state(&state);
                if records.len() != tile.len() {
                    return Err(format!(
                        "{model_name} tile warm state has {} cells, expected {}",
                        records.len(),
                        tile.len()
                    )
                    .into());
                }
                let destination = warm_states
                    .entry(*time)
                    .or_insert_with(|| vec![None; cell_count]);
                for (&position, record) in tile.model_positions.iter().zip(records) {
                    destination[position] = Some(record);
                }
            }
        }
    }

    let mut output_writer = config.output_writer()?;
    for (time, output) in outputs {
        output.sync_all()?;
        info!(
            "{model_name}: postprocessing output {}",
            time.format("%Y-%m-%d %H:%M")
        );
        output_writer.write_mapped_output(&lats, &lons, time, &output)?;
        let path = output.path().to_path_buf();
        drop(output);
        fs::remove_file(&path).map_err(|error| {
            format!(
                "cannot remove completed output scratch {}: {error}",
                path.display()
            )
        })?;
    }

    for (time, records) in warm_states {
        let missing = records.iter().filter(|record| record.is_none()).count();
        if missing != 0 {
            return Err(
                format!("{model_name} warm state at {time} is missing {missing} cells").into(),
            );
        }
        let records: Vec<C::WarmState> = records
            .into_iter()
            .map(|record| record.expect("missing records were checked"))
            .collect();
        info!(
            "{model_name}: writing warm state {}",
            time.format("%Y-%m-%d %H:%M")
        );
        config.write_warm_state_records(&records, time)?;
    }

    fs::remove_dir(&scratch).map_err(|error| {
        format!(
            "cannot remove completed scratch directory {}: {error}",
            scratch.display()
        )
    })?;
    Ok(())
}

fn get_input_handler(
    input_path_str: &str,
    configs: &ConfigContainer,
) -> Result<Box<dyn InputHandler>, Box<dyn Error>> {
    let input_path = Path::new(input_path_str);
    if input_path.is_file() {
        info!(
            "Loading input data from {} using BinaryInputHandler",
            input_path_str
        );
        return Ok(Box::new(
            BinaryInputHandler::new(input_path_str).map_err(|_| "Could not load input data")?,
        ));
    }
    if input_path.is_dir() {
        info!(
            "Loading input data from {} using NetCdfInputHandler",
            input_path_str
        );
        let nc_config = configs
            .get_netcdf_input_config()
            .clone()
            .unwrap_or_else(NetCdfInputConfiguration::default);
        return Ok(Box::new(
            NetCdfInputHandler::new(input_path_str, &nc_config)
                .map_err(|_| "Could not load input data")?,
        ));
    }
    Err(format!("Input path {} is not valid", input_path_str).into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    if var("RUST_LOG").is_err() {
        set_var("RUST_LOG", "info")
    }
    pretty_env_logger::init();

    if !Path::new(&args.config_path).is_file() {
        return Err(format!("Config file {} is not a file", args.config_path).into());
    }
    let date = NaiveDateTime::parse_from_str(&args.date, "%Y%m%d%H%M")
        .map_err(|_| format!("Could not parse run date '{}'", args.date))?;
    let date = DateTime::from_naive_utc_and_offset(date, Utc);
    let configs = ConfigContainer::from_file(&args.config_path)
        .map_err(|error| format!("Failed to load config: {error}"))?;
    let mut input_handler = get_input_handler(&args.input_path, &configs)?;
    info!("Input files:\n{}", input_handler.info_input());

    for model_config in &configs.models {
        let model_name = model_config.get_model_name();
        info!("Running model: {model_name}");
        let start = Utc::now();
        let result = match model_config {
            ConfigBuilderType::RISICO(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::FWI(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Mark5(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::KBDI(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Angstrom(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Fosberg(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Nesterov(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Sharples(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Orieux(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
            ConfigBuilderType::Hdw(builder) => {
                builder.build(&date, &configs.palettes).and_then(|config| {
                    run_tiled_model(
                        model_name,
                        &config,
                        input_handler.as_mut(),
                        &configs.streaming,
                    )
                })
            }
        };
        if let Err(error) = result {
            warn!("Error running model {model_name}: {error}");
        }
        info!(
            "{model_name} elapsed time: {} seconds",
            (Utc::now() - start).num_seconds()
        );
    }

    Ok(())
}
