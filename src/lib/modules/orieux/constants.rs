pub const NODATAVAL: f32 = -9999.0;
pub const R_MAX: f32 = 150.0;  // maximum water reserve of the soil [mm]
// NOTE: the initiali water reserve shouldb be the maxium after a period of rain
// Since this isnfo is not given we use the "half the range" approach
pub const ORIEUX_WR_INIT: f32 = 75.0;  // initial water reserve of the soil [mm]
pub const ORIEUX_WR_MAX: f32 = 150.0;  // maximum water reserve of the soil [mm]