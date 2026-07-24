mod common;

use std::{
    env::{set_var, var},
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
};

use chrono::prelude::*;
use clap::Parser;
use log::{info, warn};
use rayon::prelude::*;
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
            streaming::{MappedNativeOutputs, TileNativeOutput, TilePlan, TiledNativeOutputs},
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

/// What one tile produced in a timestep, carried back in plan order.
struct TileOutcome {
    output: Option<(DateTime<Utc>, MappedNativeOutputs)>,
}

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

/// Render a byte count the way the configuration would have written it.
fn format_bytes(bytes: u64) -> String {
    const UNITS: [(&str, u64); 3] = [("GB", 1_000_000_000), ("MB", 1_000_000), ("kB", 1_000)];
    for (unit, scale) in UNITS {
        if bytes >= scale {
            return format!("{:.2} {unit}", bytes as f64 / scale as f64);
        }
    }
    format!("{bytes} B")
}

/// How many tiles a budget pays for once the whole-domain arrays are charged.
///
/// Always at least one: a budget smaller than the domain's fixed cost cannot be
/// honoured by tiling at all, and refusing to run would be less useful than
/// running at the smallest footprint available.
fn tiles_within_budget(budget: u64, resident: u64, per_tile: u64, ceiling: usize) -> usize {
    if per_tile == 0 {
        return ceiling.max(1);
    }
    let affordable = budget.saturating_sub(resident) / per_tile;
    (affordable as usize).clamp(1, ceiling.max(1))
}

/// Decide how many tiles may run at once.
///
/// An explicit `tile_concurrency` is taken as given. Otherwise a `max_memory`
/// budget is turned into a thread count by charging the whole-domain arrays
/// first and dividing what is left by the cost of the largest tile. With
/// neither set the runner keeps rayon's default of one tile per core.
fn resolve_tile_concurrency<C>(
    model_name: &str,
    config: &C,
    execution: &StreamingExecutionConfig,
    plan: &TilePlan,
) -> Result<Option<usize>, RISICOError>
where
    C: TileModelRuntime,
{
    if let Some(threads) = execution.tile_concurrency {
        return Ok(Some(threads));
    }
    let Some(budget) = execution.max_memory_bytes()? else {
        return Ok(None);
    };

    let footprint = config.memory_model();
    let cell_count = plan.cell_count() as u64;
    let resident = cell_count * footprint.resident_per_cell as u64;

    // Concurrency is bounded by the largest tile, not the average one, so the
    // estimate holds for every step of the plan rather than on average.
    let largest_tile = plan
        .tiles()
        .iter()
        .map(|tile| tile.len())
        .max()
        .unwrap_or(0) as u64;
    let per_tile = largest_tile * footprint.in_flight_per_cell as u64;

    let ceiling = plan.tiles().len().min(rayon::current_num_threads());
    let threads = tiles_within_budget(budget, resident, per_tile, ceiling);

    if resident >= budget {
        warn!(
            "{model_name}: max_memory is {} but the domain alone needs about {}; \
             running one tile at a time, which is the least this domain can use",
            format_bytes(budget),
            format_bytes(resident)
        );
    } else {
        info!(
            "{model_name}: max_memory {} allows {threads} concurrent tiles \
             (domain {}, {} per tile)",
            format_bytes(budget),
            format_bytes(resident),
            format_bytes(per_tile)
        );
    }

    Ok(Some(threads))
}

