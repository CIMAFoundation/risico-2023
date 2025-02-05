use super::functions::kbdi_output;
use crate::models::output::OutputElement;
use crate::modules::kbdi::functions::kbdi_update_mm;

/// configuration structure for model config
/// can be used to store functions and constants
#[derive(Debug)]
pub struct Mark5ModelConfig {
    pub model_version: String,
    // soil moisture deficit function
    smd_fn: fn(f32, f32, &[f32], f32) -> f32,
    // return output element
    get_output_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> OutputElement,
}

impl Mark5ModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        let smd_fn: fn(f32, f32, &[f32], f32) -> f32;
        let get_output_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> OutputElement;
        match model_version_str {
            "legacy" => {
                smd_fn = kbdi_update_mm;
                get_output_fn = kbdi_output;
            }
            _ => {
                smd_fn = kbdi_update_mm;
                get_output_fn = kbdi_output;
            }
        }

        Mark5ModelConfig {
            model_version: model_version_str.to_owned(),
            smd_fn,
            get_output_fn,
        }
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn update_smd(
        &self,
        smd: f32,              // previous SMD value
        temp: f32,             // temperature [°C]
        history_rain: &[f32],  // daily rain of the last days [mm]
        mean_annual_rain: f32, // mean annual rain [mm]
    ) -> f32 {
        (self.smd_fn)(smd, temp, history_rain, mean_annual_rain)
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn get_output(
        &self,
        smd: f32,
        df: f32,
        ffdi: f32,
        temperature: f32,
        rain: f32,
        wind_speed: f32,
        humidity: f32,
    ) -> OutputElement {
        (self.get_output_fn)(smd, df, ffdi, temperature, rain, wind_speed, humidity)
    }
}
