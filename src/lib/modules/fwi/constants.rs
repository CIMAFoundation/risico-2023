pub const NODATAVAL: f32 = -9999.0;

pub const FFMC_INIT: f32 = 85.0;
pub const DMC_INIT: f32 = 6.0;
pub const DC_INIT: f32 = 15.0;

pub const TIME_WINDOW: i64 = 24;  // hours

// FFMC CONSTANTS
pub const FFMC_S1: f32 = 147.2;
pub const FFMC_S2: f32 = 59.4688;
pub const FFMC_S3: f32 = 59.5;
// Rain phase constants
pub const FFMC_MIN_RAIN: f32 = 0.5;  // daily cumulated rain (mm)
pub const FFMC_NORMAL_COND: f32 = 150.0;
pub const FFMC_R1: f32 = 42.5;
pub const FFMC_R2: f32 = 6.93;
pub const FFMC_R3: f32 = 0.0015;
pub const FFMC_R4: f32 = 2.0;
pub const FFMC_R5: f32 = 0.5;
// No-rain phase constants
pub const FFMC_A1D: f32 = 0.942;
pub const FFMC_A2D: f32 = 0.679;
pub const FFMC_A3D: f32 = 11.0;
pub const FFMC_A4D: f32 = 0.18;
pub const FFMC_A5D: f32 = 0.115;
pub const FFMC_A1W: f32 = 0.618;
pub const FFMC_A2W: f32 = 0.753;
pub const FFMC_A3W: f32 = 10.0;
pub const FFMC_A4W: f32 = 0.18;
pub const FFMC_A5W: f32 = 0.115;
pub const FFMC_B1: f32 = 0.424;
pub const FFMC_B2: f32 = 1.7;
pub const FFMC_B3: f32 = 0.0694;
pub const FFMC_B4: f32 = 0.5;
pub const FFMC_B5: f32 = 8.0;
pub const FFMC_B6: f32 = 0.581;
pub const FFMC_B7: f32 = 0.0365;

// DMC CONSTANTS
// rain effect
pub const DMC_MIN_RAIN: f32 = 1.5;
pub const DMC_A1: f32 = 33.0;
pub const DMC_A2: f32 = 65.0;
pub const DMC_R1: f32 = 0.92;
pub const DMC_R2: f32 = 1.27;
pub const DMC_R3: f32 = 0.5;
pub const DMC_R4: f32 = 0.3;
pub const DMC_R5: f32 = 14.0;
pub const DMC_R6: f32 = 1.3;
pub const DMC_R7: f32 = 6.2;
pub const DMC_R8: f32 = 17.2;
pub const DMC_R9: f32 = 20.0;
pub const DMC_R10: f32 = 244.72;
pub const DMC_R11: f32 = 43.43;
pub const DMC_R12: f32 = 48.77;
// temperature effect
pub const DMC_MIN_TEMP: f32 = -1.1;
pub const DMC_T1: f32 = 1.894;
pub const DMC_T2: f32 = 1.1;

// DC CONSTANTS
// rain effect
pub const DC_MIN_RAIN: f32 = 2.8;
pub const DC_R1: f32 = 0.83;
pub const DC_R2: f32 = 1.27;
pub const DC_R3: f32 = 800.0;
pub const DC_R4: f32 = 400.0;
pub const DC_R5: f32 = 3.937;
// temperature effect
pub const DC_MIN_TEMP: f32 = 0.0;
pub const DC_T1: f32 = 0.36;
pub const DC_T2: f32 = 2.8;
pub const DC_T3: f32 = 0.5;

// ISI CONSTANTS
pub const ISI_A0: f32 = 0.05039;
pub const ISI_A1: f32 = 91.9;
pub const ISI_A2: f32 = -0.1386;
pub const ISI_A3: f32 = 5.31;
pub const ISI_A4: f32 = 4.93;
pub const ISI_A5: f32 = 0.208;

// BUI CONSTANTS
pub const BUI_A1: f32 = 0.4;
pub const BUI_A2: f32 = 0.8;
pub const BUI_A3: f32 = 0.92;
pub const BUI_A4: f32 = 0.0114;
pub const BUI_A5: f32 = 1.7;

// FWI CONSTANTS
pub const FWI_A1: f32 = 0.626;
pub const FWI_A2: f32 = 0.809;
pub const FWI_A3: f32 = 2.0;
pub const FWI_A4: f32 = 25.0;
pub const FWI_A5: f32 = 108.64;
pub const FWI_A6: f32 = -0.023;
pub const FWI_A7: f32 = 2.72;
pub const FWI_A8: f32 = 0.434;
pub const FWI_A9: f32 = 0.647;


pub const IFWI_A1: f32 = 0.98;
pub const IFWI_A2: f32 = 1.546;
pub const IFWI_A3: f32 = 0.289;