fn run_tiled_model<C>(
    model_name: &str,
    config: &C,
    handler: &mut dyn InputHandler,
    execution: &StreamingExecutionConfig,
) -> Result<(), RISICOError>
where
    C: TileModelRuntime + Sync,
    C::WarmState: Send,
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
    let state_paths: Vec<PathBuf> = tiles
        .iter()
        .map(|tile| scratch.join(format!("state-tile-{}.bin", tile.ordinal)))
        .collect();
    let mut output_writer = config.output_writer()?;

    // Tiles run concurrently, so memory in flight scales with how many are
    // allowed at once. An explicit pool keeps that bounded and independent of
    // any other rayon use in the process.
    let concurrency = resolve_tile_concurrency(model_name, config, execution, &plan)?;
    let tile_pool = match concurrency {
        None => None,
        Some(threads) => Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(|error| format!("cannot build the tile thread pool: {error}"))?,
        ),
    };

    // Mapping a tile onto the input grid does not depend on time, so register
    // every tile once and only re-select it as the timeline advances.
    handler.clear_registered_coordinates();
    let mut tile_selections = Vec::with_capacity(tiles.len());
    for tile in tiles {
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
        let selection = handler
            .register_coordinates(&tile_lats, &tile_lons)
            .map_err(|error| format!("cannot map input coordinates for tile: {error}"))?;
        tile_selections.push(selection);
    }

    for time in &timeline {
        info!("{model_name}: processing {}", time.format("%Y-%m-%d %H:%M"));
        // Decode this timestamp once so concurrent tiles only read the cache.
        handler.preload(time);

        // The whole domain's warm state is filled in place. Tiles own disjoint
        // positions, so the lock only orders the scatters and is held for a
        // small fraction of a tile's work.
        let warm_state_sink: Mutex<Option<Vec<C::WarmState>>> = Mutex::new(None);
        let warm_state_cells = AtomicUsize::new(0);
        let handler_ref: &dyn InputHandler = handler;

        // Tiles are independent: disjoint cells, disjoint state checkpoints and
        // one output scratch each. `collect` restores plan order afterwards.
        let run_tiles = || -> Result<Vec<TileOutcome>, RISICOError> {
            tiles
            .par_iter()
            .enumerate()
            .map(|(tile_index, tile)| -> Result<TileOutcome, RISICOError> {
                let selection = tile_selections[tile_index];
                let properties = config.tile_properties(tile);
                let mut state = config.tile_state(tile);
                let state_path = &state_paths[tile_index];
                if state_path.exists() {
                    config.restore_tile_state(&mut state, state_path)?;
                }
                let input = get_input(handler_ref, selection, time, tile.len());
                let step = config.step(&mut state, &properties, &input);

                let output = match step.output {
                    None => None,
                    Some(output) => {
                        if native_variables.is_empty() {
                            return Err(format!(
                                "{model_name} produced output but no output variables are configured"
                            )
                            .into());
                        }
                        let path = scratch.join(format!(
                            "output-{}-tile-{}.native-f32",
                            output.time.format("%Y%m%dT%H%M%SZ"),
                            tile.ordinal
                        ));
                        let mut mapped =
                            MappedNativeOutputs::create(path, native_variables.clone(), tile.len())?;
                        mapped.write_output(&output)?;
                        // Tile scratches are held until the timestep is joined,
                        // so keeping them resident would cost the whole domain's
                        // output no matter how few tiles run at once.
                        mapped.release_pages()?;
                        Some((output.time, mapped))
                    }
                };

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
                    let mut sink = warm_state_sink
                        .lock()
                        .expect("warm state sink lock is poisoned");
                    let destination = sink
                        .get_or_insert_with(|| vec![C::WarmState::default(); cell_count]);
                    for (&position, record) in tile.model_positions.iter().zip(records) {
                        destination[position] = record;
                    }
                    warm_state_cells.fetch_add(tile.len(), Ordering::Relaxed);
                }

                config.checkpoint_tile_state(&state, state_path)?;
                Ok(TileOutcome { output })
            })
            .collect::<Result<Vec<_>, RISICOError>>()
        };
        let tile_results = match &tile_pool {
            Some(pool) => pool.install(run_tiles)?,
            None => run_tiles()?,
        };

        handler.clear_cached_values();

        let mut output_time: Option<DateTime<Utc>> = None;
        let mut tile_outputs: Vec<TileNativeOutput> = Vec::new();
        for (outcome, tile) in tile_results.into_iter().zip(tiles) {
            let Some((produced_time, mapped)) = outcome.output else {
                continue;
            };
            match output_time {
                Some(expected) if expected != produced_time => {
                    return Err(format!(
                        "{model_name} produced output for {time} and {produced_time} in the same timestep"
                    )
                    .into());
                }
                Some(_) => {}
                None => output_time = Some(produced_time),
            }
            tile_outputs.push(TileNativeOutput::new(mapped, &tile.model_positions));
        }
        let warm_state = warm_state_sink
            .into_inner()
            .expect("warm state sink lock is poisoned");
        let warm_state_cells = warm_state_cells.load(Ordering::Relaxed);

        if let Some(output_time) = output_time {
            if tile_outputs.len() != tiles.len() {
                return Err(format!(
                    "{model_name} produced output for only {} of {} tiles at {time}",
                    tile_outputs.len(),
                    tiles.len()
                )
                .into());
            }
            let scratch_paths: Vec<PathBuf> = tile_outputs
                .iter()
                .map(|tile| tile.path().to_path_buf())
                .collect();
            let joined = TiledNativeOutputs::new(cell_count, tile_outputs)?;
            joined.sync_all()?;
            info!(
                "{model_name}: postprocessing output {}",
                output_time.format("%Y-%m-%d %H:%M")
            );
            output_writer.write_tiled_output(&lats, &lons, output_time, &joined)?;
            drop(joined);
            for path in scratch_paths {
                fs::remove_file(&path).map_err(|error| {
                    format!(
                        "cannot remove completed output scratch {}: {error}",
                        path.display()
                    )
                })?;
            }
        } else {
            tile_outputs.clear();
        }

        if let Some(records) = warm_state {
            if warm_state_cells != cell_count {
                return Err(format!(
                    "{model_name} warm state at {time} covers {warm_state_cells} of {cell_count} cells"
                )
                .into());
            }
            info!(
                "{model_name}: writing warm state {}",
                time.format("%Y-%m-%d %H:%M")
            );
            config.write_warm_state_records(&records, *time)?;
        }
    }

    handler.clear_registered_coordinates();

    for path in state_paths {
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "cannot remove completed tile state checkpoint {}: {error}",
                    path.display()
                )
            })?;
        }
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

#[cfg(test)]
mod budget_tests {
    use super::tiles_within_budget;

    const GB: u64 = 1_000_000_000;

    #[test]
    fn a_budget_pays_for_what_is_left_after_the_domain() {
        // 8 GB budget, 4 GB domain, 500 MB per tile: eight tiles fit.
        assert_eq!(tiles_within_budget(8 * GB, 4 * GB, GB / 2, 14), 8);
        // Raising the budget buys more tiles until the ceiling stops it.
        assert_eq!(tiles_within_budget(12 * GB, 4 * GB, GB / 2, 14), 14);
        // Lowering it gives them back.
        assert_eq!(tiles_within_budget(5 * GB, 4 * GB, GB / 2, 14), 2);
    }

    #[test]
    fn a_budget_below_the_domain_still_runs_one_tile_at_a_time() {
        assert_eq!(tiles_within_budget(GB, 4 * GB, GB / 2, 14), 1);
        assert_eq!(tiles_within_budget(0, 4 * GB, GB / 2, 14), 1);
    }

    #[test]
    fn the_ceiling_is_never_exceeded_and_never_reaches_zero() {
        assert_eq!(tiles_within_budget(1_000 * GB, 0, GB, 4), 4);
        assert_eq!(tiles_within_budget(1_000 * GB, 0, GB, 0), 1);
        // A model that costs nothing per tile still respects the ceiling.
        assert_eq!(tiles_within_budget(GB, 0, 0, 6), 6);
    }
}
