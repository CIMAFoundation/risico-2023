use chrono::prelude::*;
use ndarray::Array1;

use super::functions::update_moisture;

const UPDATE_TIME: i64 = 100;
#[derive(Debug)]
pub struct Vegetation {
    pub id: String,	
    pub d0: f32,
    pub d1: f32,
    pub hhv: f32,	
    pub umid: f32,
    pub v0: f32,
    pub T0: f32,
    pub	sat: f32,
    pub name: String
}

pub struct Output{
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
	pub snow_cover: Array1<f32>,
}


impl Output {
    pub fn get(&self, variable: &str) -> &[f32]{
        match variable {
            // Output variables
            "dffm" => self.dffm.as_slice().unwrap(),
            "W" => self.W.as_slice().unwrap(),
            "V" => self.V.as_slice().unwrap(),
            "I" => self.I.as_slice().unwrap(),
            
            
            "contrT" => self.t_effect.as_slice().unwrap(),
            // "SWI" => self.SWI,
            "temperature" => self.temperature.as_slice().unwrap(),
            "rain" => self.rain.as_slice().unwrap(),
            "windSpeed" => self.wind_speed.as_slice().unwrap(),
            "windDir" => self.wind_dir.as_slice().unwrap(),
            "humidity" => self.humidity.as_slice().unwrap(),
            "snowCover" => self.snow_cover.as_slice().unwrap(),
            // "NDVI" => self.NDVI,
            // "NDWI" => self.NDWI,
            // Derived variables
            // "INDWI" => self.I * out.NDWI,
            // "VNDWI" => self.V * out.NDWI,
            // "VPPFNDWI" => self.V * out.PPF * out.NDWI,
            // "IPPFNDWI" => self.I * out.PPF * out.NDWI,
            "VPPF" => (self.V * self.PPF).as_slice().unwrap(),
            "IPPF" => (self.I * self.PPF).as_slice().unwrap(),
            // "INDVI" => self.I * out.NDVI,
            // "VNDVI" => self.V * out.NDVI,
            // "VPPFNDVI" => self.V * out.PPF * out.NDVI,
            // "IPPFNDVI" => self.I * out.PPF * out.NDVI,

            _ => panic!("Unknown variable: {}", variable)
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
    // properties
    pub time: DateTime<Utc>,
    pub lons: Array1<f32>,
    pub lats: Array1<f32>,
    pub slope: Array1<f32>,
    pub aspect: Array1<f32>,
    
    // pub vegetation: String,
    pub ppf_summer: Array1<f32>,
    pub ppf_winter: Array1<f32>,
    
    // vegetation properties
    pub d0: Array1<f32>,
    pub d1: Array1<f32>,
    pub hhv: Array1<f32>,	
    pub umid: Array1<f32>,
    pub v0: Array1<f32>,
    pub T0: Array1<f32>,
    pub	sat: Array1<f32>,

    // state
    pub dffm: Array1<f32>,
    pub snow_cover: Array1<f32>,

}

impl State {
    /// Create a new state.
    pub fn new(
        // properties
        lon: Array1<f32>,
        lat: Array1<f32>,
        slope: Array1<f32>,
        aspect: Array1<f32>,       
        ppf_summer: Array1<f32>,
        ppf_winter: Array1<f32>,

        // vegetation properties
        d0: Array1<f32>,
        d1: Array1<f32>,
        hhv: Array1<f32>,	
        umid: Array1<f32>,
        v0: Array1<f32>,
        T0: Array1<f32>,
        sat: Array1<f32>,
        // state

        dffm: Array1<f32>,
        snow_cover: Array1<f32>,


        time: DateTime<Utc>
    ) -> State {
        State { 
            time,
            lons: lon,
            lats: lat,
            slope,
            aspect,
            d0,
            d1,
            hhv,
            umid,
            v0, 
            T0,
            ppf_summer,
            ppf_winter,
            dffm,
            sat,
            snow_cover,
        }
    }
    

    /// Update the state of the cells.
    pub fn update(&self, input: &Input, new_time: &DateTime<Utc>) -> State {
        let dt = 3600.0;
        let new_dffm = update_moisture(self, input, dt);

        let new_state = State {
            dffm: new_dffm,
            snow_cover: input.snow_cover,
            time: new_time.clone(),
            ..*self
        };

        new_state
    }



    pub fn coords(&self) -> (&[f32], &[f32]) {
        (
            &self.lats.as_slice().unwrap(), 
            &self.lons.as_slice().unwrap()
        )
    }
}   
