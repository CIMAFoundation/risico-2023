use std::{collections::HashMap, sync::Arc};

use chrono::prelude::*;
use ndarray::{azip, Array1};

use crate::library::{
    config::models::WarmState,
    state::{
        constants::{MAXRAIN, NODATAVAL, SNOW_COVER_THRESHOLD, SNOW_SECONDS_VALIDITY},
        functions::{
            get_intensity, get_lhv_dff, get_lhv_l1, get_ppf, get_slope_effect, get_t_effect, get_v,
            get_v0, get_wind_effect, update_dffm_dry, update_dffm_rain,
        },
    },
};

use super::constants::SATELLITE_DATA_SECONDS_VALIDITY;

//const UPDATE_TIME: i64 = 100;

fn get_derived(a: &Array1<f32>, b: &Array1<f32>, c: Option<&Array1<f32>>) -> Array1<f32> {
    let mut r = Array1::ones(a.len()) * NODATAVAL;
    for i in 0..a.len() {
        if b[i] != NODATAVAL {
            r[i] = a[i] * b[i];
        } else {
            r[i] = a[i];
        }
    }
    if let Some(c) = c {
        for i in 0..a.len() {
            if c[i] != NODATAVAL {
                r[i] = r[i] * c[i];
            }
        }
    }
    r
}

#[derive(Debug)]
pub struct Properties {
    pub lons: Array1<f32>,
    pub lats: Array1<f32>,
    pub slopes: Array1<f32>,
    pub aspects: Array1<f32>,

    pub ppf_summer: Array1<f32>,
    pub ppf_winter: Array1<f32>,

    pub vegetations: Array1<Arc<Vegetation>>,
    pub vegetations_dict: HashMap<String, Arc<Vegetation>>,
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

        let lats = Array1::from_vec(lats);
        let lons = Array1::from_vec(lons);
        let slopes = Array1::from_vec(slopes);
        let aspects = Array1::from_vec(aspects);
        let ppf_summer = Array1::from_vec(ppf_summer);
        let ppf_winter = Array1::from_vec(ppf_winter);

        let default_veg = Arc::new(Vegetation::default());

        let vegetations = vegetations
            .iter()
            .map(|v| vegetations_dict.get(v).unwrap_or(&default_veg).clone())
            .collect();

