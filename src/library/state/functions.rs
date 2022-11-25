use chrono::Date;
#[allow(dead_code)]
///functions to work on the state of the risico model

use chrono::{DateTime, Utc, Datelike};
use super::{constants::*, models::{Properties, Vegetation, Cell, CellInput, CellOutput}};


pub fn get_ffm(ffm: f64) -> f64 {
    ffm + 1.0
}


///calculate PPF from the date and the two values
pub fn get_ppf(time: DateTime<Utc>, ppf_summer: f64, ppf_winter: f64) -> f64{
	const MARCH_31:u32 = 89;
    const APRIL_1:u32 = 90;
    const MAY_31:u32 = 150;
    const JUNE_1:u32 = 151;
    const SEPTEMBER_30:u32 = 272;
    const OCTOBER_1:u32 = 273;
    const NOVEMBER_30:u32 = 334;
    const DECEMBER_1:u32 = 335;

	if ppf_summer < 0.0 || ppf_winter < 0.0 {
		return 0.0;
	} 
    let day_number: u32 = time.date_naive().ordinal();
    
    let ppf = match day_number{
        1..=MARCH_31 => ppf_winter,
        APRIL_1..=MAY_31 => {
            let val:f64 = (day_number - (MARCH_31 + 1)) as f64 / (MAY_31 - MAY_31) as f64;
            val * ppf_summer + (1.0 - val) * ppf_winter
        }
        JUNE_1..=SEPTEMBER_30 => ppf_summer,
        OCTOBER_1..=NOVEMBER_30 => {
            let val: f64 = 1.0 - ((day_number - (SEPTEMBER_30 + 1)) as f64 / (NOVEMBER_30 - SEPTEMBER_30) as f64);
            val * ppf_summer + (1.0 - val) * ppf_winter
        },
        DECEMBER_1..=366 => ppf_winter,
        _ => panic!("Invalid day number")
    };
    ppf
}

///calculate the wind effect on fire propagation
pub fn get_wind_effect(w_speed: f64, w_dir: f64, slope: f64, aspect: f64) -> f64{
	let mut w: f64 = 0.0;
	//wind speed effect    
	let ws = (1.0 + DELTA1 * (DELTA2 + ((w_speed / DELTA3) - DELTA4)).tanh())
					* (1.0 - (w_speed / DELTA5));
	let eta = w_dir - aspect;

	//aspect contribution
	let mut n = 1.0 + (slope / (PI / 2.0)) * (ws - 1.0)
							* (-(eta - PI).powf(2.0) / QEPSIX2).exp();
	if n < 1.0 {
		n = 1.0;
    }

	if ws != NODATAVAL {
		w = ws / n;
	}
	return w;
}


pub fn get_slope_effect(slope: f64) -> f64 {
	1.0 + LAMBDA * (slope / (PI / 2.0))	
}

pub fn get_v0(v0: f64, d0: f64, d1: f64, dffm: f64, snowCover: f64) -> f64{
    if snowCover > 0.0 || d0 == NODATAVAL { 
        return 0.0;
    }
	
	v0 * (-1.0 * (dffm / 20.0).powf(2.0)).exp()	
}

pub fn get_v(v0: f64, w_effect: f64, s_effect: f64, t_effect:f64) -> f64{
	v0 * w_effect * s_effect * t_effect
}

pub fn get_t_effect(t: f64) -> f64{
	if(t <= 0.0) {
		return 1.0;
	}
    (t * 0.0171).exp()	
}

pub fn get_lhv_dff(hhv: f64, dffm: f64) -> f64 {
	hhv * (1.0 - (dffm / 100.0)) - Q * (dffm / 100.0)
}

pub fn get_lhv_l1(humidity: f64, MSI: f64, hhv: f64) -> f64{
	
	if humidity == NODATAVAL {
        return 0.0;
    }
    let lhv_l1: f64;
    if MSI >= 0.0 && MSI <= 1.0 {
        let l1_msi = f64::max(20.0, humidity - (20.0 * MSI));
        lhv_l1 = hhv * (1.0 - (l1_msi / 100.0)) - Q * (l1_msi / 100.0);
    }
    else{
        lhv_l1 = hhv * (1.0 - (humidity / 100.0)) - Q * (humidity / 100.0);
    }

	lhv_l1
}


///calculate the fire intensity
pub fn getI(d0: f64, d1: f64, v: f64, relative_greenness: f64, lhv_dff: f64, lhv_l1: f64) -> f64{
    let mut d0 = d0;
    let mut d1 = d1;

	if d1 == NODATAVAL { d1 = 0.0; }
	if d0 == NODATAVAL { d0 = 0.0; }

	
	if relative_greenness >= 0.0 {
		if d1 == 0.0 {
			return v * (lhv_dff * d0 * (1.0 - relative_greenness))/ 3600.0 ;
		}
		
		return  v * (lhv_dff * d0 + lhv_l1 * (d1 *(1.0 - relative_greenness)))/ 3600.0 ;
		
	}

	v * (lhv_dff * d0 + lhv_l1 * d1)/3600.0	
}



