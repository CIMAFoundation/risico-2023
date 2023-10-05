use super::functions::{update_dffm_dry, update_dffm_dry_legacy, update_dffm_rain_legacy, update_dffm_rain, get_v_legacy, get_v0_legacy, get_v, get_v0};

/// configuration structure for model config 
/// can be used to store functions and constants
/// 

#[derive(Debug)]
pub struct ModelConfig {
    model_version: String,

    ffmc_no_rain_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> f32,
    ffmc_rain_fn: fn(f32, f32, f32) -> f32,
    ros0_fn : fn(f32, f32, f32, f32, f32) -> f32,
    ros_fn : fn(f32, f32, f32, f32, f32, f32) -> f32
}

impl ModelConfig {
    pub fn new(model_version_str: &str) -> Self{                
        log::info!("Model version: {}", model_version_str);
        let ffmc_no_rain_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> f32;
        let ffmc_rain_fn: fn(f32, f32, f32) -> f32;
        let ros0_fn : fn(f32, f32, f32, f32, f32) -> f32;
        let ros_fn : fn(f32, f32, f32, f32, f32, f32) -> f32;

        match model_version_str {
            "legacy" => {
                ffmc_no_rain_fn = update_dffm_dry_legacy;
                ffmc_rain_fn = update_dffm_rain_legacy;
                ros0_fn = get_v0_legacy;
                ros_fn = get_v_legacy;
            },
            "v2023" => {
                ffmc_no_rain_fn = update_dffm_dry;
                ffmc_rain_fn = update_dffm_rain;
                ros0_fn = get_v0;
                ros_fn = get_v;
            },
            _ => {
                ffmc_no_rain_fn = update_dffm_dry_legacy;
                ffmc_rain_fn = update_dffm_rain_legacy;
                ros0_fn = get_v0_legacy;
                ros_fn = get_v_legacy;
            }
        }


        ModelConfig {
            model_version: model_version_str.to_owned(),
            ffmc_no_rain_fn,
            ffmc_rain_fn,
            ros0_fn,
            ros_fn
        }
    }
    
    #[allow(non_snake_case)]
    pub fn ffmc_no_rain(&self, dffm: f32, sat: f32, T: f32, W: f32, H: f32, T0: f32, dT: f32) -> f32 {
        (self.ffmc_no_rain_fn)(dffm, sat, T, W, H, T0, dT)
    }

    pub fn ffmc_rain(&self, r: f32, dffm: f32, sat: f32) -> f32 {
        (self.ffmc_rain_fn)(r, dffm, sat)
    }

    pub fn ros0(&self, v0: f32, d0: f32, _d1: f32, dffm: f32, snow_cover: f32) -> f32 {
        (self.ros0_fn)(v0, d0, _d1, dffm, snow_cover)
    }

    pub fn ros(&self, v0: f32, slope: f32, aspect: f32, w_speed: f32, w_dir: f32, t_effect: f32) -> f32 {
        (self.ros_fn)(v0, slope, aspect, w_speed, w_dir, t_effect)
    }

}
