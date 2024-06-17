use super::functions::{
    get_v, get_v_legacy, update_dffm_dry, update_dffm_dry_legacy, update_dffm_rain,
    update_dffm_rain_legacy,
};

/// configuration structure for model config
/// can be used to store functions and constants
///

#[derive(Debug)]
pub struct ModelConfig {
    pub model_version: String,

    pub use_t_effect: bool,
    ffmc_no_rain_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> f32,
    ffmc_rain_fn: fn(f32, f32, f32) -> f32,
    ros_fn: fn(f32, f32, f32, f32, f32, f32, f32, f32, f32, f32) -> (f32, f32),
}

impl ModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        log::info!("Model version: {}", model_version_str);
        let ffmc_no_rain_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> f32;
        let ffmc_rain_fn: fn(f32, f32, f32) -> f32;
        let ros_fn: fn(f32, f32, f32, f32, f32, f32, f32, f32, f32, f32) -> (f32, f32);

        match model_version_str {
            "legacy" => {
                ffmc_no_rain_fn = update_dffm_dry_legacy;
                ffmc_rain_fn = update_dffm_rain_legacy;
                ros_fn = get_v_legacy;
            }
            "v2023" => {
                ffmc_no_rain_fn = update_dffm_dry;
                ffmc_rain_fn = update_dffm_rain;
                ros_fn = get_v;
            }
            _ => {
                ffmc_no_rain_fn = update_dffm_dry_legacy;
                ffmc_rain_fn = update_dffm_rain_legacy;
                ros_fn = get_v_legacy;
            }
        }

        ModelConfig {
            model_version: model_version_str.to_owned(),
            use_t_effect: false,
            ffmc_no_rain_fn,
            ffmc_rain_fn,
            ros_fn,
        }
    }

    #[allow(non_snake_case)]
    pub fn ffmc_no_rain(
        &self,
        dffm: f32,
        sat: f32,
        T: f32,
        W: f32,
        H: f32,
        T0: f32,
        dT: f32,
    ) -> f32 {
        (self.ffmc_no_rain_fn)(dffm, sat, T, W, H, T0, dT)
    }

    pub fn ffmc_rain(&self, r: f32, dffm: f32, sat: f32) -> f32 {
        (self.ffmc_rain_fn)(r, dffm, sat)
    }

    pub fn ros(
        &self,
        v0: f32,
        d0: f32,
        _d1: f32,
        dffm: f32,
        snow_cover: f32,
        slope: f32,
        aspect: f32,
        wind_speed: f32,
        wind_dir: f32,
        t_effect: f32,
    ) -> (f32, f32) {
        (self.ros_fn)(
            v0, d0, _d1, snow_cover, dffm, slope, aspect, wind_speed, wind_dir, t_effect,
        )
    }
}
