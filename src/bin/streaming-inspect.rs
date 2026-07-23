mod common;

use std::error::Error;

use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;

use common::config::builder::{ConfigBuilderType, ConfigContainer};
use common::config::models::TileModelFactory;

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
    config: &impl TileModelFactory,
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
    println!(
        "{model}: {} cells in {} non-empty tiles (largest tile: {largest} cells), {} native output variables",
        plan.cell_count(),
        plan.tiles().len(),
        config.native_output_variables().len(),
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let date = NaiveDateTime::parse_from_str(&args.date, "%Y%m%d%H%M")?;
    let date = DateTime::from_naive_utc_and_offset(date, Utc);
    let mut configs = ConfigContainer::from_file(&args.config)?;
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
