pub const NODATAVAL: f32 = -9999.0;
// NOTE: the KBDI index should be initilized to 0 but after a period of rainfall
pub const KBDI_INIT: f32 = 0.0;  // [mm]
pub const KBDI_RAIN_RUNOFF: f32 = 5.0;  // mm (almost 0.2 inch) from Finkele et al. 2006
pub const TIME_WINDOW: i64 = 20;  // days