///Get the new value for the dfmm when is raining (p>p*)
pub fn update_dffm_rain(R: f64, dffm: f64, sat: f64) -> f64{
	let delta_dffm = R * R1 * (-R2 / ((sat + 1.0) - dffm)).exp() *(1.0 - (-R3 / R).exp());
	let dffm = dffm + delta_dffm;

	f64::min(dffm, sat)
}


///Get the new value for the dfmm when there is no rain (p<p*)
pub fn update_dffm_dry(dffm: f64, sat: f64, t: f64, w: f64, h: f64, t0: f64, dt: f64) -> f64{
	let emc  = A1 * h.powf(A2) + 
                    A3 * ((h - 100.0)/10.0).exp() + 
                    A4 * (30.0 - f64::min(t, 30.0))*(1.0 - (-A5 * h).exp());
	let k1 = t0 / (1.0 + A6 * t.powf(B1) + A7 * w.powf(B2));

	//dinamica di drying / wetting
	let dffm = emc + (dffm - emc) * (-dt/k1).exp();
	let dffm = if dffm >= 0.0 { dffm } else { 0.0 } ;
	return dffm;
}


pub fn index_from_swi(dffm: f64, SWI: f64) -> f64{
	if SWI <= 10.0 { return 0.0 } ;
    dffm

}



pub fn update_moisture(cell: &Cell, input: &CellInput, dt: f64 ) -> f64{
	let par = cell.vegetation;
	let dffm = cell.state.dffm;

	if par.d0 == NODATAVAL {
		return NODATAVAL;
	}

	if cell.state.snowCover > SNOW_COVER_THRESHOLD{
		return par.sat;
	}

	if dffm == NODATAVAL || input.temperature == NODATAVAL || input.humidity == NODATAVAL{
		return dffm;
	}
	
	let T = if input.temperature > 0.0  { input.temperature }  else  {0.0};
	
	let H = if input.humidity < 100.0 { input.humidity } else { 100.0 };
	let W = if input.windSpeed != NODATAVAL { input.windSpeed } else { 0.0 };
	let R = if input.rain != NODATAVAL { input.rain } else { 0.0 };

	//let dT = f64::max(1.0, f64::min(72.0, ((currentTime - previousTime) / 3600.0)));
	//		float pdffm = dffm;
	// modello per temperature superiori a 0 gradi Celsius
	if R > MAXRAIN {
		return update_dffm_rain(R, dffm, par.sat);
	}
	else {
		return update_dffm_dry(dffm, par.sat, T, W, H, par.T0, dt);
	}
	
	
}

pub fn update_snow_cover(input: &CellInput, NDSI: f64) -> f64{
	// Controllo la neve. Se >25 cm, considero innevato, altrimenti no
	
	let mut snowCover = input.snowCover;
	if NDSI != NODATAVAL{
		if NDSI == MODISSNOWVAL {
			snowCover = 1.0;
		} else {
			snowCover = NODATAVAL;
		}
	}
	snowCover
}

// pub fn updateSatelliteData(RISICO_CellInput *input)
// {
// 	if (input.NDSI < 0)
// 	{
// 		if (NDSI_TTL > 0)
// 		{
// 			NDSI_TTL -= 1;
// 		}
// 		else
// 		{
// 			NDSI = -9999;
// 		}
// 	}
// 	else
// 	{
// 		NDSI = input.NDSI;
// 		NDSI_TTL = 56;
// 	}
// 	if (input.MSI < 0 || input.MSI > 1)
// 	{
// 		if (MSI_TTL > 0)
// 		{
// 			MSI_TTL -= 1;
// 		}
// 		else
// 		{
// 			MSI = -9999;
// 		}
// 	}
// 	else
// 	{
// 		MSI = input.MSI;
// 		MSI_TTL = 56;
// 	}
// 	if (input.NDVI == NODATAVAL)
// 	{
// 		if (input.time - NDVI_TIME > (240 * 3600))
// 		{
// 			NDVI = NODATAVAL;
// 		}
// 	}
// 	else
// 	{
// 		if (input.NDVI < 0.0f || input.NDVI > 1.0f)
// 		{
// 			NDVI = NODATAVAL;
// 		}
// 		else
// 		{
// 			NDVI = input.NDVI;
// 			NDVI_TIME = input.time;
// 		}
// 	}

// 	if (input.NDWI == NODATAVAL)
// 	{
// 		if (input.time - NDWI_TIME > (240 * 3600))
// 		{
// 			NDWI = NODATAVAL;
// 		}
// 	}
// 	else
// 	{
// 		if (input.NDWI < 0.0f || input.NDWI > 1.0f)
// 		{
// 			NDWI = NODATAVAL;
// 		}
// 		else
// 		{
// 			NDWI = input.NDWI;
// 			NDWI_TIME = input.time;
// 		}
// 	}
// }

