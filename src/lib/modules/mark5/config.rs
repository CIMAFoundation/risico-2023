use super::functions::{
    update_moisture, update_dmc, update_dc,
    compute_isi, compute_bui, compute_fwi
};

/// configuration structure for model config
/// can be used to store functions and constants
#[derive(Debug)]
pub struct FWIModelConfig {
    pub model_version: String,

    moisture_fn: fn(f32, f32, f32, f32, f32) -> f32,
    dmc_fn: fn(f32, f32, f32, f32, f32) -> f32,
    dc_fn: fn(f32, f32, f32, f32) -> f32,
    isi_fn: fn(f32, f32) -> f32,
    bui_fn: fn(f32, f32) -> f32,
    fwi_fn: fn(f32, f32) -> f32
}

impl FWIModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        let moisture_fn: fn(f32, f32, f32, f32, f32) -> f32;
        let dmc_fn: fn(f32, f32, f32, f32, f32) -> f32;
        let dc_fn: fn(f32, f32, f32, f32) -> f32;
        let isi_fn: fn(f32, f32) -> f32;
        let bui_fn: fn(f32, f32) -> f32;
        let fwi_fn: fn(f32, f32) -> f32;

        match model_version_str {
            "legacy" => {
                moisture_fn = update_moisture;
                dmc_fn = update_dmc;
                dc_fn = update_dc;
                isi_fn = compute_isi;
                bui_fn = compute_bui;
                fwi_fn = compute_fwi;
            }
            _ => {
                moisture_fn = update_moisture;
                dmc_fn = update_dmc;
                dc_fn = update_dc;
                isi_fn = compute_isi;
                bui_fn = compute_bui;
                fwi_fn = compute_fwi;
            }
        }

        FWIModelConfig {
            model_version: model_version_str.to_owned(),
            moisture_fn,
            dmc_fn,
            dc_fn,
            isi_fn,
            bui_fn,
            fwi_fn
        }
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn moisture(
        &self,
        moisture: f32,
        rain: f32,
        humidity: f32,
        temperature: f32,
        wind_speed: f32
    ) -> f32 {
        (self.moisture_fn)(moisture, rain, humidity, temperature, wind_speed)
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn dmc(
        &self,
        dmc: f32,
        rain: f32,
        temperature: f32,
        humidity: f32,
        l_e: f32
    ) -> f32 {
        (self.dmc_fn)(dmc, rain, temperature, humidity, l_e)
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn dc(
        &self,
        dc: f32,
        rain: f32,
        temperature: f32,
        l_f: f32
    ) -> f32 {
        (self.dc_fn)(dc, rain, temperature, l_f)
    }

    #[allow(non_snake_case)]
    pub fn isi(
        &self,
        moisture: f32,
        wind_speed: f32
    ) -> f32 {
        (self.isi_fn)(moisture, wind_speed)
    }

    #[allow(non_snake_case)]
    pub fn bui(
        &self,
        dmc: f32,
        dc: f32
    ) -> f32 {
        (self.bui_fn)(dmc, dc)
    }

    #[allow(non_snake_case)]
    pub fn fwi(
        &self,
        isi: f32,
        bui: f32
    ) -> f32 {
        (self.fwi_fn)(isi, bui)
    }

}
