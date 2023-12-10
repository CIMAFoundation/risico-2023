pub const NODATAVAL: f32 = -9999.0;

pub const MODISSNOWVAL: f32 = 200.0;
pub const DFFM_DEFAULT: f32 = 20.0;

pub const T_STANDARD: f32 = 27.0;
pub const W_STANDARD: f32 = 0.0;
pub const H_STANDARD: f32 = 20.0;
pub const MAXRAIN: f32 = 0.1;

/// old constants for legacy ffmc functions
pub const A1_LEGACY: f32 = 1.0; //OLD
pub const R1_LEGACY:f32 = 12.119; //OLD
pub const R2_LEGACY:f32 = 20.77;  //OLD
pub const R3_LEGACY:f32 = 3.2;    //OLD

// pub const R1: f32 = 68.8371;
// pub const R2: f32 = 53.4436;
// pub const R3: f32 = 0.9423;

pub const R1: f32 = 68.658964;
pub const R2: f32 = 53.374067;
pub const R3: f32 = 0.935953;

// pub const A1: f32 = 0.7012;
pub const A1: f32 = 0.592789;
pub const A2: f32 = 0.555;
pub const A3: f32 = 10.6;
pub const A4: f32 = 0.5022;
pub const A5: f32 = 0.0133;
pub const A6: f32 = 0.000343;
pub const A7: f32 = 0.00722;

pub const B1: f32 = 3.0;
pub const B2: f32 = 0.60;
pub const B3: f32 = 0.1;

// pub const B1_D: f32 = 1.3037;
// pub const B2_D: f32 = 2.4539;
// pub const C1_D: f32 = 0.1753;
// pub const C2_D: f32 = 0.1141;
// pub const B1_W: f32 = 2.5942;
// pub const B2_W: f32 = 4.1077;
// pub const C1_W: f32 = 1.1502;
// pub const C2_W: f32 = 1.2764;

pub const B1_D: f32 = 0.112756;
pub const B2_D: f32 = 0.34982;
pub const B3_D: f32 = 0.111055;
pub const C1_D: f32 = 0.531471;
pub const C2_D: f32 = 0.534400;
pub const C3_D: f32 = 0.517728;
pub const B1_W: f32 = 0.104363;
pub const B2_W: f32 = 0.482954;
pub const B3_W: f32 = 0.100061;
pub const C1_W: f32 = 0.509857;
pub const C2_W: f32 = 0.6789;
pub const C3_W: f32 = 0.504871;

pub const GAMMA1: f32 = 1.0;
pub const GAMMA2: f32 = 0.01;
pub const GAMMA3: f32 = 1.4;

pub const DELTA1: f32 = 1.5;
pub const DELTA2: f32 = 0.8483;
pub const DELTA3: f32 = 16000.0;
pub const DELTA4: f32 = 1.25;
pub const DELTA5: f32 = 250000.0;

pub const LAMBDA: f32 = 2.0;

pub const QEPSIX2: f32 = 8.0;
pub const Q: f32 = 2442.0;
pub const PHI: f32 = 6.667;

pub const CLOUDCOVER: f32 = 1.0;

pub const SNOW_COVER_THRESHOLD: f32 = 0.001;

pub const SATELLITE_DATA_SECONDS_VALIDITY: i64 = 10 * 24 * 3600;
pub const SNOW_SECONDS_VALIDITY: i64 = 10 * 24 * 3600;

// ROS NEW CONSTANTS
pub const N_ANGLES_ROS: usize = 40;
pub const D1: f32 = 0.5;
pub const D2: f32 = 1.4;
pub const D3: f32 = 8.2;
pub const D4: f32 = 2.0;
pub const D5: f32 = 50.0;
pub const M0: f32 = 1.0003;
pub const M1: f32 = -1.7211;
pub const M2: f32 = 6.598;
pub const M3: f32 = -17.331;
pub const M4: f32 = 22.963;
pub const M5: f32 = -11.507;
pub const MX: f32 = 0.3;