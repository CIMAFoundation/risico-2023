

#[allow(dead_code)]
///functions to work on the state of the risico model

use chrono::{DateTime, Utc, Datelike};

use super::{constants::*};


pub fn get_ffm(ffm: f32) -> f32 {
    ffm + 1.0
}


///calculate PPF from the date and the two values
pub fn get_ppf(time: &DateTime<Utc>, ppf_summer: f32, ppf_winter: f32) -> f32{
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
            let val:f32 = (day_number - (MARCH_31 + 1)) as f32 / (MAY_31 - MAY_31) as f32;
            val * ppf_summer + (1.0 - val) * ppf_winter
        }
        JUNE_1..=SEPTEMBER_30 => ppf_summer,
        OCTOBER_1..=NOVEMBER_30 => {
            let val: f32 = 1.0 - ((day_number - (SEPTEMBER_30 + 1)) as f32 / (NOVEMBER_30 - SEPTEMBER_30) as f32);
            val * ppf_summer + (1.0 - val) * ppf_winter
        },
        DECEMBER_1..=366 => ppf_winter,
        _ => panic!("Invalid day number")
    };
    ppf
}

///calculate the wind effect on fire propagation
pub fn get_wind_effect(w_speed: f32, w_dir: f32, slope: f32, aspect: f32) -> f32{
	let mut w: f32 = 0.0;
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


pub fn get_slope_effect(slope: f32) -> f32 {
	1.0 + LAMBDA * (slope / (PI / 2.0))	
}

pub fn get_v0(v0: f32, d0: f32, _d1: f32, dffm: f32, snow_cover: f32) -> f32{
    if snow_cover > 0.0 || d0 == NODATAVAL { 
        return 0.0;
    }
	
	v0 * (-1.0 * (dffm / 20.0).powf(2.0)).exp()	
}

pub fn get_v(v0: f32, w_effect: f32, s_effect: f32, t_effect:f32) -> f32{
	v0 * w_effect * s_effect * t_effect
}

pub fn get_t_effect(t: f32) -> f32{
	if t <= 0.0 {
		return 1.0;
	}
    (t * 0.0171).exp()	
}

pub fn get_lhv_dff(hhv: f32, dffm: f32) -> f32 {
	hhv * (1.0 - (dffm / 100.0)) - Q * (dffm / 100.0)
}

pub fn get_lhv_l1(humidity: f32, msi: f32, hhv: f32) -> f32{
	
	if humidity == NODATAVAL {
        return 0.0;
    }
    let lhv_l1: f32;
    if msi >= 0.0 && msi <= 1.0 {
        let l1_msi = f32::max(20.0, humidity - (20.0 * msi));
        lhv_l1 = hhv * (1.0 - (l1_msi / 100.0)) - Q * (l1_msi / 100.0);
    }
    else{
        lhv_l1 = hhv * (1.0 - (humidity / 100.0)) - Q * (humidity / 100.0);
    }

	lhv_l1
}


///calculate the fire intensity
pub fn get_intensity(d0: f32, d1: f32, v: f32, relative_greenness: f32, lhv_dff: f32, lhv_l1: f32) -> f32{
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
pub fn update_dffm_rain(r: f32, dffm: f32, sat: f32) -> f32{
	let delta_dffm = r * R1 * (-R2 / ((sat + 1.0) - dffm)).exp() *(1.0 - (-R3 / r).exp());
	let dffm = dffm + delta_dffm;

	f32::min(dffm, sat)
}


///Get the new value for the dfmm when there is no rain (p<p*)
pub fn update_dffm_dry(dffm: f32, _sat: f32, t: f32, w: f32, h: f32, t0: f32, dt: f32) -> f32{
	let emc  = A1 * h.powf(A2) + 
                    A3 * ((h - 100.0)/10.0).exp() + 
                    A4 * (30.0 - f32::min(t, 30.0))*(1.0 - (-A5 * h).exp());
	let k1 = t0 / (1.0 + A6 * t.powf(B1) + A7 * w.powf(B2));

	//dinamica di drying / wetting
	let dffm = emc + (dffm - emc) * (-dt/k1).exp();
	let dffm = if dffm >= 0.0 { dffm } else { 0.0 } ;
	return dffm;
}


pub fn index_from_swi(dffm: f32, swi: f32) -> f32{
	if swi <= 10.0 { return 0.0 } ;
    dffm

}



// pub fn update_snow_cover(input: &Input, NDSI: f32) -> f32{
// 	// Controllo la neve. Se >25 cm, considero innevato, altrimenti no
	
// 	let mut snowCover = input.snow_cover;
// 	if NDSI != NODATAVAL{
// 		if NDSI == MODISSNOWVAL {
// 			snowCover = 1.0;
// 		} else {
// 			snowCover = NODATAVAL;
// 		}
// 	}
// 	snowCover
// }

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


