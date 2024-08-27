#![allow(dead_code)]
// import state from lib
mod library;
const VERSION: &str = env!("CARGO_PKG_VERSION");

use clap::Parser;
use library::config::serde::SerializableConfig;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// configuration file
    #[arg(short, long)]
    config_file: String,
}

fn main() {
    let args = Args::parse();
    let config_path = args.config_file;
    let config = SerializableConfig::new(&config_path).expect("Could not configure model");
    let yml_str = serde_yaml::to_string(&config).expect("Could not convert config to yaml");
    println!("{}", yml_str);
}