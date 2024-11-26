pub const NODATAVAL: f32 = -9999.0;
// NOTE: the KBDI index should be initilized to 0 but after a period of rainfall
// We set initial value equal to half of the rangin [0, 200] since we do not have info of
// the period with greater rainfall
pub const KBDI_INIT: f32 = 100.0;  // [mm]
pub const KBDI_RAIN_RUNOFF: f32 = 5.0;  // mm (almost 0.2 inch) from Finkele et al. 2006
pub const TIME_WINDOW: i64 = 20;  // days