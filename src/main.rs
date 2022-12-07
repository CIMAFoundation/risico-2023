#![allow(dead_code)]

use std::collections::HashMap;

use library::config::models::read_config;
mod library;

#[derive(Default, Debug, Clone)]
struct ConfigOutputVariable {
    internal_name: String,
    name: String,
    cluster_mode: String,
    precision:  String,
}

#[derive(Default, Debug, Clone)]
struct ConfigOutputType {
    name: String,
    path: String,
    grid: String,
    format: String,
    variables: Vec<ConfigOutputVariable>
}

#[derive(Default, Debug)]
struct ConfigBuilder {
    pub model_name: String,
    pub warm_state_path: String,
    pub cells_file: String,
    pub vegetation_file: String,
    pub ppf_file: String,
    pub cache_path: String,
    pub use_temperature_effect: bool,
    pub use_ndvi: bool,
    pub outputs: Vec<ConfigOutputType>
}


fn main() -> Result<(), Box<dyn std::error::Error>>{
    let config_map = read_config("data/config.txt").unwrap();
    let mut config_builder = ConfigBuilder::default();

    let model_name_key = "MODELNAME";
    let warm_state_path_key = "STATO0";
    let cells_file_key = "CELLE";
    let vegetation_file_key = "VEG";
    let ppf_file_key = "PPF";  
    let cache_path_key = "BUFFERS";
    let use_temperature_effect_key = "USETCONTR";
    let use_ndvi_key = "USENDVI";
    let outputs_key = "MODEL";
    let variables_key = "VARIABLE";


    // try to get the model name, expect it to be there
    let model_name = config_map.get(model_name_key).unwrap().get(0).unwrap();
    let warm_state_file = config_map.get(warm_state_path_key).unwrap().get(0).unwrap();
    let cells_file = config_map.get(cells_file_key).unwrap().get(0).unwrap();
    let vegetation_file = config_map.get(vegetation_file_key).unwrap().get(0).unwrap();
    let ppf_file = config_map.get(ppf_file_key).unwrap().get(0).unwrap();
    let cache_path = config_map.get(cache_path_key).unwrap().get(0).unwrap();
    let use_temperature_effect = config_map.get(use_temperature_effect_key).unwrap().get(0).unwrap();
    let use_ndvi = config_map.get(use_ndvi_key).unwrap().get(0).unwrap();

    let output_types_defs = config_map.get(outputs_key).ok_or("Outputs not found")?;
    let variables_defs = config_map.get(variables_key).ok_or("Variables not found")?;

    let mut output_types_map: HashMap<String, ConfigOutputType> = HashMap::new();
    for output_def in output_types_defs {
        let parts = output_def.split(":").collect::<Vec<&str>>();
        if parts.len() != 5 {
            return Err("Invalid output definition".into());
        }
        let (internal_name, name, path, grid, format) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
        let mut output_type = ConfigOutputType::default();

        output_type.name = name.to_string();
        output_type.path = path.to_string();
        output_type.grid = grid.to_string();
        output_type.format = format.to_string();

        output_types_map.insert(internal_name.to_string(), output_type);
    }

    for variable_def in variables_defs {
        let parts = variable_def.split(":").collect::<Vec<&str>>();
        if parts.len() != 5 {
            return Err("Invalid variable definition".into());
        }
        let (output_type, internal_name, name, cluster_mode, precision) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
        let mut variable = ConfigOutputVariable::default();
        variable.internal_name = internal_name.to_string();
        variable.name = name.to_string();
        variable.cluster_mode = cluster_mode.to_string();
        variable.precision = precision.to_string();
        
        let mut output_type = output_types_map.get_mut(output_type).ok_or(format!("Output type not found {output_type}"))?;
        output_type.variables.push(variable);
    }

    config_builder.model_name = model_name.to_string();
    config_builder.warm_state_path = warm_state_file.to_string();
    config_builder.cells_file = cells_file.to_string();
    config_builder.vegetation_file = vegetation_file.to_string();
    config_builder.ppf_file = ppf_file.to_string();
    config_builder.cache_path = cache_path.to_string();
    config_builder.use_temperature_effect = match use_temperature_effect.as_str() {
        "0" => false,
        "1" => true,
        "true" => true,
        "false" => false,
        "TRUE" => true,
        "FALSE" => false,
        _ => false
    };
    config_builder.use_ndvi = match use_ndvi.as_str() {
        "0" => false,
        "1" => true,
        "true" => true,
        "false" => false,
        "TRUE" => true,
        "FALSE" => false,
        _ => false
    };
    config_builder.outputs = output_types_map.values().map(|output| output.clone()).collect();

    println!("{:#?}", config_builder);
    

    Ok(())
}

