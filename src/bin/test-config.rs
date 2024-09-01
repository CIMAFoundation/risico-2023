mod common;

use common::config::builder::ConfigContainer;
use serde_yaml;
use std::io::Read;
use std::{error::Error, fs::File};

pub fn main() -> Result<(), Box<dyn Error>> {
    let config_file = "configuration.yml";
    let mut file = File::open(config_file)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let conf: ConfigContainer = serde_yaml::from_str(&contents)?;

    println!("{:#?}", conf);
    Ok(())
}
