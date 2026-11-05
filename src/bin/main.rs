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
    thread::ScopedJoinHandle,
    time::Duration,
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
            models::{ExecutionDecision, OutputWriter, TileModelRuntime, TiledModelConfig},
        },
        helpers::{get_input, RISICOError},
        io::{
            readers::{
                binary::BinaryInputHandler,
                netcdf::{NetCdfInputConfiguration, NetCdfInputHandler},
                prelude::InputHandler,
            },
            streaming::{NativeOutputScratch, SpatialTile, TileNativeOutput, TiledNativeOutputs},
        },
        process::{describe_memory, format_bytes, PhaseTimer},
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
    output: Option<(DateTime<Utc>, NativeOutputScratch)>,
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

/// Say how the run is laid out, and warn when the budget cannot be met.
fn log_execution_decision(model_name: &str, decision: &ExecutionDecision) {
    let state = if decision.resident_state {
        format!("in memory ({})", format_bytes(decision.state_bytes))
    } else {
        String::from("checkpointed to disk")
    };
    let concurrency = match decision.tile_concurrency {
        Some(threads) => format!("{threads} concurrent tiles"),
        None => format!("one tile per core ({} tiles)", decision.ceiling),
    };
    let finish = if decision.overlap_postprocess {
        format!(
            "output written while the next step runs ({})",
            format_bytes(decision.finish_bytes)
        )
    } else {
        String::from("output written between steps")
    };
    info!(
        "{model_name}: tile state {state}; {concurrency}; {finish}; expected peak about {} \
         (domain {}, {} per tile)",
        format_bytes(decision.expected_peak()),
        format_bytes(decision.estimate.resident),
        format_bytes(decision.estimate.per_tile)
    );
    if let Some(budget) = decision.budget {
        if decision.over_budget() {
            warn!(
                "{model_name}: max_memory is {} but the run needs about {} even one tile \
                 at a time; running at the smallest footprint available",
                format_bytes(budget),
                format_bytes(decision.estimate.at_concurrency(1))
            );
        }
    }
}

/// Run `work` on the tile pool when one was built, else on the global pool.
fn in_pool<T: Send>(pool: &Option<rayon::ThreadPool>, work: impl FnOnce() -> T + Send) -> T {
    match pool {
        Some(pool) => pool.install(work),
        None => work(),
    }
}

/// A finish running behind the current step, with the timestamp it belongs to.
type PendingFinish<'scope> = (
    DateTime<Utc>,
    ScopedJoinHandle<'scope, Result<FinishReport, RISICOError>>,
);

/// How long the two halves of finishing a timestep took.
struct FinishReport {
    postprocess: Option<Duration>,
    warm_state: Option<Duration>,
}

/// Resample and write one timestep's output, then its warm state.
///
/// Everything it needs is owned or shared, so it can run on its own thread
/// while the next timestep is stepped. The writer is behind a mutex only so
/// the borrow can be shared; at most one finish runs at a time.
#[allow(clippy::too_many_arguments)]
fn finish_timestep<C>(
    model_name: &str,
    config: &C,
    output_writer: &Mutex<OutputWriter>,
    lats: &[f32],
    lons: &[f32],
    cell_count: usize,
    time: DateTime<Utc>,
    output: Option<(DateTime<Utc>, Vec<TileNativeOutput<'_>>)>,
    warm_state: Option<Vec<C::WarmState>>,
) -> Result<FinishReport, RISICOError>
where
    C: TileModelRuntime,
{
    let mut report = FinishReport {
        postprocess: None,
        warm_state: None,
    };

    if let Some((output_time, tile_outputs)) = output {
        let scratch_paths: Vec<PathBuf> = tile_outputs
            .iter()
            .map(|tile| tile.path().to_path_buf())
            .collect();
        let joined = TiledNativeOutputs::new(cell_count, tile_outputs)?;
        info!(
            "{model_name}: postprocessing output {}",
            output_time.format("%Y-%m-%d %H:%M")
        );
        let start = std::time::Instant::now();
        output_writer
            .lock()
            .expect("output writer lock is poisoned")
            .write_tiled_output(lats, lons, output_time, &joined)?;
        report.postprocess = Some(start.elapsed());
        drop(joined);
        for path in scratch_paths {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "cannot remove completed output scratch {}: {error}",
                    path.display()
                )
            })?;
        }
    }

    if let Some(records) = warm_state {
        info!(
            "{model_name}: writing warm state {}",
            time.format("%Y-%m-%d %H:%M")
        );
        let start = std::time::Instant::now();
        config.write_warm_state_records(&records, time)?;
        report.warm_state = Some(start.elapsed());
    }

    Ok(report)
}

