#![allow(dead_code)]
// import state from lib
mod library;
const VERSION: &str = env!("CARGO_PKG_VERSION");

use clap::Parser;
use library::{config::serde::SerializableConfig, version::LONG_VERSION};


#[derive(Parser, Debug)]
#[command(
    author, 
    version, 
    about, 
    version, 
    long_version=LONG_VERSION, 
    )]
struct Args {
    /// configuration file
    #[arg(required=true, index=1)]
    config_file: String,
}

fn main() {
    let args = Args::parse();
    let config_path = args.config_file;
    let config = SerializableConfig::new(&config_path).expect("Could not configure model");
    let yml_str = serde_yaml::to_string(&config).expect("Could not convert config to yaml");
    println!("{}", yml_str);
}