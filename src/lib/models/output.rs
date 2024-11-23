use chrono::{DateTime, Utc};
use ndarray::Array1;
use rayon::prelude::*;
use serde_derive::{Deserialize, Serialize};
use strum_macros::{Display, EnumProperty, EnumString};

use crate::constants::NODATAVAL;

#[allow(non_snake_case)]
pub struct OutputElement {
    // ------------------- RISICO ------------------- //
    /// Fine fuel moisture content
    pub dffm: f32,
    /// Wind effect on fire spread
    pub W: f32,
    /// Rate of spread
    pub V: f32,
    /// Intensity
    pub I: f32,
    /// NDVI effect
    pub NDVI: f32,
    /// NDWI effect
    pub NDWI: f32,
    /// Probability of ignition
    pub PPF: f32,
    /// Temperature effect on fire spread
    pub t_effect: f32,
    // pub SWI: f32,

    // ---------------- FWI ----------------- //
    /// Fine Fuel Moisture Code
    pub ffmc: f32,
    /// Duff Moisture Code
    pub dmc: f32,
    /// Dought Code
    pub dc: f32,
    /// Initial Spread  Index
    pub isi: f32,
    /// Build Up Index
    pub bui: f32,
    /// Fire Weather Index
    pub fwi: f32,
    /// IFWI
    pub ifwi: f32,

    // ------------- Keetch-Byram Drought Index ----------------- //
    pub kbdi: f32,

    // ------------- Mark 5 ----------------- //
    /// Drought Factor
    pub df: f32,
    /// Fire Danger Index
    pub ffdi: f32,

    // ------------- Angstrom Index ----------------- //
    pub angstrom: f32,

    // ------------- Fosberg Index ----------------- //
    pub ffwi: f32,

    // ----------------- INPUTS ---------------//
    /// Input temperature in celsius
    pub temperature: f32,
    /// Input rain in mm
    pub rain: f32,
    /// Input wind speed in m/s
    pub wind_speed: f32,
    /// Input wind direction in radians
    pub wind_dir: f32,
    /// Input relative humidity in %
    pub humidity: f32,
    /// Input snow cover
    pub snow_cover: f32,
    /// Meteorological index
    pub meteo_index: f32,
}

impl Default for OutputElement {
    fn default() -> Self {
        Self {
            // RISICO
            dffm: NODATAVAL,
            W: NODATAVAL,
            V: NODATAVAL,
            I: NODATAVAL,
            NDVI: NODATAVAL,
            NDWI: NODATAVAL,
            PPF: NODATAVAL,
            t_effect: NODATAVAL,
            meteo_index: NODATAVAL,

            // FWI
            ffmc: NODATAVAL,
            dmc: NODATAVAL,
            dc: NODATAVAL,
            isi: NODATAVAL,
            bui: NODATAVAL,
            fwi: NODATAVAL,
            ifwi: NODATAVAL,

            // Keech-Byram Drought Index
            kbdi: NODATAVAL,

            // Mark 5
            df: NODATAVAL,
            ffdi: NODATAVAL,

            // Angstrom
            angstrom: NODATAVAL,

            // Fosberg
            ffwi: NODATAVAL,

            // input variables
            temperature: NODATAVAL,
            rain: NODATAVAL,
            wind_speed: NODATAVAL,
            wind_dir: NODATAVAL,
            humidity: NODATAVAL,
            snow_cover: NODATAVAL,
        }
    }
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(
    Debug,
    PartialEq,
    Eq,
    Hash,
    Copy,
    Clone,
    EnumString,
    EnumProperty,
    Display,
    Serialize,
    Deserialize,
)]
#[strum(ascii_case_insensitive)]
pub enum OutputVariableName {
    /// Fine Fuel Moisture
    #[strum(props(long_name = "Fine Fuel Moisture", units = "%"))]
    dffm,
    /// Wind Effect on Fire Spread
    #[strum(props(long_name = "Wind Effect on Fire Spread", units = "-"))]
    W,
    /// Fire Spread Rate
    #[strum(props(long_name = "Fire Spread Rate", units = "m/h"))]
    V,
    /// Fire Intensity
    #[strum(props(long_name = "Fire Intensity", units = "kW/m"))]
    I,

