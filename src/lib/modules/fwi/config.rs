use chrono::{DateTime, Utc};
use super::functions::{
    update_state_legacy, update_state_sliding,
    get_output_legacy, get_output_sliding
};
use super::models::{
    FWIStateElement, FWIPropertiesElement
};
use crate::models::{input::InputElement, output::OutputElement};

/// configuration structure for model config
/// can be used to store functions and constants
#[derive(Debug)]
pub struct FWIModelConfig {
    pub model_version: String,
    update_state_fn: fn(&mut FWIStateElement, &FWIPropertiesElement, &InputElement, &DateTime<Utc>),
    get_output_fn: fn(&mut FWIStateElement, &FWIPropertiesElement, &DateTime<Utc>) -> OutputElement
}

impl FWIModelConfig {
    pub fn new(model_version_str: &str) -> Self {
        let update_state_fn: fn(&mut FWIStateElement, &FWIPropertiesElement, &InputElement, &DateTime<Utc>);
        let get_output_fn: fn(&mut FWIStateElement, &FWIPropertiesElement, &DateTime<Utc>) -> OutputElement;

        match model_version_str {
            "legacy" => {
                update_state_fn = update_state_legacy;
                get_output_fn = get_output_legacy;

            }
            "sliding_window" => {
                update_state_fn = update_state_sliding;
                get_output_fn = get_output_sliding;
            }
            _ => {
                update_state_fn = update_state_legacy;
                get_output_fn = get_output_legacy;
            }
        }

        FWIModelConfig {
            model_version: model_version_str.to_owned(),
            update_state_fn,
            get_output_fn
        }
    }

    #[allow(non_snake_case)]
    pub fn update_state(
        &self,
        state: &mut FWIStateElement,
        props: &FWIPropertiesElement,
        input: &InputElement,
        time: &DateTime<Utc>
    ) {
        (self.update_state_fn)(state, props, input, time)
    }

    #[allow(non_snake_case)]
    pub fn get_output(
        &self,
        state: &mut FWIStateElement,
        props: &FWIPropertiesElement,
        time: &DateTime<Utc>
    ) -> OutputElement {
        (self.get_output_fn)(state, props, time)
    }
}
