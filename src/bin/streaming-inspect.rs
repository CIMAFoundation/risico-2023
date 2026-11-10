mod common;

use std::error::Error;

use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;

use common::config::builder::{BuildConfig, ConfigBuilderType, ConfigContainer};
use common::config::models::{ExecutionDecision, TileModelRuntime};
use common::process::format_bytes;

#[derive(Debug, Parser)]
#[command(about = "Validate model configuration and inspect its streaming tile plan")]
struct Args {
    /// Model date in YYYYMMDDHHMM form.
    date: String,
    /// YAML or legacy text configuration.
    config: String,
    /// Override raster tile height.
    #[arg(long)]
    tile_height: Option<usize>,
    /// Override raster tile width.
    #[arg(long)]
    tile_width: Option<usize>,
    /// Override legacy cells per tile.
    #[arg(long)]
    cells_per_tile: Option<usize>,
}

fn print_plan(
    model: &str,
    config: &impl TileModelRuntime,
    execution: &common::config::builder::StreamingExecutionConfig,
) -> Result<(), Box<dyn Error>> {
    let plan = config.tile_plan(execution)?;
    if let Some(first) = plan.tiles().first() {
        // Exercise the same adapter the runner will use, including warm-state
        // selection for history-bearing models.
        drop(config.tile_properties(first));
        drop(config.tile_state(first));
    }
    let largest = plan
        .tiles()
        .iter()
        .map(|tile| tile.len())
        .max()
        .unwrap_or(0);
    let smallest = plan
        .tiles()
        .iter()
        .map(|tile| tile.len())
        .min()
        .unwrap_or(0);
    println!(
        "{model}: {} cells in {} non-empty tiles (largest tile: {largest} cells, smallest: {smallest}), {} native output variables",
        plan.cell_count(),
        plan.tiles().len(),
        config.native_output_variables().len(),
    );

    let footprint = config.memory_model();
    let cores = rayon::current_num_threads();
    let output_bytes = config.output_writer()?.peak_bytes(cores, largest);
    let decision = ExecutionDecision::resolve(footprint, execution, &plan, cores, output_bytes)?;
    println!(
        "{model}: tile state {}; {}; {}; expected peak about {} (domain {}, {} per tile)",
        if decision.resident_state {
            format!("in memory ({})", format_bytes(decision.state_bytes))
        } else {
            String::from("checkpointed to disk")
        },
        match decision.tile_concurrency {
            Some(threads) => format!("{threads} concurrent tiles"),
            None => format!("one tile per core ({} tiles)", decision.ceiling),
        },
        if decision.overlap_postprocess {
            format!(
                "output written while the next step runs ({})",
                format_bytes(decision.finish_bytes)
            )
        } else {
            String::from("output written between steps")
        },
        format_bytes(decision.expected_peak()),
        format_bytes(decision.estimate.resident),
        format_bytes(decision.estimate.per_tile),
    );
    match decision.budget {
        Some(budget) if decision.over_budget() => println!(
            "{model}: max_memory {} is below the {} needed even one tile at a time",
            format_bytes(budget),
            format_bytes(decision.estimate.at_concurrency(1))
        ),
        Some(budget) => println!("{model}: within max_memory {}", format_bytes(budget)),
        None => {
            let on_disk = footprint.estimate(&plan, false);
            println!(
                "{model}: no max_memory set; with tile_state: disk the same run would peak at about {}",
                format_bytes(on_disk.at_concurrency(decision.ceiling))
            );
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let date = NaiveDateTime::parse_from_str(&args.date, "%Y%m%d%H%M")?;
    let date = DateTime::from_naive_utc_and_offset(date, Utc);
    let mut configs = ConfigContainer::load(&args.config)?;
    if let Some(value) = args.tile_height {
        configs.streaming.tile_height = value;
    }
    if let Some(value) = args.tile_width {
        configs.streaming.tile_width = value;
    }
    if let Some(value) = args.cells_per_tile {
        configs.streaming.cells_per_tile = value;
    }
    configs.streaming.validate()?;

    for model in &configs.models {
        match model {
            ConfigBuilderType::RISICO(builder) => print_plan(
                "RISICO",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::FWI(builder) => print_plan(
                "FWI",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Mark5(builder) => print_plan(
                "Mark5",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::KBDI(builder) => print_plan(
                "KBDI",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Angstrom(builder) => print_plan(
                "Angstrom",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Fosberg(builder) => print_plan(
                "Fosberg",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Nesterov(builder) => print_plan(
                "Nesterov",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Sharples(builder) => print_plan(
                "Sharples",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Orieux(builder) => print_plan(
                "Orieux",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
            ConfigBuilderType::Hdw(builder) => print_plan(
                "Hdw",
                &builder.build(&date, &configs.palettes)?,
                &configs.streaming,
            )?,
        }
    }
    Ok(())
}