    /// Temperature Effect on Fire Spread
    #[strum(props(long_name = "Temperature Effect on Fire Spread", units = "-"))]
    contrT,

    /// Input Temperature
    #[strum(props(long_name = "Input Temperature", units = "°C"))]
    temperature,
    /// Input Rain
    #[strum(props(long_name = "Input Rain", units = "mm"))]
    rain,

    /// Input Wind Speed
    #[strum(props(long_name = "Input Wind Speed", units = "m/s"))]
    windSpeed,

    /// Input Wind Direction
    #[strum(props(long_name = "Input Wind Direction", units = "°"))]
    windDir,

    /// Input Relative Humidity
    #[strum(props(long_name = "Input Relative Humidity", units = "%"))]
    humidity,

    /// Input Snow Cover
    #[strum(props(long_name = "Input Snow Cover", units = "mm"))]
    snowCover,

    /// NDVI factor
    #[strum(props(long_name = "NDVI factor", units = "-"))]
    NDVI,

    /// NDWI factor
    #[strum(props(long_name = "NDWI factor", units = "-"))]
    NDWI,

    /// Meteorological Index
    #[strum(
        props(long_name = "Meteorological Index", units = "-"),
        serialize = "meteoIndex",
        serialize = "meteoIndex2"
    )]
    meteoIndex2,

    /// Fire Spread Rate + PPF
    #[strum(props(long_name = "Fire Spread Rate + PPF", units = "m/h"))]
    VPPF,

    /// Fire Intensity + PPF
    #[strum(props(long_name = "Fire Intensity + PPF", units = "kW/m"))]
    IPPF,

    /// Fire Intensity + NDWI factor
    #[strum(props(long_name = "Fire Intensity + NDWI factor", units = "kW/m"))]
    INDWI,

    /// Fire Spread rate + NDWI factor
    #[strum(props(long_name = "Fire Spread rate + NDWI factor", units = "m/h"))]
    VNDWI,

    /// Fire Intensity + NDVI factor
    #[strum(props(long_name = "Fire Intensity + NDVI factor", units = "kW/m"))]
    INDVI,
    /// Fire Spread rate + NDVI factor
    #[strum(props(long_name = "Fire Spread rate + NDVI factor", units = "m/h"))]
    VNDVI,

    /// Fire Spread rate + PPF + NDWI factor
    #[strum(props(long_name = "Fire Spread rate + PPF + NDWI factor", units = "m/h"))]
    VPPFNDWI,
    /// Fire Intensity + PPF + NDWI factor
    #[strum(props(long_name = "Fire Intensity + PPF + NDWI factor", units = "kW/m"))]
    IPPFNDWI,

    /// Fire Spread rate + PPF + NDVI factor
    #[strum(props(long_name = "Fire Spread rate + PPF + NDVI factor", units = "m/h"))]
    VPPFNDVI,
    /// Fire Intensity + PPF + NDVI factor
    #[strum(props(long_name = "Fire Intensity + PPF + NDVI factor", units = "kW/m"))]
    IPPFNDVI,

    /// Fine Fuel Moisture Code
    #[strum(props(long_name = "Fine Fuel Moisture Code", units = "-"))]
    ffmc,
    /// Duff Moisture Code
    #[strum(props(long_name = "Duff Moisture Code", units = "-"))]
    dmc,
    /// Drought Code
    #[strum(props(long_name = "Drought Code", units = "-"))]
    dc,
    /// Initial Spread Index
    #[strum(props(long_name = "Initial Spread Index", units = "-"))]
    isi,
    /// Build Up Index
    #[strum(props(long_name = "Build Up Index", units = "-"))]
    bui,
    /// Fire Weather Index
    #[strum(props(long_name = "Fire Weather Index", units = "-"))]
    fwi,
    /// Fire Weather Index
    #[strum(props(long_name = "IFWI", units = "-"))]
    ifwi,

    // Keetch-Byram Drought Index
    #[strum(props(long_name = "Keetch-Byram Drought Index", units = "mm"))]
    kbdi,

    // Mark 5 - Drought Factor
    #[strum(props(long_name = "Drought Factor", units = "-"))]
    df,

    // Mark 5 - Fire Danger Index
    #[strum(props(long_name = "Mark5 Fire Danger Index", units = "-"))]
    ffdi,

    // Angstrom Index
    #[strum(props(long_name = "Angstrom Index", units = "-"))]
    angstrom,
    
    // Fosberg Index
    #[strum(props(long_name = "Fosberg Fire Weather Index", units = "-"))]
    ffwi,
}


