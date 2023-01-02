use std::{collections::HashMap, rc::Rc, f32::NAN, sync::Arc};

use chrono::prelude::*;
use ndarray::{azip, Array1};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};

use crate::library::{state::{functions::{get_v0, get_wind_effect, get_slope_effect, get_t_effect, get_ppf, get_v, update_dffm_rain, update_dffm_dry, get_lhv_dff, get_lhv_l1, get_intensity}, constants::{NODATAVAL, SNOW_COVER_THRESHOLD, MAXRAIN}}, config::models::WarmState};



const UPDATE_TIME: i64 = 100;

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

        let lats = Array1::from_vec(lats.clone());
        let lons = Array1::from_vec(lons.clone());
        let slopes = Array1::from_vec(slopes.clone());
        let aspects = Array1::from_vec(aspects.clone());
        let ppf_summer = Array1::from_vec(ppf_summer.clone());
        let ppf_winter = Array1::from_vec(ppf_winter.clone());

        let vegetations = vegetations
            .iter()
            .map(|v| vegetations_dict.get(v).unwrap().clone())
            .collect::<Array1<_>>();

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
            "INDWI" => Some((&self.I * &self.NDWI).clone()),
            "VNDWI" => Some((&self.V * &self.NDWI).clone()),
            "VPPFNDWI" => Some((&self.V * &self.PPF * &self.NDWI).clone()),
            "IPPFNDWI" => Some((&self.I * &self.PPF * &self.NDWI).clone()),
            "VPPF" => Some((&self.V * &self.PPF).clone()),
            "IPPF" => Some((&self.I * &self.PPF).clone()),
            "INDVI" => Some((&self.I * &self.NDVI).clone()),
            "VNDVI" => Some((&self.V * &self.NDVI).clone()),
            "VPPFNDVI" => Some((&self.V * &self.PPF * &self.NDVI).clone()),
            "IPPFNDVI" => Some((&self.I * &self.PPF * &self.NDVI).clone()),
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
    pub ndsi: Array1<f32>,
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

    pub NDSI:  Array1<f32>,
    pub NDSI_TTL: Array1<f32>,
    pub MSI: Array1<f32>,
    pub MSI_TTL: Array1<f32>,
    pub NDVI: Array1<f32>,
    pub NDVI_TIME: Array1<f32>,
    pub NDWI: Array1<f32>,
    pub NDWI_TIME: Array1<f32>,

}

impl State {
    #[allow(dead_code, non_snake_case)]
    /// Create a new state.
    pub fn new(
        warm_state: &Vec<WarmState>,
        time: &DateTime<Utc>,
    ) -> State {
        let dffm = Array1::from_vec(warm_state.iter().map(|w| w.dffm).collect());
        let NDSI = Array1::from_vec(warm_state.iter().map(|w| w.NDSI).collect());
        let NDSI_TTL = Array1::from_vec(warm_state.iter().map(|w| w.NDSI_TTL).collect());
        let MSI = Array1::from_vec(warm_state.iter().map(|w| w.MSI).collect());
        let MSI_TTL = Array1::from_vec(warm_state.iter().map(|w| w.MSI_TTL).collect());
        let NDVI = Array1::from_vec(warm_state.iter().map(|w| w.NDVI).collect());
        let NDVI_TIME = Array1::from_vec(warm_state.iter().map(|w| w.NDVI_TIME).collect());
        let NDWI = Array1::from_vec(warm_state.iter().map(|w| w.NDWI).collect());
        let NDWI_TIME = Array1::from_vec(warm_state.iter().map(|w| w.NDWI_TIME).collect());
        let snow_cover = Array1::zeros(warm_state.len());
        State {
            time: time.clone(),
            // props,
            dffm,
            NDSI_TTL,
            NDSI,
            MSI_TTL,
            MSI,
            NDVI,
            NDVI_TIME,
            NDWI,
            NDWI_TIME,
            snow_cover,
        }
    }

    fn update_snow_cover(&mut self, input: &Input) {
        azip!((
            snow_cover in &mut self.snow_cover,
            ndsi in &self.NDSI,
            i_snow_cover in &input.snow_cover,
        ){
            *snow_cover = *i_snow_cover;
            if *ndsi != NODATAVAL {
                *snow_cover = 1.0;
            }
        });
    }

    fn update_satellite(&mut self, input: &Input) {
        azip!((
            ndsi in &mut self.NDSI,
            ndsi_ttl in &mut self.NDSI_TTL,
            i_ndsi in &input.ndsi,            
        ){
            if *i_ndsi < 0.0 {
                if *ndsi_ttl > 0.0 {
                    *ndsi_ttl -= 1.0;
                } else {
                    *ndsi = NODATAVAL;                    
                }
            } else {
                *ndsi = *i_ndsi;
                *ndsi_ttl = 56.0;
            }
        });
        azip!((
            msi in &mut self.MSI,
            msi_ttl in &mut self.MSI_TTL,
            i_msi in &input.msi,            
        ){
            if *i_msi < 0.0 || *i_msi > 1.0 {
                if *msi_ttl > 0.0 {
                    *msi_ttl -= 1.0;
                } else {
                    *msi = NODATAVAL;                    
                }
            } else {
                *msi = *i_msi;
                *msi_ttl = 56.0;
            }
        });
        
        azip!((
            ndvi in &mut self.NDVI,
            ndvi_time in &mut self.NDVI_TIME,
            i_ndvi in &input.ndvi,            
        ){
            if *i_ndvi < 0.0 || *i_ndvi > 1.0 {
                let time_diff = input.time.timestamp() - *ndvi_time as i64;
                if time_diff > 240 * 3600 {
                    *ndvi = NODATAVAL;                    
                }
            } else {
                if *i_ndvi<0.0 || *i_ndvi>1.0 {
                    *ndvi = NODATAVAL;
                } else {                    
                    *ndvi = *i_ndvi;
                    *ndvi_time = input.time.timestamp() as f32;
                }
            }
        });

        
        azip!((
            ndwi in &mut self.NDWI,
            ndwi_time in &mut self.NDWI_TIME,
            i_ndwi in &input.ndwi,            
        ){
            if *i_ndwi < 0.0 || *i_ndwi > 1.0 {
                let time_diff = input.time.timestamp() - *ndwi_time as i64;
                if time_diff > 240 * 3600 {
                    *ndwi = NODATAVAL;
                }
            } else {
                if *i_ndwi<0.0 || *i_ndwi>1.0 {
                    *ndwi = NODATAVAL;
                } else {                    
                    *ndwi = *i_ndwi;
                    *ndwi_time = input.time.timestamp() as f32;
                }
            }
        });

    

    }

