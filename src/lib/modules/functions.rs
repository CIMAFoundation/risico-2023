use chrono::{DateTime, Datelike, Utc};
use std::f32::consts::PI;

pub fn daylight_hours(
    latitude: f32,       // latitude [°]
    date: DateTime<Utc>, // datetime
) -> f32 {
    // Calculate the number of daylight hours for a given latitude and julian day
    // Based on the FAO formula from https://wikifire.wsl.ch/tiki-index9a98.html?page=Daylight+hours&structure=Fire
    let jday = date.ordinal() as f32;
    let declination = 0.409 * ((2.0 * PI / 365.0) * jday - 1.39).sin();
    let latitude_rad = latitude * PI / 180.0;
    let sunset_angle = (-(latitude_rad).tan() * (declination).tan()).acos();

    sunset_angle * 24.0 / PI
}

pub fn evapotranspiration_thornthwaite(
    temp: f32,           // air temperature [°C]
    latitude: f32,       // latitude [°]
    date: DateTime<Utc>, // date
    heat_index: f32,     // heat index [°C]
) -> f32 {
    // [mm]
    // Thornthwaite method for calculating potential evapotranspiration
    // Thornthwaite, C. W. (1948). An approach toward a rational classification of climate.
    // Geographical Review, 38(1), 55-94.
    // NOTE: the original formula uses mean daily ari temperature, but other modifications are possible
    let a = 6.75e-7 * heat_index.powf(3.0) - 7.71e-5 * heat_index.powf(2.0)
        + 0.01792 * heat_index
        + 0.49239;
    let hsum: f32 = daylight_hours(latitude, date);
    let pet: f32 = if temp < 0.0 {
        0.0
    } else if temp <= 26.0 {
        16.0 * (hsum / 360.0) * (10.0 * temp / heat_index).powf(a)
    } else {
        (hsum / 360.0) * (-415.85 + 30.533224 * temp - 0.43 * temp.powf(2.0))
    };
    pet
}

pub fn heat_index(monthly_mean_temp: Vec<f32>, // monthly mean temperature [°C]
) -> f32 {
    // sum of the maximum between 0 and the monthlym mean temperature
    let heat_index = monthly_mean_temp
        .iter()
        .map(|&x| (x.max(0.0) / 5.0).powf(1.514))
        .sum();
    heat_index
}