fn get_derived(a: &f32, b: &f32, c: Option<&f32>) -> f32 {
    let mut r = *a;

    if *b != NODATAVAL {
        r = a * b;
    }

    if let Some(c) = c {
        if *c != NODATAVAL {
            r *= c;
        }
    }
    r
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

    pub fn get(&self, variable: &OutputVariableName) -> Option<Array1<f32>> {
        use OutputVariableName::*;
        match variable {
            // Output variables
            dffm => Some(self.get_array(|o| o.dffm)),
            W => Some(self.get_array(|o| o.W)),
            V => Some(self.get_array(|o| o.V)),
            I => Some(self.get_array(|o| o.I)),
            contrT => Some(self.get_array(|o| o.t_effect)),

            NDVI => Some(self.get_array(|o| o.NDVI)),
            NDWI => Some(self.get_array(|o| o.NDWI)),
            meteoIndex2 => Some(self.get_array(|o| o.meteo_index)),

            // Input variables
            temperature => Some(self.get_array(|o| o.temperature)),
            rain => Some(self.get_array(|o| o.rain)),
            windSpeed => Some(self.get_array(|o| o.wind_speed)),
            windDir => Some(self.get_array(|o| o.wind_dir)),
            humidity => Some(self.get_array(|o| o.humidity)),
            snowCover => Some(self.get_array(|o| o.snow_cover)),

            //Derived variables
            VPPF => Some(self.get_array(|o| get_derived(&o.V, &o.PPF, None))),
            IPPF => Some(self.get_array(|o| get_derived(&o.I, &o.PPF, None))),
            INDWI => Some(self.get_array(|o| get_derived(&o.I, &o.NDWI, None))),
            VNDWI => Some(self.get_array(|o| get_derived(&o.V, &o.NDWI, None))),
            INDVI => Some(self.get_array(|o| get_derived(&o.I, &o.NDVI, None))),
            VNDVI => Some(self.get_array(|o| get_derived(&o.V, &o.NDVI, None))),
            VPPFNDWI => Some(self.get_array(|o| get_derived(&o.V, &o.NDWI, Some(&o.PPF)))),
            IPPFNDWI => Some(self.get_array(|o| get_derived(&o.I, &o.NDWI, Some(&o.PPF)))),
            VPPFNDVI => Some(self.get_array(|o| get_derived(&o.V, &o.NDVI, Some(&o.PPF)))),
            IPPFNDVI => Some(self.get_array(|o| get_derived(&o.I, &o.NDVI, Some(&o.PPF)))),

            ffmc => Some(self.get_array(|o| o.ffmc)),
            dmc => Some(self.get_array(|o| o.dmc)),
            dc => Some(self.get_array(|o| o.dc)),
            isi => Some(self.get_array(|o| o.isi)),
            bui => Some(self.get_array(|o| o.bui)),
            fwi => Some(self.get_array(|o| o.fwi)),
            ifwi => Some(self.get_array(|o| o.ifwi)),
        
            kbdi => Some(self.get_array(|o| o.kbdi)),
            df => Some(self.get_array(|o| o.df)),
            ffdi => Some(self.get_array(|o| o.ffdi)),

            angstrom => Some(self.get_array(|o| o.angstrom)),

            ffwi => Some(self.get_array(|o| o.ffwi)),
        }
    }
}
