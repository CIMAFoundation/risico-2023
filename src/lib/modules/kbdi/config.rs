use super::functions::kbdi_update_mm;

/// configuration structure for model config
/// can be used to store functions and constants
#[derive(Debug)]
pub struct KBDIModelConfig {
    pub model_version: String,
    kbdi_fn: fn(f32, f32, &Vec<f32>, f32) -> f32,
}

impl KBDIModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        let kbdi_fn: fn(f32, f32, &Vec<f32>, f32) -> f32;
        match model_version_str {
            "legacy" => {
                kbdi_fn = kbdi_update_mm;
            }
            _ => {
                kbdi_fn = kbdi_update_mm;
            }
        }

        KBDIModelConfig {
            model_version: model_version_str.to_owned(),
            kbdi_fn,
        }
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn update_kbdi(&self,
        kbdi: f32,  // previous SMD value
        temp: f32,  // temperature [°C]
        history_rain: &Vec<f32>,  // daily rain of the last days [mm]
        mean_annual_rain: f32,  // mean annual rain [mm]
    ) -> f32 {
        (self.kbdi_fn)(kbdi, temp, history_rain, mean_annual_rain)
    }


}
