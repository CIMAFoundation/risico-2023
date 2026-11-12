use super::functions::{
    get_v_v2025, get_v_v2023, get_v_legacy, update_dffm_dry, update_dffm_dry_legacy, update_dffm_rain,
    update_dffm_rain_legacy, get_meteo_index_v2023, get_meteo_index_v2025, get_meteo_index_legacy,
};

type RosFnType = fn(f32, f32, f32, f32, f32, f32, f32, f32, f32, f32) -> (f32, f32);

/// Default bounds, in hours, of the time step used to update the fine fuel
/// moisture: the gridded runs are hourly or coarser.
pub const DEFAULT_DT_BOUNDS_HOURS: (f32, f32) = (1.0, 72.0);

/// configuration structure for model config
/// can be used to store functions and constants
#[derive(Debug)]
pub struct RISICOModelConfig {
    pub model_version: String,
    pub use_t_effect: bool,
    /// Bounds `(min, max)`, in hours, the time step between two inputs is
    /// clamped to before updating the fine fuel moisture.
    /// Defaults to [`DEFAULT_DT_BOUNDS_HOURS`]; point runs on sub-hourly
    /// observations lower the minimum so a 10 minute step counts as 1/6 h.
    pub dt_bounds_hours: (f32, f32),
    ffmc_no_rain_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> f32,
    ffmc_rain_fn: fn(f32, f32, f32) -> f32,
    ros_fn: RosFnType,
    meteo_index_fn: fn(f32, f32) -> f32,
}

impl RISICOModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        let ffmc_no_rain_fn: fn(f32, f32, f32, f32, f32, f32, f32) -> f32;
        let ffmc_rain_fn: fn(f32, f32, f32) -> f32;
        let ros_fn: RosFnType;
        let meteo_index_fn: fn(f32, f32) -> f32;

        match model_version_str {
            "legacy" => {
                ffmc_no_rain_fn = update_dffm_dry_legacy;
                ffmc_rain_fn = update_dffm_rain_legacy;
                ros_fn = get_v_legacy;
                meteo_index_fn = get_meteo_index_legacy;
            }
            // DEPRECATED
            // "new_moisture" => {
            //     ffmc_no_rain_fn = update_dffm_dry;
            //     ffmc_rain_fn = update_dffm_rain;
            //     ros_fn = get_v_legacy;
            //     meteo_index_fn = get_meteo_index_legacy;
            // }
            // "new_ros" => {
            //     ffmc_no_rain_fn = update_dffm_dry_legacy;
            //     ffmc_rain_fn = update_dffm_rain_legacy;
            //     ros_fn = get_v_v2023;
            //     meteo_index_fn = get_meteo_index_legacy;
            // }
            "v2023" => {
                ffmc_no_rain_fn = update_dffm_dry;
                ffmc_rain_fn = update_dffm_rain;
                ros_fn = get_v_v2023;
                meteo_index_fn = get_meteo_index_v2023;
            },
            "v2025" => {
                ffmc_no_rain_fn = update_dffm_dry;
                ffmc_rain_fn = update_dffm_rain;
                ros_fn = get_v_v2025;
                meteo_index_fn = get_meteo_index_v2025;
            }
            _ => {
                ffmc_no_rain_fn = update_dffm_dry_legacy;
                ffmc_rain_fn = update_dffm_rain_legacy;
                ros_fn = get_v_legacy;
                meteo_index_fn = get_meteo_index_legacy;
            }
        }

        RISICOModelConfig {
            model_version: model_version_str.to_owned(),
            use_t_effect: false,
            dt_bounds_hours: DEFAULT_DT_BOUNDS_HOURS,
            ffmc_no_rain_fn,
            ffmc_rain_fn,
            ros_fn,
            meteo_index_fn,
        }
    }

    /// Set the bounds, in hours, of the moisture update time step.
    pub fn with_dt_bounds_hours(mut self, min: f32, max: f32) -> Self {
        assert!(
            min >= 0.0 && min <= max,
            "invalid dt bounds [{min}, {max}] hours"
        );
        self.dt_bounds_hours = (min, max);
        self
    }

    /// Clamp a time step, in hours, to [`Self::dt_bounds_hours`].
    pub fn clamp_dt_hours(&self, dt: f32) -> f32 {
        dt.clamp(self.dt_bounds_hours.0, self.dt_bounds_hours.1)
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
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

    #[allow(clippy::too_many_arguments)]
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

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn meteo_index(&self, dffm: f32, W: f32) -> f32 {
        (self.meteo_index_fn)(dffm, W)
    }
}
