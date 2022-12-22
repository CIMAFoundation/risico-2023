use chrono::prelude::*;

use crate::library::config::models::InputDataHandler;
use crate::library::config::models::WarmState;

use super::constants::NODATAVAL;
use rayon::prelude::*;
use super::functions::get_output;
use super::functions::update_moisture;

const UPDATE_TIME: i64 = 100;
#[derive(Debug)]
pub struct Properties {
    pub lon: f32,
    pub lat: f32,
    pub slope: f32,
    pub aspect: f32,
    pub vegetation: String,
    pub ppf_summer: f32,
    pub ppf_winter: f32,
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
    pub	sat: f32,
    pub name: String
}

#[derive(Debug)]
#[derive(Clone)]
pub struct CellState {
    pub dffm: f32,
    pub snowCover: f32,
}


pub struct TimeStepOutput {
    pub time: DateTime<Utc>,
    pub data: Vec<CellState>
}

#[derive(Debug)]
pub struct CellOutput{
    pub time: DateTime<Utc>,

   	pub dffm: f32,
	pub W: f32,
	pub V: f32,
	pub I: f32,
	pub VPPF: f32,
	pub IPPF: f32,
	pub INDVI: f32,
	pub VNDVI: f32,
	pub VPPFNDVI: f32,
	pub IPPFNDVI: f32,
	pub NDVI: f32,
	pub INDWI: f32,
	pub VNDWI: f32,
	pub VPPFNDWI: f32,
	pub IPPFNDWI: f32,
	pub NDWI: f32,
	pub contrT: f32,
	pub SWI: f32,	
	pub temperature: f32,
	pub rain: f32,
	pub windSpeed: f32,
	pub windDir: f32,
	pub humidity: f32,
	pub snowCover: f32,
}


impl CellOutput {
    pub fn new(time: &DateTime<Utc>) -> Self {
        CellOutput {
            time: time.clone(),
            dffm: NODATAVAL,
            W: NODATAVAL,
            V: NODATAVAL,
            I: NODATAVAL,
            VPPF: NODATAVAL,
            IPPF: NODATAVAL,
            INDVI: NODATAVAL,
            VNDVI: NODATAVAL,
            VPPFNDVI: NODATAVAL,
            IPPFNDVI: NODATAVAL,
            NDVI: NODATAVAL,
            INDWI: NODATAVAL,
            VNDWI: NODATAVAL,
            VPPFNDWI: NODATAVAL,
            IPPFNDWI: NODATAVAL,
            NDWI: NODATAVAL,
            contrT: NODATAVAL,
            SWI: NODATAVAL,
            temperature: NODATAVAL,
            rain: NODATAVAL,
            windSpeed: NODATAVAL,
            windDir: NODATAVAL,
            humidity: NODATAVAL,
            snowCover: NODATAVAL,
        }
        
    }

    pub fn get(variable: &str) -> fn(&CellOutput) -> f32{
        match variable {
            "dffm" => |out| out.dffm,
            "W" => |out| out.W,
            "V" => |out| out.V,
            "I" => |out| out.I,
            "VPPF" => |out| out.VPPF,
            "IPPF" => |out| out.IPPF,
            "INDVI" => |out| out.INDVI,
            "VNDVI" => |out| out.VNDVI,
            "VPPFNDVI" => |out| out.VPPFNDVI,
            "IPPFNDVI" => |out| out.IPPFNDVI,
            "NDVI" => |out| out.NDVI,
            "INDWI" => |out| out.INDWI,
            "VNDWI" => |out| out.VNDWI,
            "VPPFNDWI" => |out| out.VPPFNDWI,
            "IPPFNDWI" => |out| out.IPPFNDWI,
            "NDWI" => |out| out.NDWI,
            "contrT" => |out| out.contrT,
            "SWI" => |out| out.SWI,
            "temperature" => |out| out.temperature,
            "rain" => |out| out.rain,
            "windSpeed" => |out| out.windSpeed,
            "windDir" => |out| out.windDir,
            "humidity" => |out| out.humidity,
            "snowCover" => |out| out.snowCover,
            _ => |_| NODATAVAL
        }
    }
}

