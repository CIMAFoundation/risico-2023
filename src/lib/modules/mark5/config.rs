use super::functions::{store_day_extremes, kbdi_update, kbdi_output};
use super::models::{Mark5StateElement, Mark5PropertiesElement};
use crate::models::{input::InputElement, output::OutputElement};
use chrono::prelude::*;

/// configuration structure for model config
/// can be used to store functions and constants
#[derive(Debug)]
pub struct Mark5ModelConfig {
    pub model_version: String,
    // store day info method
    store_day_fn: fn(&mut Mark5StateElement, &InputElement, &DateTime<Utc>),
    // soil moisture deficit function
    smd_fn: fn(&mut Mark5StateElement, &Mark5PropertiesElement, &DateTime<Utc>),
    // return output element
    get_output_fn: fn(&Mark5StateElement, f32, f32) -> OutputElement,
}

impl Mark5ModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        let store_day_fn: fn(&mut Mark5StateElement, &InputElement, &DateTime<Utc>);
        let smd_fn: fn(&mut Mark5StateElement, &Mark5PropertiesElement, &DateTime<Utc>);
        let get_output_fn: fn(&Mark5StateElement, f32, f32) -> OutputElement;
        match model_version_str {
            "legacy" => {
                store_day_fn = store_day_extremes;
                smd_fn = kbdi_update;
                get_output_fn = kbdi_output;
            }
            _ => {
                store_day_fn = store_day_extremes;
                smd_fn = kbdi_update;
                get_output_fn = kbdi_output;
            }
        }

        Mark5ModelConfig {
            model_version: model_version_str.to_owned(),
            store_day_fn,
            smd_fn,
            get_output_fn,
        }
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn store_day(
        &self,
        state: &mut Mark5StateElement,
        input: &InputElement,
        time: &DateTime<Utc>,
    ) {
        (self.store_day_fn)(state, input, time);
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn update_smd(&self,
        state: &mut Mark5StateElement,
        props: &Mark5PropertiesElement,
        time: &DateTime<Utc>
    ) {
        (self.smd_fn)(state, props, time);
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn get_output(
        &self,
        state: &Mark5StateElement,
        df: f32,
        ffdi: f32,
    ) -> OutputElement {
        (self.get_output_fn)(state, df, ffdi)
    }

}