pub fn get_output(cell: &Cell, time: &DateTime<Utc>, input: &CellInput) -> CellOutput{
	let mut out = CellOutput::new(time);
	
	let state = &cell.state;
	let par = cell.vegetation;

	let slope = cell.properties.slope;
	let aspect = cell.properties.aspect;

	if state.dffm == NODATAVAL || input.temperature == NODATAVAL	{
		return out;
	}

	let WSpeed = if input.windSpeed != NODATAVAL { input.windSpeed }   else { 0.0 };
	let WDir = if input.windDir != NODATAVAL  { input.windDir } else { 0.0 };
	let T = input.temperature;

	
	// Contributo del vento
	let W_Effect = get_wind_effect(WSpeed, WDir, slope, aspect);
	
	let dffm = state.dffm;
	let snowCover = state.snowCover;

	// calcolo della velocità iniziale
	let V0 = get_v0(par.v0, par.d0, par.d1, dffm, snowCover);

	let mut T_Effect = 1.0;
	let use_TEffect = false;
	if use_TEffect{
		T_Effect = get_t_effect(T);
	}
	let slopeEffect = get_slope_effect(slope);
	let V = get_v(V0, W_Effect, slopeEffect, T_Effect);

	let mut LHVdff = NODATAVAL;
	let mut LHVl1 = NODATAVAL;
	let mut I = NODATAVAL;
	let mut IPPF = NODATAVAL;
	let mut VPPF = 1.0;
	if par.hhv != NODATAVAL && dffm != NODATAVAL {
		// calcolo LHV per la lettiera
		LHVdff = get_lhv_dff(par.hhv, dffm);
		// calcolo LHV per la vegetazione viva
		let MSI = NODATAVAL;
		LHVl1 = get_lhv_l1(par.umid, MSI, par.hhv);
		// Calcolo Intensità
		let NDVI = NODATAVAL;
		I = getI(par.d0, par.d1, V, NDVI, LHVdff, LHVl1);
		// Calcolo PPF
		
		//PPF = getPPF(time, par.PPF_summer, par.PPF_winter);
	}

	let vNDVI = 1.0;
	// if (par.useNDVI) {
	// 	vNDVI = (1 - max(min(NDVI, 1.0f), 0.0f));
	// }
	//let vNDWI = (1 - f64::max(f64::min(NDWI, 1.0), 0.0));

	// IPPF = I != NODATAVAL ? I * PPF : NODATAVAL;
	// VPPF = V != NODATAVAL ? V * PPF : NODATAVAL;
	// IPPFNDVI = I != NODATAVAL ? IPPF * vNDVI : NODATAVAL;
	// VPPFNDVI = V != NODATAVAL ? VPPF * vNDVI : NODATAVAL;
	// INDVI = I != NODATAVAL ? I * vNDVI : NODATAVAL;
	// VNDVI = V != NODATAVAL ? V * vNDVI : NODATAVAL;

	// IPPFNDWI = I != NODATAVAL ? IPPF * vNDWI : NODATAVAL;
	// VPPFNDWI = V != NODATAVAL ? VPPF * vNDWI : NODATAVAL;
	// INDWI = I != NODATAVAL ? I * vNDWI : NODATAVAL;
	// VNDWI = V != NODATAVAL ? V * vNDWI : NODATAVAL;

	//out.cellID = this.ID;
	out.time = input.time;
	//out.coord.set(m_oCoord.x, m_oCoord.y);

	// Variabili di input
	out.temperature = input.temperature;
	out.rain = input.rain;
	out.humidity = input.humidity;
	out.windDir = input.windDir;
	out.windSpeed = input.windSpeed;

	// variabili di stato
	out.dffm = dffm;
	out.snowCover = snowCover;

	// variabili di output
	out.contrT = T_Effect;
	out.W = W_Effect;
	out.V = V;
	out.I = I;

	// out.IPPF = IPPF;
	// out.VPPF = VPPF;

	// out.VPPFNDVI = VPPFNDVI;
	// out.IPPFNDVI = IPPFNDVI;
	// out.VNDVI = VNDVI;
	// out.INDVI = INDVI;

	// out.NDVI = NODATAVAL;
	// if (par.useNDVI)
	// {
	// 	out.NDVI = vNDVI;
	// }

	// out.VPPFNDWI = VPPFNDWI;
	// out.IPPFNDWI = IPPFNDWI;
	// out.VNDWI = VNDWI;
	// out.INDWI = INDWI;
	// out.NDWI = vNDWI;

	//out.SWI = calculateIndexFromSWI(dffm, input.SWI);
	out

}