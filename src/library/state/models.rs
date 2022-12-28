use std::{collections::HashMap, hash::Hash, rc::Rc};

use chrono::prelude::*;
use ndarray::Array1;

use super::functions::{get_output, update_moisture};

const UPDATE_TIME: i64 = 100;

#[derive(Debug)]
pub struct Properties {
    pub lons: Array1<f32>,
    pub lats: Array1<f32>,
    pub slopes: Array1<f32>,
    pub aspects: Array1<f32>,

    pub ppf_summer: Array1<f32>,
    pub ppf_winter: Array1<f32>,

    pub vegetations: Array1<Rc<Vegetation>>,
    pub vegetations_dict: HashMap<String, Rc<Vegetation>>,
}

impl  Properties {
    pub fn new (
        lats: Vec<f32>,
        lons: Vec<f32>,
        slopes: Vec<f32>,
        aspects: Vec<f32>,
        vegetations: Vec<String>,
        vegetations_dict: HashMap<String, Rc<Vegetation>>,
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

        let vegetations = vegetations.iter().map(
            |v| vegetations_dict.get(v).unwrap().clone()
        ).collect::<Array1<_>>();


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

pub struct Output {
    pub time: DateTime<Utc>,

    pub dffm: Array1<f32>,
    pub W: Array1<f32>,
    pub V: Array1<f32>,
    pub I: Array1<f32>,
    // pub NDVI: Array1<f32>,
    // pub NDWI: Array1<f32>,
    pub PPF: Array1<f32>,
    pub t_effect: Array1<f32>,
    // pub SWI: Array1<f32>,
    pub temperature: Array1<f32>,
    pub rain: Array1<f32>,
    pub wind_speed: Array1<f32>,
    pub wind_dir: Array1<f32>,
    pub humidity: Array1<f32>,
    // pub snow_cover: Array1<f32>,
    derived: HashMap<String, Array1<f32>>,
}

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
            wind_speed,
            wind_dir,
            humidity,
            derived: HashMap::new(),
        }
    }

    pub fn get(&self, variable: &str) -> Array1<f32> {
        match variable {
            // Output variables
            "dffm" => self.dffm.clone(),
            "W" => self.W.clone(),
            "V" => self.V.clone(),
            "I" => self.I.clone(),

            "contrT" => self.t_effect.clone(),
            // "SWI" => self.SWI,
            "temperature" => self.temperature.clone(),
            "rain" => self.rain.clone(),
            "windSpeed" => self.wind_speed.clone(),
            "windDir" => self.wind_dir.clone(),
            "humidity" => self.humidity.clone(),
            // "snowCover" => self.snow_cover.as_slice().unwrap(),
            // "NDVI" => self.NDVI,
            // "NDWI" => self.NDWI,
            // Derived variables
            // "INDWI" => self.I * out.NDWI,
            // "VNDWI" => self.V * out.NDWI,
            // "VPPFNDWI" => self.V * out.PPF * out.NDWI,
            // "IPPFNDWI" => self.I * out.PPF * out.NDWI,
            "VPPF" => (&self.V * &self.PPF).clone(),
            "IPPF" => (&self.I * &self.PPF).clone(),
            // "INDVI" => self.I * out.NDVI,
            // "VNDVI" => self.V * out.NDVI,
            // "VPPFNDVI" => self.V * out.PPF * out.NDVI,
            // "IPPFNDVI" => self.I * out.PPF * out.NDVI,
            _ => panic!("Unknown variable: {}", variable),
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
    pub ndvi: Array1<f32>,
    pub ndwi: Array1<f32>,
}

#[derive(Debug)]
pub struct State {
    pub time: DateTime<Utc>,
    // pub props: &'a Properties ,

    // state
    pub dffm: Array1<f32>,
    // pub snow_cover: Array1<f32>,
}

impl State {
    /// Create a new state.
    pub fn new(
        // props: &Properties,
        // state
        dffm: Array1<f32>,
        snow_cover: Array1<f32>,

        time: DateTime<Utc>,
    ) -> State {
        State {
            time,
            // props,            
            dffm,
            // snow_cover,
        }
    }

    /// Update the state of the cells.
    pub fn update(&mut self, props: &Properties, input: &Input, new_time: &DateTime<Utc>) {
        let dt = 3600.0;
        let new_dffm = update_moisture(self, props, input, dt);

        self.dffm = new_dffm;
    }

    pub fn output(&self, props: &Properties, input: &Input) -> Output {
        get_output(self, props, input)
    }

}