/// Wait for a background finish, fold its timings in, and log them.
fn join_finish(
    model_name: &str,
    timer: &mut PhaseTimer,
    time: DateTime<Utc>,
    handle: ScopedJoinHandle<'_, Result<FinishReport, RISICOError>>,
) -> Result<(), RISICOError> {
    let report = handle
        .join()
        .map_err(|_| RISICOError::from(format!("{model_name}: finishing {time} panicked")))??;
    record_finish(model_name, timer, time, &report);
    Ok(())
}

fn record_finish(
    model_name: &str,
    timer: &mut PhaseTimer,
    time: DateTime<Utc>,
    report: &FinishReport,
) {
    let mut phases = Vec::new();
    if let Some(elapsed) = report.postprocess {
        timer.add("postprocess", elapsed);
        phases.push(format!("postprocess {:.1}s", elapsed.as_secs_f64()));
    }
    if let Some(elapsed) = report.warm_state {
        timer.add("warm state", elapsed);
        phases.push(format!("warm state {:.1}s", elapsed.as_secs_f64()));
    }
    if !phases.is_empty() {
        info!(
            "{model_name}: {} finished: {}; {}",
            time.format("%Y-%m-%d %H:%M"),
            phases.join(", "),
            describe_memory()
        );
    }
}

/// Where one tile's state lives between timesteps.
enum TileSlot<S> {
    /// Held here for the whole run.
    Resident(S),
    /// Restored from, and checkpointed back to, the tile's state file.
    OnDisk,
}

/// Write every tile's initial state, taken from the domain warm state.
///
/// Doing it before the timeline starts is what lets the warm state be released
/// straight away, and it costs one checkpoint per tile that the first timestamp
/// would have written anyway.
fn seed_tile_states<C>(
    config: &C,
    tiles: &[SpatialTile],
    state_paths: &[PathBuf],
) -> Result<(), RISICOError>
where
    C: TileModelRuntime + Sync,
{
    tiles
        .par_iter()
        .zip(state_paths.par_iter())
        .try_for_each(|(tile, path)| {
            let state = config.tile_state(tile);
            config.checkpoint_tile_state(&state, path)
        })
}