    #[allow(non_snake_case)]
    fn update_moisture(&mut self, props: &Properties, input: &Input, dt: f32) {
        let dt = f32::max(1.0, f32::min(72.0, dt));
        
        let dffm = (0..self.dffm.len()).into_par_iter().map(|i| {
            let mut dffm = self.dffm[i];

            let snow_cover  = self.snow_cover[i];            
            let veg = &props.vegetations[i];
            let temperature = input.temperature[i];
            let humidity = input.humidity[i];
            let wind_speed = input.wind_speed[i];
            let rain = input.rain[i];
            
            let d0 = veg.d0;
            let sat = veg.sat;
            
            let T0 = veg.T0;
            if d0 == NODATAVAL {
                return NODATAVAL;
            }
            else if snow_cover > SNOW_COVER_THRESHOLD{
                return sat
            }

            else if dffm == NODATAVAL || temperature == NODATAVAL || humidity == NODATAVAL{
                
                return NODATAVAL;
            }
            
            let t = if temperature > 0.0  { temperature }  else  {0.0};

            let h = if humidity < 100.0 { humidity } else { 100.0 };
            let w = if wind_speed != NODATAVAL { wind_speed } else { 0.0 };
            let r = if rain != NODATAVAL { rain } else { 0.0 };

            
            
            if r > MAXRAIN {
                dffm = update_dffm_rain(r, dffm, sat);
            }else{
                dffm = update_dffm_dry(dffm, sat, t, w, h, T0, dt)
            }
            dffm
        }).collect::<Vec<f32>>();
        self.dffm = Array1::from(dffm);

    }

    #[allow(non_snake_case)]
    pub fn get_output(self: &State, props: &Properties, input: &Input) -> Output {
        let time = &self.time;
        // if dffm == NODATAVAL || temperature == NODATAVAL	{
        // 	// return NODATAVAL;
        // }
        let len = props.lats.len();

        let snow_cover = &self.snow_cover;
        let mut w_effect = Array1::<f32>::zeros(len);
        let mut V0 = Array1::<f32>::zeros(len);
        let mut t_effect = Array1::<f32>::ones(len);
        let mut slope_effect = Array1::<f32>::ones(len);
        let mut V = Array1::<f32>::zeros(len);
        let mut PPF = Array1::<f32>::zeros(len);
        let mut I = Array1::<f32>::ones(len) * NAN;

        let vegs = &props.vegetations;
        

        azip!((
                V0 in &mut V0,
                &dffm in &self.dffm,
                veg in vegs,
                &snow_cover in snow_cover,
            ){
            *V0 = get_v0(veg.v0, veg.d0, veg.d1, dffm, snow_cover);
        });

        azip!((
                w_effect in &mut w_effect,
                slope_effect in &mut slope_effect,
                &wind_dir in &input.wind_dir,
                &wind_speed in &input.wind_speed,
                &slope in &props.slopes,
                &aspect in &props.aspects,
            ){
            *w_effect = get_wind_effect(wind_speed, wind_dir, slope, aspect);
            *slope_effect = get_slope_effect(slope);
        });

        let use_t_effect = false;
        if use_t_effect {
            azip!((
                t_effect in &mut t_effect,
                &temperature in &input.temperature,
            ){
                *t_effect = get_t_effect(temperature);
            });
        }
        azip!((
                ppf in &mut PPF,
                &ppf_summer in &props.ppf_summer,
                &ppf_winter in &props.ppf_winter,
            ){
            *ppf = get_ppf(time, ppf_summer, ppf_winter);
        });

        azip!((
            V in &mut V,
            &V0 in &V0,
            &w_effect in &w_effect,
            &slope_effect in &slope_effect,
            &t_effect in &t_effect,
            ){
            *V = get_v(V0, w_effect, slope_effect, t_effect);
        });

        azip!((
            I in &mut I,
            &V in &V,
            &dffm in &self.dffm,
            &msi in &self.MSI,
            &ndvi in &self.NDVI,
            veg in &props.vegetations,
            ){

            if veg.hhv == NODATAVAL || dffm == NODATAVAL {
                *I = NODATAVAL;
            }else{
				let LHVdff = get_lhv_dff(veg.hhv, dffm);
				// calcolo LHV per la vegetazione viva
				
				let LHVl1 = get_lhv_l1(veg.umid, msi, veg.hhv);
                // Calcolo Intensità
				
				*I = get_intensity(veg.d0, veg.d1, V, ndvi, LHVdff, LHVl1);
			}

        });


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
            input.wind_dir.clone(),
            input.wind_speed.clone(),
            input.humidity.clone(),
            self.snow_cover.clone(),
            self.NDVI.clone(),
            self.NDWI.clone(),
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
