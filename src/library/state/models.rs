use chrono::prelude::*;
use ndarray::{Array1, Zip};
use rayon::prelude::*;
use std::{collections::HashMap, sync::Arc};

use crate::library::{
    config::models::WarmState,
    state::constants::{NODATAVAL, SNOW_SECONDS_VALIDITY},
};

use super::{
    config::ModelConfig,
    constants::SATELLITE_DATA_SECONDS_VALIDITY,
    functions::{get_output_fn, update_moisture_fn},
};

//const UPDATE_TIME: i64 = 100;

fn get_derived(a: &f32, b: &f32, c: Option<&f32>) -> f32 {
    let mut r = *a;

    if *b != NODATAVAL {
        r = a * b;
    }

    if let Some(c) = c {
        if *c != NODATAVAL {
            r = r * c;
        }
    }
    r
}

#[derive(Debug)]
pub struct PropertiesElement {
    pub lon: f32,
    pub lat: f32,
    pub slope: f32,
    pub aspect: f32,
    pub ppf_summer: f32,
    pub ppf_winter: f32,
    pub vegetation: Arc<Vegetation>,
}

#[derive(Debug)]
pub struct Properties {
    pub data: Array1<PropertiesElement>,
    pub vegetations_dict: HashMap<String, Arc<Vegetation>>,
    pub len: usize,
}

impl Properties {
    pub fn new(
        lats: Vec<f32>,
        lons: Vec<f32>,
        slopes: Vec<f32>,
        aspects: Vec<f32>,
        vegetations: Vec<String>,
        vegetations_dict: HashMap<String, Arc<Vegetation>>,
        ppf_summer: Vec<f32>,
        ppf_winter: Vec<f32>,
    ) -> Self {
        // check if all vectors have the same length
        // let n_elements = lats.len();

        // if ppf_summer.len() < n_elements {
        //     println!("Warning: PPF is not consistent with cell file. Overriding missing elements.");
        //     println!("PPF: {} elements ", ppf_summer.len());
        //     println!("Cells: {} elements ", n_elements);

        // }

        let default_veg = Arc::new(Vegetation::default());
        let data: Array1<PropertiesElement> = vegetations
            .iter()
            .enumerate()
            .map(|(idx, v)| PropertiesElement {
                lon: lons[idx],
                lat: lats[idx],
                slope: slopes[idx],
                aspect: aspects[idx],
                ppf_summer: ppf_summer[idx],
                ppf_winter: ppf_winter[idx],
                vegetation: vegetations_dict.get(v).unwrap_or(&default_veg).clone(),
            })
            .collect();

        let len = data.len();
        Self {
            data,
            vegetations_dict,
            len,
        }
    }

    pub fn get_coords(&self) -> (Vec<f32>, Vec<f32>) {
        let lats: Vec<f32> = self.data.iter().map(|p| p.lat).collect();
        let lons: Vec<f32> = self.data.iter().map(|p| p.lon).collect();
        (lats, lons)
    }
}

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Vegetation {
    pub id: String,
    pub d0: f32,
    pub d1: f32,
    pub hhv: f32,
    pub umid: f32,
    pub v0: f32,
    pub T0: f32,
    pub sat: f32,
    pub name: String,
    pub use_ndvi: bool,
}

impl Default for Vegetation {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            d0: 0.5,
            d1: NODATAVAL,
            hhv: 18000.0,
            umid: NODATAVAL,
            v0: 120.0,
            T0: 30.0,
            sat: 40.0,
            name: "default".to_string(),
            use_ndvi: false,
        }
    }
}

#[allow(non_snake_case)]
pub struct OutputElement {
    pub dffm: f32,
    pub W: f32,
    pub V: f32,
    pub I: f32,
    pub NDVI: f32,
    pub NDWI: f32,
    pub PPF: f32,
    pub t_effect: f32,
    // pub SWI: f32,
    pub temperature: f32,
    pub rain: f32,
    pub wind_speed: f32,
    pub wind_dir: f32,
    pub humidity: f32,
    pub snow_cover: f32,
}

impl Default for OutputElement {
    fn default() -> Self {
        Self {
            dffm: NODATAVAL,
            W: NODATAVAL,
            V: NODATAVAL,
            I: NODATAVAL,
            NDVI: NODATAVAL,
            NDWI: NODATAVAL,
            PPF: NODATAVAL,
            t_effect: NODATAVAL,
            // SWI: NODATAVAL,
            temperature: NODATAVAL,
            rain: NODATAVAL,
            wind_speed: NODATAVAL,
            wind_dir: NODATAVAL,
            humidity: NODATAVAL,
            snow_cover: NODATAVAL,
        }
    }
}

