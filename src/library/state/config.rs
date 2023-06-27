use super::functions::{update_dffm_dry, update_dffm_dry_legacy, update_dffm_rain_legacy, update_dffm_rain};

/// configuration structure for model config 
/// can be used to store functions and constants
/// 

#[derive(Debug)]
pub struct ModelConfig {
    model_version: String
}

impl ModelConfig {
    pub fn new(model_version_str: &str) -> Self{                
        log::info!("Model version: {}", model_version_str);
        ModelConfig {
            model_version: model_version_str.to_owned()
        }
    }
    
    #[allow(non_snake_case)]
    pub fn ffmc_no_rain_function(&self, dffm: f32, _sat: f32, T: f32, W: f32, H: f32, T0: f32, dT: f32) -> f32 {
        match self.model_version.as_str() {
            "legacy" => update_dffm_dry_legacy(dffm, _sat, T, W, H, T0, dT),
            "v2023" => update_dffm_dry(dffm, _sat, T, W, H, T0, dT),
            _ => update_dffm_dry_legacy(dffm, _sat, T, W, H, T0, dT)
        }
    }

    pub fn ffmc_rain_function(&self, r: f32, dffm: f32, sat: f32) -> f32 {
        match self.model_version.as_str() {
            "legacy" => update_dffm_rain_legacy(r, dffm, sat),
            "v2023" => update_dffm_rain(r, dffm, sat),
            _ => update_dffm_rain_legacy(r, dffm, sat)
        }
    }
}