pub struct CellInput {
    pub time: DateTime<Utc>,
    pub temperature: f32,
    pub rain: f32,
    pub windSpeed: f32,
    pub windDir: f32,
    pub humidity: f32,
    pub snowCover: f32,
    pub NDVI: f32,
    pub NDWI: f32,
}


#[derive(Debug)]
pub struct Cell<'a> {
    // The cell's properties
    pub properties: &'a Properties,
    // The cell's current state.
    pub vegetation: &'a Vegetation,
    pub state: CellState,
    pub output: Option<CellOutput>,
    // The cell's next state.
}

impl Cell<'_> {
    pub fn new<'a>(
        properties: &'a Properties, 
        warm_state: &'a WarmState,
        vegetation: &'a Vegetation
    ) -> Cell<'a> {
        Cell {
            properties,
            vegetation,
            state: CellState { 
                dffm: warm_state.dffm as f32,
                snowCover: 0.0
            },
            output: None,
        }
    }

    pub fn update(&self, time: &DateTime<Utc>, input: &CellInput) -> Cell {
        let dt = 3600.0;
        let new_dffm = update_moisture(self, input, dt);

        let new_state = CellState {
            dffm: new_dffm,
            snowCover: input.snowCover
        };

        let new_cell = Cell {
            properties: self.properties,
            vegetation: self.vegetation,
            state: new_state,
            output: None,
        };
        let output = get_output(&new_cell, time, input);
        Cell {
            output: Some(output),
            ..new_cell
        }
    }

}

#[derive(Debug)]
pub struct State<'a> {
    // The grid's cells.
    pub cells: Vec<Cell<'a>>,
    //pub outputs: Vec<CellOutput>,
    pub time: DateTime<Utc>
}

impl State<'_> {
    /// Create a new state.
    pub fn new(
        cells: Vec<Cell>, 
        
        time: DateTime<Utc>
    ) -> State {
        State { cells, time }
    }
    

    /// Update the state of the cells.
    pub fn update<'a>(&'a self, input_handler: &InputDataHandler, new_time: &DateTime<Utc>) -> State<'a> {
        // determine the new time for the state
        // execute the update function on each cell
        let cells = self.cells.iter()
                    .map(|cell| {
                        let (lat, lon) = (cell.properties.lat  as f32, cell.properties.lon  as f32);
                        let t = input_handler.get_value("T", &new_time, lat, lon) as f32 -273.15;
                        let u = input_handler.get_value("U", &new_time, lat, lon) as f32;
                        let v = input_handler.get_value("V", &new_time, lat, lon) as f32;
                        let p = input_handler.get_value("P", &new_time, lat, lon) as f32;
                        let h = input_handler.get_value("H", &new_time, lat, lon) as f32;

                        let wind_speed = f32::sqrt(f32::powi(u, 2) + f32::powi(v, 2)) * 3600.0;
                        let wind_dir = f32::atan2(u, v);

                        let cell_input = CellInput {
                            time: new_time.to_owned(),
                            temperature: t,
                            rain: p,
                            windSpeed: wind_speed,
                            windDir: wind_dir,
                            humidity: h,
                            snowCover: 0.0,
                            NDVI: 0.0,
                            NDWI: 0.0
                        };
                        (cell, cell_input)
                    }).map(|(cell, cell_input)| {
                        let new_cell = cell.update(&new_time, &cell_input);
                        new_cell
                    })
                    .collect::<Vec<Cell>>();
        //return the new state
        //State { cells: cells, time: new_time }

        State::new(cells, new_time.to_owned())
    }

    pub fn coords(&self) -> (Vec<f32>, Vec<f32>) {
        (
            self.cells.iter().map(|cell| cell.properties.lat).collect::<Vec<f32>>(),
            self.cells.iter().map(|cell| cell.properties.lon).collect::<Vec<f32>>()
        )
    }
    

}   