pub struct Output {
    pub time: DateTime<Utc>,
    pub data: Array1<OutputElement>,
}

#[allow(non_snake_case)]
impl Output {
    pub fn new(time: DateTime<Utc>, data: Array1<OutputElement>) -> Self {
        Self { time, data }
    }

    pub fn get_array(&self, func: fn(&OutputElement) -> f32) -> Array1<f32> {
        let vec = self.data.par_iter().map(func).collect::<Vec<_>>();
        Array1::from_vec(vec)
    }

    pub fn get(&self, variable: &str) -> Option<Array1<f32>> {
        match variable {
            // Output variables
            "dffm" => Some(self.get_array(|o| o.dffm)),
            "W" => Some(self.get_array(|o| o.W)),
            "V" => Some(self.get_array(|o| o.V)),
            "I" => Some(self.get_array(|o| o.I)),

            "contrT" => Some(self.get_array(|o| o.t_effect)),
            // "SWI" => self.SWI,
            "temperature" => Some(self.get_array(|o| o.temperature)),
            "rain" => Some(self.get_array(|o| o.rain)),
            "windSpeed" => Some(self.get_array(|o| o.wind_speed)),
            "windDir" => Some(self.get_array(|o| o.wind_dir)),
            "humidity" => Some(self.get_array(|o| o.humidity)),
            "snowCover" => Some(self.get_array(|o| o.snow_cover)),
            "NDVI" => Some(self.get_array(|o| o.NDVI)),
            "NDWI" => Some(self.get_array(|o| o.NDVI)),

            // //Derived variables
            "VPPF" => Some(self.get_array(|o| get_derived(&o.V, &o.PPF, None))),
            "IPPF" => Some(self.get_array(|o| get_derived(&o.I, &o.PPF, None))),

            "INDWI" => Some(self.get_array(|o| get_derived(&o.I, &o.NDWI, None))),
            "VNDWI" => Some(self.get_array(|o| get_derived(&o.V, &o.NDWI, None))),
            "INDVI" => Some(self.get_array(|o| get_derived(&o.I, &o.NDVI, None))),
            "VNDVI" => Some(self.get_array(|o| get_derived(&o.V, &o.NDVI, None))),

            "VPPFNDWI" => Some(self.get_array(|o| get_derived(&o.I, &o.NDWI, Some(&o.PPF)))),
            "IPPFNDWI" => Some(self.get_array(|o| get_derived(&o.V, &o.NDWI, Some(&o.PPF)))),
            "VPPFNDVI" => Some(self.get_array(|o| get_derived(&o.I, &o.NDVI, Some(&o.PPF)))),
            "IPPFNDVI" => Some(self.get_array(|o| get_derived(&o.V, &o.NDVI, Some(&o.PPF)))),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct InputElement {
    /// temperature in celsius
    pub temperature: f32,
    /// rain in mm
    pub rain: f32,
    /// wind speed in m/h
    pub wind_speed: f32,
    /// wind direction in radians
    pub wind_dir: f32,
    /// relative humidity in %
    pub humidity: f32,
    /// snow cover
    pub snow_cover: f32,

    // satellite variables
    pub ndvi: f32,
    pub ndwi: f32,
    pub msi: f32,
    pub swi: f32,
}

impl Default for InputElement {
    fn default() -> Self {
        Self {
            temperature: NODATAVAL,
            rain: NODATAVAL,
            wind_speed: NODATAVAL,
            wind_dir: NODATAVAL,
            humidity: NODATAVAL,
            snow_cover: NODATAVAL,
            ndvi: NODATAVAL,
            ndwi: NODATAVAL,
            msi: NODATAVAL,
            swi: NODATAVAL,
        }
    }
}

pub struct Input {
    pub time: DateTime<Utc>,
    pub data: Array1<InputElement>,
}

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct StateElement {
    pub dffm: f32,
    pub snow_cover: f32,
    pub snow_cover_time: f32,
    pub MSI: f32,
    pub MSI_TTL: f32,
    pub NDVI: f32,
    pub NDVI_TIME: f32,
    pub NDWI: f32,
    pub NDWI_TIME: f32,
}

#[derive(Debug)]
pub struct State {
    pub time: DateTime<Utc>,
    pub data: Array1<StateElement>,

    len: usize,

    config: ModelConfig,
}

impl State {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &Vec<WarmState>, time: &DateTime<Utc>, config: ModelConfig) -> State {
        let data = Array1::from_vec(
            warm_state
                .iter()
                .map(|w| StateElement {
                    dffm: w.dffm,
                    snow_cover: w.snow_cover,
                    snow_cover_time: w.snow_cover_time,
                    MSI: w.MSI,
                    MSI_TTL: w.MSI_TTL,
                    NDVI: w.NDVI,
                    NDVI_TIME: w.NDVI_TIME,
                    NDWI: w.NDWI,
                    NDWI_TIME: w.NDWI_TIME,
                })
                .collect(),
        );

        State {
            time: time.clone(),
            // props,
            data,
            len: warm_state.len(),
            config: config,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    fn update_snow_cover(&mut self, input: &Input) {
        let time = input.time.timestamp() as f32;

        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input| {
                let i_snow_cover = input.snow_cover;

                if i_snow_cover == NODATAVAL {
                    if (time - state.snow_cover_time) as i64 > SNOW_SECONDS_VALIDITY {
                        state.snow_cover = NODATAVAL;
                    }
                    return;
                }

                state.snow_cover = i_snow_cover;
                state.snow_cover_time = time;
            });
    }

    fn update_satellite(&mut self, input: &Input) {
        let time = input.time.timestamp() as f32;
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input| {
                let i_msi = input.msi;

                if i_msi < 0.0 || i_msi > 1.0 {
                    if state.MSI > 0.0 {
                        state.MSI_TTL -= 1.0;
                    } else {
                        state.MSI = NODATAVAL;
                    }
                } else {
                    state.MSI = i_msi;
                    state.MSI_TTL = 56.0;
                }
            });
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input| {
                let i_ndvi = input.ndvi;

                if self.time.timestamp() - state.NDVI_TIME as i64 > SATELLITE_DATA_SECONDS_VALIDITY
                {
                    state.NDVI = NODATAVAL;
                }

                if i_ndvi != NODATAVAL {
                    if i_ndvi >= 0.0 && i_ndvi <= 1.0 {
                        state.NDVI = i_ndvi;
                    } else {
                        state.NDVI = NODATAVAL;
                    }

                    state.NDVI_TIME = time;
                }
            });
        Zip::from(&mut self.data)
            .and(&input.data)
            .par_for_each(|state, input| {
                let i_ndwi = input.ndwi;

                if self.time.timestamp() - state.NDWI_TIME as i64 > SATELLITE_DATA_SECONDS_VALIDITY
                {
                    state.NDWI = NODATAVAL;
                }

                if self.time.timestamp() - state.NDWI_TIME as i64 > 240 * 3600 {
                    state.NDWI = NODATAVAL;
                }

                if i_ndwi != NODATAVAL {
                    if i_ndwi >= 0.0 && i_ndwi <= 1.0 {
                        state.NDWI = i_ndwi;
                    } else {
                        state.NDWI = NODATAVAL;
                    }
                    state.NDWI_TIME = time;
                }
            });
    }

    #[allow(non_snake_case)]
    fn update_moisture(&mut self, props: &Properties, input: &Input, dt: f32) {
        let dt = f32::max(1.0, f32::min(72.0, dt));

        Zip::from(&mut self.data)
            // .and(&self.snow_cover)
            .and(&props.data)
            .and(&input.data)
            .par_for_each(|state, props, input_data| {
                update_moisture_fn(state, props, input_data, &self.config, dt)
            });
    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &State, props: &Properties, input: &Input) -> Output {
        let time = &self.time;

        let output_data = Zip::from(&self.data)
            .and(&props.data)
            .and(&input.data)
            .par_map_collect(|state, props, input| {
                get_output_fn(state, props, input, &self.config, time)
            });

        Output::new(time.clone(), output_data)
    }

    /// Update the state of the cells.
    pub fn update(&mut self, props: &Properties, input: &Input) {
        let new_time = &input.time;
        let dt = new_time.signed_duration_since(self.time).num_seconds() as f32 / 3600.0;
        self.time = new_time.clone();
        self.update_satellite(input);
        self.update_snow_cover(input);
        self.update_moisture(props, input, dt);
    }

    pub fn output(&self, props: &Properties, input: &Input) -> Output {
        self.get_output(props, input)
    }
}