        Self {
            lons,
            lats,
            slopes,
            aspects,
            ppf_summer,
            ppf_winter,
            vegetations,
            vegetations_dict,
        }
    }

    pub fn len(&self) -> usize {
        self.lons.len()
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
pub struct Output {
    pub time: DateTime<Utc>,

    pub dffm: Array1<f32>,
    pub W: Array1<f32>,
    pub V: Array1<f32>,
    pub I: Array1<f32>,
    pub NDVI: Array1<f32>,
    pub NDWI: Array1<f32>,
    pub PPF: Array1<f32>,
    pub t_effect: Array1<f32>,
    // pub SWI: Array1<f32>,
    pub temperature: Array1<f32>,
    pub rain: Array1<f32>,
    pub wind_speed: Array1<f32>,
    pub wind_dir: Array1<f32>,
    pub humidity: Array1<f32>,
    pub snow_cover: Array1<f32>,
}

#[allow(non_snake_case)]
impl Output {
    pub fn new(
        time: DateTime<Utc>,
        dffm: Array1<f32>,
        W: Array1<f32>,
        V: Array1<f32>,
        I: Array1<f32>,
        PPF: Array1<f32>,
        t_effect: Array1<f32>,
        temperature: Array1<f32>,
        rain: Array1<f32>,
        wind_speed: Array1<f32>,
        wind_dir: Array1<f32>,
        humidity: Array1<f32>,
        snow_cover: Array1<f32>,
        NDVI: Array1<f32>,
        NDWI: Array1<f32>,
    ) -> Self {
        Self {
            time,
            dffm,
            W,
            V,
            I,
            PPF,
            t_effect,
            temperature,
            rain,
            wind_dir,
            wind_speed,
            humidity,
            snow_cover,
            NDVI,
            NDWI,
        }
    }

    pub fn get(&self, variable: &str) -> Option<Array1<f32>> {
        match variable {
            // Output variables
            "dffm" => Some(self.dffm.clone()),
            "W" => Some(self.W.clone()),
            "V" => Some(self.V.clone()),
            "I" => Some(self.I.clone()),

            "contrT" => Some(self.t_effect.clone()),
            // "SWI" => self.SWI,
            "temperature" => Some(self.temperature.clone()),
            "rain" => Some(self.rain.clone()),
            "windSpeed" => Some(self.wind_speed.clone()),
            "windDir" => Some(self.wind_dir.clone()),
            "humidity" => Some(self.humidity.clone()),
            "snowCover" => Some(self.snow_cover.clone()),
            "NDVI" => Some(self.NDVI.clone()),
            "NDWI" => Some(self.NDWI.clone()),
            //Derived variables
            "VPPF" => Some(get_derived(&self.V, &self.PPF, None)),
            "IPPF" => Some(get_derived(&self.I, &self.PPF, None)),

            "INDWI" => Some(get_derived(&self.I, &self.NDWI, None)),
            "VNDWI" => Some(get_derived(&self.V, &self.NDWI, None)),
            "INDVI" => Some(get_derived(&self.I, &self.NDVI, None)),
            "VNDVI" => Some(get_derived(&self.V, &self.NDVI, None)),

            "VPPFNDWI" => Some(get_derived(&self.V, &self.NDWI, Some(&self.PPF))),
            "IPPFNDWI" => Some(get_derived(&self.I, &self.NDWI, Some(&self.PPF))),
            "VPPFNDVI" => Some(get_derived(&self.V, &self.NDVI, Some(&self.PPF))),
            "IPPFNDVI" => Some(get_derived(&self.I, &self.NDVI, Some(&self.PPF))),

            _ => None,
        }
    }
}

pub struct Input {
    pub time: DateTime<Utc>,
    pub temperature: Array1<f32>,
    pub rain: Array1<f32>,
    pub wind_speed: Array1<f32>,
    pub wind_dir: Array1<f32>,
    pub humidity: Array1<f32>,
    pub snow_cover: Array1<f32>,
    // satellite variables
    //[TODO] refactor this!!!
    pub ndvi: Array1<f32>,
    pub ndwi: Array1<f32>,
    pub msi: Array1<f32>,
    pub swi: Array1<f32>,
}

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct State {
    pub time: DateTime<Utc>,
    // pub props: &'a Properties ,

    // state
    pub dffm: Array1<f32>,
    pub snow_cover: Array1<f32>,
    pub snow_cover_time: Array1<f32>,

    pub MSI: Array1<f32>,
    pub MSI_TTL: Array1<f32>,
    pub NDVI: Array1<f32>,
    pub NDVI_TIME: Array1<f32>,
    pub NDWI: Array1<f32>,
    pub NDWI_TIME: Array1<f32>,

    len: usize,
}

impl State {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(warm_state: &Vec<WarmState>, time: &DateTime<Utc>) -> State {
        let dffm = Array1::from_vec(warm_state.iter().map(|w| w.dffm).collect());
        let MSI = Array1::from_vec(warm_state.iter().map(|w| w.MSI).collect());
        let MSI_TTL = Array1::from_vec(warm_state.iter().map(|w| w.MSI_TTL).collect());
        let NDVI = Array1::from_vec(warm_state.iter().map(|w| w.NDVI).collect());
        let NDVI_TIME = Array1::from_vec(warm_state.iter().map(|w| w.NDVI_TIME).collect());
        let NDWI = Array1::from_vec(warm_state.iter().map(|w| w.NDWI).collect());
        let NDWI_TIME = Array1::from_vec(warm_state.iter().map(|w| w.NDWI_TIME).collect());

        let snow_cover = Array1::zeros(warm_state.len());
        let snow_cover_time = Array1::zeros(warm_state.len());

        State {
            time: time.clone(),
            // props,
            dffm,
            snow_cover,
            snow_cover_time,
            MSI_TTL,
            MSI,
            NDVI,
            NDVI_TIME,
            NDWI,
            NDWI_TIME,
            len: warm_state.len(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    fn update_snow_cover(&mut self, input: &Input) {
        azip!((
            snow_cover in &mut self.snow_cover,
            snow_cover_time in &mut self.snow_cover_time,
            i_snow_cover in &input.snow_cover,
        ){
            let time = input.time.timestamp() as f32;
            
            if *i_snow_cover == NODATAVAL {
                if (time - *snow_cover_time) as i64 > SNOW_SECONDS_VALIDITY {
                    *snow_cover = NODATAVAL;
                }
                return;
            }
            
            *snow_cover = *i_snow_cover;
            *snow_cover_time = time;
        });
    }

    fn update_satellite(&mut self, input: &Input) {
        for idx in 0..self.len() {
            {
                let msi = &mut self.MSI[idx];
                let msi_ttl = &mut self.MSI_TTL[idx];
                let i_msi = input.msi[idx];

                if i_msi < 0.0 || i_msi > 1.0 {
                    if *msi_ttl > 0.0 {
                        *msi_ttl -= 1.0;
                    } else {
                        *msi = NODATAVAL;
                    }
                } else {
                    *msi = i_msi;
                    *msi_ttl = 56.0;
                }
            }
            {
                let ndvi = &mut self.NDVI[idx];
                let ndvi_time = &mut self.NDVI_TIME[idx];
                let i_ndvi = input.ndvi[idx];

                if self.time.timestamp() - *ndvi_time as i64 > SATELLITE_DATA_SECONDS_VALIDITY {
                    *ndvi = NODATAVAL;
                }

                if i_ndvi != NODATAVAL {
                    if i_ndvi >= 0.0 && i_ndvi <= 1.0 {
                        *ndvi = i_ndvi;
                    } else {
                        *ndvi = NODATAVAL;
                    }

                    *ndvi_time = input.time.timestamp() as f32;
                }
            }
            {
                let ndwi = &mut self.NDWI[idx];
                let ndwi_time = &mut self.NDWI_TIME[idx];
                let i_ndwi = input.ndwi[idx];

                if self.time.timestamp() - *ndwi_time as i64 > SATELLITE_DATA_SECONDS_VALIDITY {
                    *ndwi = NODATAVAL;
                }


                if self.time.timestamp() - *ndwi_time as i64 > 240 * 3600 {
                    *ndwi = NODATAVAL;
                }

                if i_ndwi != NODATAVAL {
                    if i_ndwi >= 0.0 && i_ndwi <= 1.0 {
                        *ndwi = i_ndwi;
                        
                    } else {
                        *ndwi = NODATAVAL;
                    }
                    *ndwi_time = input.time.timestamp() as f32;                 
                }
            }
        }
    }

    #[allow(non_snake_case)]
    fn update_moisture(&mut self, props: &Properties, input: &Input, dt: f32) {
        let dt = f32::max(1.0, f32::min(72.0, dt));

        for idx in 0..self.len() {
            let dffm = &mut self.dffm[idx];

            let snow_cover = self.snow_cover[idx];
            let veg = &props.vegetations[idx];
            let temperature = input.temperature[idx];
            let humidity = input.humidity[idx];
            let wind_speed = input.wind_speed[idx];
            let rain = input.rain[idx];

            let d0 = veg.d0;
            let sat = veg.sat;

            let T0 = veg.T0;

            if d0 == NODATAVAL {
                *dffm = NODATAVAL;
                continue;
            } else if snow_cover > SNOW_COVER_THRESHOLD {
                *dffm = sat;
                continue;
            } else if temperature == NODATAVAL || humidity == NODATAVAL {
                // keep current humidity if we don't have all the data
                continue;
            }

            let t = if temperature > 0.0 { temperature } else { 0.0 };

            let h = if humidity <= 100.0 { humidity } else { 100.0 };
            let w = if wind_speed != NODATAVAL {
                wind_speed
            } else {
                0.0
            };
            let r = if rain != NODATAVAL { rain } else { 0.0 };

            if r > MAXRAIN {
                *dffm = update_dffm_rain(r, *dffm, sat);
            } else {
                *dffm = update_dffm_dry(*dffm, sat, t, w, h, T0, dt)
            }

            // limit dffm to [0, sat]

            *dffm = f32::max(0.0, f32::min(sat, *dffm));
        }
    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &State, props: &Properties, input: &Input) -> Output {
        let time = &self.time;

        let mut w_effect = Array1::<f32>::ones(self.len) * NODATAVAL;
        let mut t_effect = Array1::<f32>::ones(self.len);
        let mut V = Array1::<f32>::ones(self.len) * NODATAVAL;
        let mut PPF = Array1::<f32>::ones(self.len) * NODATAVAL;
        let mut I = Array1::<f32>::ones(self.len) * NODATAVAL;
        let mut NDVI = Array1::<f32>::ones(self.len) * NODATAVAL;
        let mut NDWI = Array1::<f32>::ones(self.len) * NODATAVAL;

        let use_t_effect = false;

        for idx in 0..self.len {
            let dffm = self.dffm[idx];

            let t_effect = &mut t_effect[idx];
            let w_effect = &mut w_effect[idx];

            let ppf = &mut PPF[idx];
            let V = &mut V[idx];
            let I = &mut I[idx];

            let veg = &props.vegetations[idx];

            let wind_dir = input.wind_dir[idx];
            let wind_speed = input.wind_speed[idx];
            let slope = props.slopes[idx];
            let aspect = props.aspects[idx];

            let temperature = input.temperature[idx];
            let snow_cover = self.snow_cover[idx];

            let ppf_summer = props.ppf_summer[idx];
            let ppf_winter = props.ppf_winter[idx];
            let msi = self.MSI[idx];
            let ndvi = &mut NDVI[idx];
            let ndwi = &mut NDWI[idx];

            *ndvi = 1.0;
            if veg.use_ndvi && self.NDVI[idx] != NODATAVAL {
                *ndvi = f32::max(f32::min(1.0 - self.NDVI[idx], 1.0), 0.0);
            }
            
            *ndwi = 1.0;
            if self.NDWI[idx] != NODATAVAL {
                *ndwi = f32::max(f32::min(1.0 - self.NDWI[idx], 1.0), 0.0);
            }

            *w_effect = get_wind_effect(wind_speed, wind_dir, slope, aspect);
            let slope_effect = get_slope_effect(slope);

            *t_effect = 1.0;
            if use_t_effect {
                *t_effect = get_t_effect(temperature);
            }

            if dffm == NODATAVAL {
                continue;
            }
            let V0 = get_v0(veg.v0, veg.d0, veg.d1, dffm, snow_cover);
            *ppf = get_ppf(time, ppf_summer, ppf_winter);
            *V = get_v(V0, *w_effect, slope_effect, *t_effect);

            if veg.hhv == NODATAVAL || dffm == NODATAVAL {
                *I = NODATAVAL;
                continue;
            }
            let LHVdff = get_lhv_dff(veg.hhv, dffm);
            // calcolo LHV per la vegetazione viva

            let LHVl1 = get_lhv_l1(veg.umid, msi, veg.hhv);
            // Calcolo Intensità

            *I = get_intensity(veg.d0, veg.d1, *V, self.NDVI[idx], LHVdff, LHVl1);
        }

        Output::new(
            time.clone(),
            self.dffm.clone(),
            w_effect,
            V,
            I,
            PPF,
            t_effect,
            input.temperature.clone(),
            input.rain.clone(),
            input.wind_speed.clone(),
            input.wind_dir.clone(),
            input.humidity.clone(),
            self.snow_cover.clone(),
            NDVI,
            NDWI,
        )
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