fn run_tiled_model<C>(
    model_name: &str,
    config: &mut C,
    handler: &mut dyn InputHandler,
    execution: &StreamingExecutionConfig,
) -> Result<(), RISICOError>
where
    C: TileModelRuntime + Sync,
    C::WarmState: Send,
    C::State: Send,
    C::Properties: Send + Sync,
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
        "{model_name}: processing {cell_count} cells in {} spatial tiles; {}",
        tiles.len(),
        describe_memory()
    );
    let mut timer = PhaseTimer::new();

    let run_date = timeline.first().copied().unwrap_or_else(Utc::now);
    let scratch = create_run_scratch(execution, model_name, run_date)?;
    let state_paths: Vec<PathBuf> = tiles
        .iter()
        .map(|tile| scratch.join(format!("state-tile-{}.bin", tile.ordinal)))
        .collect();
    let output_writer = Mutex::new(config.output_writer()?);

    // Tiles run concurrently, so memory in flight scales with how many are
    // allowed at once. An explicit pool keeps that bounded and independent of
    // any other rayon use in the process.
    let largest_tile = tiles.iter().map(SpatialTile::len).max().unwrap_or(0);
    let output_bytes = output_writer
        .lock()
        .expect("output writer lock is poisoned")
        .peak_bytes(rayon::current_num_threads(), largest_tile);
    let decision = ExecutionDecision::resolve(
        config.memory_model(),
        execution,
        &plan,
        rayon::current_num_threads(),
        output_bytes,
    )?;
    log_execution_decision(model_name, &decision);
    let tile_pool = match decision.tile_concurrency {
        None => None,
        Some(threads) => Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(|error| format!("cannot build the tile thread pool: {error}"))?,
        ),
    };

    // Tile properties never change, so gather each tile's once and let the
    // model drop the domain arrays they came from: the run then holds one
    // copy instead of re-cloning a tile's slice every timestep.
    let (tile_properties, _) = timer.time("properties", || {
        in_pool(&tile_pool, || {
            tiles
                .par_iter()
                .map(|tile| config.tile_properties(tile))
                .collect::<Vec<C::Properties>>()
        })
    });
    config.release_properties();

    // Seed every tile's state from the domain warm state, then let the model
    // drop it: from here on a tile carries its own state, in memory or in its
    // checkpoint, so the one array that is read only at the first timestamp
    // stops being held for the whole run.
    let mut slots: Vec<TileSlot<C::State>> = if decision.resident_state {
        let (states, _) = timer.time("seed", || {
            in_pool(&tile_pool, || {
                tiles
                    .par_iter()
                    .map(|tile| config.tile_state(tile))
                    .collect::<Vec<_>>()
            })
        });
        states.into_iter().map(TileSlot::Resident).collect()
    } else {
        timer
            .time("seed", || {
                in_pool(&tile_pool, || seed_tile_states(config, tiles, &state_paths))
            })
            .0?;
        tiles.iter().map(|_| TileSlot::OnDisk).collect()
    };
    config.release_warm_state();

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

    // One finish may run behind the step that follows it. Its output lives
    // on disk already, so what it adds in memory is the grids being
    // resampled and the warm state being written; the decision above only
    // allows the overlap when that fits beside a step.
    let timer = &mut timer;
    let output_writer = &output_writer;
    let config = &*config;
    let lats = lats.as_slice();
    let lons = lons.as_slice();
    let run_result = std::thread::scope(|scope| -> Result<(), RISICOError> {
        let mut pending: Option<PendingFinish<'_>> = None;
        for time in &timeline {
            info!("{model_name}: processing {}", time.format("%Y-%m-%d %H:%M"));
            // Decode this timestamp once so concurrent tiles only read the cache.
            let (_, preload_elapsed) = timer.time("preload", || handler.preload(time));

            // The whole domain's warm state is filled in place. Tiles own disjoint
            // positions, so the lock only orders the scatters and is held for a
            // small fraction of a tile's work.
            let warm_state_sink: Mutex<Option<Vec<C::WarmState>>> = Mutex::new(None);
            let warm_state_cells = AtomicUsize::new(0);
            let handler_ref: &dyn InputHandler = handler;

            // Tiles are independent: disjoint cells, disjoint states and one
            // output scratch each. `collect` restores plan order afterwards.
            let slots = &mut slots;
            let run_tiles = || -> Result<Vec<TileOutcome>, RISICOError> {
                tiles
            .par_iter()
            .zip(slots.par_iter_mut())
            .enumerate()
            .map(|(tile_index, (tile, slot))| -> Result<TileOutcome, RISICOError> {
                let selection = tile_selections[tile_index];
                let properties = &tile_properties[tile_index];
                let state_path = &state_paths[tile_index];
                let on_disk = matches!(slot, TileSlot::OnDisk);
                let mut restored;
                let state: &mut C::State = match slot {
                    TileSlot::Resident(state) => state,
                    TileSlot::OnDisk => {
                        // Every tile was checkpointed before the timeline
                        // started, so this is always a restore: the shell is
                        // sized from the tile and its contents come from disk.
                        restored = config.tile_state(tile);
                        config.restore_tile_state(&mut restored, state_path)?;
                        &mut restored
                    }
                };
                let input = get_input(handler_ref, selection, time, tile.len());
                let step = config.step(state, properties, &input);

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
                        let scratch =
                            NativeOutputScratch::create(path, native_variables.clone(), tile.len())?;
                        scratch.write_output(&output)?;
                        Some((output.time, scratch))
                    }
                };

                if step.write_warm_state {
                    let records = config.tile_warm_state(state);
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

                if on_disk {
                    config.checkpoint_tile_state(state, state_path)?;
                }
                Ok(TileOutcome { output })
            })
            .collect::<Result<Vec<_>, RISICOError>>()
            };
            let (tile_results, step_elapsed) =
                timer.time("step", || in_pool(&tile_pool, run_tiles));
            let tile_results = tile_results?;

            handler.clear_cached_values();

            let mut output_time: Option<DateTime<Utc>> = None;
            let mut tile_outputs: Vec<TileNativeOutput> = Vec::new();
            for (outcome, tile) in tile_results.into_iter().zip(tiles) {
                let Some((produced_time, scratch)) = outcome.output else {
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
                tile_outputs.push(TileNativeOutput::new(scratch, &tile.model_positions));
            }
            let warm_state = warm_state_sink
                .into_inner()
                .expect("warm state sink lock is poisoned");
            let warm_state_cells = warm_state_cells.load(Ordering::Relaxed);

            let output = match output_time {
                Some(output_time) if tile_outputs.len() != tiles.len() => {
                    return Err(format!(
                        "{model_name} produced output for only {} of {} tiles at {output_time}",
                        tile_outputs.len(),
                        tiles.len()
                    )
                    .into());
                }
                Some(output_time) => Some((output_time, tile_outputs)),
                None => None,
            };
            if warm_state.is_some() && warm_state_cells != cell_count {
                return Err(format!(
                "{model_name} warm state at {time} covers {warm_state_cells} of {cell_count} cells"
            )
                .into());
            }

            info!(
                "{model_name}: {} stepped: preload {:.1}s, step {:.1}s; {}",
                time.format("%Y-%m-%d %H:%M"),
                preload_elapsed.as_secs_f64(),
                step_elapsed.as_secs_f64(),
                describe_memory()
            );

            // The previous finish must be done before this one starts: a depth
            // of one is what the memory decision was made for.
            if let Some((finished_time, handle)) = pending.take() {
                join_finish(model_name, timer, finished_time, handle)?;
            }
            if output.is_none() && warm_state.is_none() {
                continue;
            }
            let time = *time;
            let finish = move || {
                finish_timestep(
                    model_name,
                    config,
                    output_writer,
                    lats,
                    lons,
                    cell_count,
                    time,
                    output,
                    warm_state,
                )
            };
            if decision.overlap_postprocess {
                pending = Some((time, scope.spawn(finish)));
            } else {
                let report = finish()?;
                record_finish(model_name, timer, time, &report);
            }
        }
        if let Some((finished_time, handle)) = pending.take() {
            join_finish(model_name, timer, finished_time, handle)?;
        }
        Ok(())
    });

    // Outputs that stay open across the run (NetCDF files) get their
    // metadata now. This runs after a failed run too, so the timesteps
    // written before the failure stay readable.
    let (finish_result, finish_elapsed) = timer.time("finish", || {
        output_writer
            .lock()
            .expect("output writer lock is poisoned")
            .finish()
    });
    run_result?;
    finish_result?;
    info!(
        "{model_name}: outputs finished in {:.1}s",
        finish_elapsed.as_secs_f64()
    );

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
    info!(
        "{model_name}: phase totals: {}; {}",
        timer.summary(),
        describe_memory()
    );
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

    let mut failed_models: Vec<String> = Vec::new();

    for model_config in &configs.models {
        let model_name = model_config.get_model_name();
        info!("Running model: {model_name}");
        let start = Utc::now();
        let result =
            match model_config {
                ConfigBuilderType::RISICO(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::FWI(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Mark5(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::KBDI(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Angstrom(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Fosberg(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Nesterov(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Sharples(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Orieux(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
                ConfigBuilderType::Hdw(builder) => builder
                    .build(&date, &configs.palettes)
                    .and_then(|mut config| {
                        run_tiled_model(
                            model_name,
                            &mut config,
                            input_handler.as_mut(),
                            &configs.streaming,
                        )
                    }),
            };
        if let Err(error) = result {
            warn!("Error running model {model_name}: {error}");
            failed_models.push(model_name.to_string());
        }
        info!(
            "{model_name} elapsed time: {} seconds",
            (Utc::now() - start).num_seconds()
        );
    }

    if !failed_models.is_empty() {
        // Each model failure is logged above so the others can continue; the
        // process must still report it, or an operational launcher would
        // treat a run without output as a success.
        return Err(format!("model execution failed for: {}", failed_models.join(", ")).into());
    }

    Ok(())
}
