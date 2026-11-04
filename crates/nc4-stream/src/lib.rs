//! A pure-Rust streaming writer for NetCDF-4 (HDF5) files.
//!
//! The writer is built for output that arrives one record at a time along an
//! unlimited dimension, such as a model emitting one grid per timestep.
//! Every record is compressed and appended to the file as soon as it is
//! given, so the memory held per open file is one chunk. All metadata, the
//! chunk index included, is written when the file is finished.
//!
//! The on-disk structures are the ones netcdf-c itself emits (version 2
//! superblock, version 1 B-tree chunk index, dimension scales), so ncdump,
//! xarray, GDAL and every HDF5 reader open the result as an ordinary
//! NetCDF-4 file.
//!
//! ```no_run
//! use nc4_stream::{AttrValue, DataType, Nc4Writer, VariableOptions};
//!
//! let mut file = Nc4Writer::create("W.nc")?;
//! let time = file.add_unlimited_dimension("time")?;
//! let y = file.add_dimension("y", 2)?;
//! let x = file.add_dimension("x", 3)?;
//! let w = file.add_variable(
//!     "W",
//!     DataType::F32,
//!     &[time, y, x],
//!     VariableOptions::chunked(vec![1, 2, 3]).shuffle(true).deflate(7),
//! )?;
//! file.put_var_attr(w, "units", AttrValue::Str("-".into()))?;
//! for step in 0..10 {
//!     file.append_record(w, &[step as f32; 6])?;
//! }
//! file.finish()?;
//! # Ok::<(), nc4_stream::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod checksum;
mod filters;
mod hdf5;
mod writer;

pub use writer::{DimId, EncodedRecord, Nc4Writer, VarId};

/// What can go wrong while writing.
#[derive(Debug)]
pub enum Error {
    /// The underlying file could not be written.
    Io(std::io::Error),
    /// The file definition is inconsistent (duplicate names, shape mismatch, ...).
    Definition(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(error) => write!(f, "NetCDF-4 I/O error: {error}"),
            Error::Definition(message) => write!(f, "NetCDF-4 definition error: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(error) => Some(error),
            Error::Definition(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Error::Definition(message)
    }
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Element type of a variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// 32-bit float (`NC_FLOAT`).
    F32,
    /// 64-bit float (`NC_DOUBLE`).
    F64,
    /// 32-bit signed integer (`NC_INT`).
    I32,
    /// 64-bit signed integer (`NC_INT64`).
    I64,
    /// 32-bit unsigned integer (`NC_UINT`).
    U32,
    /// unsigned byte (`NC_UBYTE`).
    U8,
}

impl DataType {
    /// Bytes per element.
    pub fn size(self) -> usize {
        match self {
            DataType::F32 | DataType::I32 | DataType::U32 => 4,
            DataType::F64 | DataType::I64 => 8,
            DataType::U8 => 1,
        }
    }

    /// The fill value netcdf-c uses when none is set.
    pub(crate) fn default_fill(self) -> Vec<u8> {
        match self {
            DataType::F32 => 9.969_21e36_f32.to_le_bytes().to_vec(),
            DataType::F64 => 9.969_209_968_386_869e36_f64.to_le_bytes().to_vec(),
            DataType::I32 => (-2_147_483_647_i32).to_le_bytes().to_vec(),
            DataType::I64 => (-9_223_372_036_854_775_806_i64).to_le_bytes().to_vec(),
            DataType::U32 => 4_294_967_295_u32.to_le_bytes().to_vec(),
            DataType::U8 => vec![255],
        }
    }

    pub(crate) fn datatype_message(self) -> Vec<u8> {
        match self {
            DataType::F32 => hdf5::datatype_float(4),
            DataType::F64 => hdf5::datatype_float(8),
            DataType::I32 => hdf5::datatype_fixed_point(4, true),
            DataType::I64 => hdf5::datatype_fixed_point(8, true),
            DataType::U32 => hdf5::datatype_fixed_point(4, false),
            DataType::U8 => hdf5::datatype_fixed_point(1, false),
        }
    }
}

/// An attribute value. Numeric scalars are stored as one-element arrays,
/// which is how netcdf-c stores them too.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrValue {
    /// Text (`NC_CHAR` array).
    Str(String),
    /// One float.
    F32(f32),
    /// One double.
    F64(f64),
    /// One int.
    I32(i32),
    /// One int64.
    I64(i64),
    /// One unsigned int.
    U32(u32),
    /// One unsigned int64.
    U64(u64),
    /// Float array.
    F32s(Vec<f32>),
    /// Double array.
    F64s(Vec<f64>),
    /// Int array.
    I32s(Vec<i32>),
    /// Int64 array.
    I64s(Vec<i64>),
}

impl From<&str> for AttrValue {
    fn from(value: &str) -> Self {
        AttrValue::Str(value.to_string())
    }
}

impl From<String> for AttrValue {
    fn from(value: String) -> Self {
        AttrValue::Str(value)
    }
}

mod sealed {
    pub trait Sealed {}
}

/// A value type that can be written as variable elements.
pub trait Element: Copy + sealed::Sealed {
    /// The variable type these elements belong to.
    const DATA_TYPE: DataType;
    /// Append the little-endian bytes of `self`.
    fn push_le(self, out: &mut Vec<u8>);
}

macro_rules! element {
    ($($ty:ty => $dt:expr),* $(,)?) => {
        $(
            impl sealed::Sealed for $ty {}
            impl Element for $ty {
                const DATA_TYPE: DataType = $dt;
                fn push_le(self, out: &mut Vec<u8>) {
                    out.extend_from_slice(&self.to_le_bytes());
                }
            }
        )*
    };
}

element! {
    f32 => DataType::F32,
    f64 => DataType::F64,
    i32 => DataType::I32,
    i64 => DataType::I64,
    u32 => DataType::U32,
    u8 => DataType::U8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FillValue {
    F32(f32),
    F64(f64),
    I32(i32),
    I64(i64),
    U32(u32),
    U8(u8),
}

impl FillValue {
    pub(crate) fn data_type(self) -> DataType {
        match self {
            FillValue::F32(_) => DataType::F32,
            FillValue::F64(_) => DataType::F64,
            FillValue::I32(_) => DataType::I32,
            FillValue::I64(_) => DataType::I64,
            FillValue::U32(_) => DataType::U32,
            FillValue::U8(_) => DataType::U8,
        }
    }

    pub(crate) fn bytes(self) -> Vec<u8> {
        match self {
            FillValue::F32(v) => v.to_le_bytes().to_vec(),
            FillValue::F64(v) => v.to_le_bytes().to_vec(),
            FillValue::I32(v) => v.to_le_bytes().to_vec(),
            FillValue::I64(v) => v.to_le_bytes().to_vec(),
            FillValue::U32(v) => v.to_le_bytes().to_vec(),
            FillValue::U8(v) => vec![v],
        }
    }

    pub(crate) fn attr_value(self) -> AttrValue {
        match self {
            FillValue::F32(v) => AttrValue::F32(v),
            FillValue::F64(v) => AttrValue::F64(v),
            FillValue::I32(v) => AttrValue::I32(v),
            FillValue::I64(v) => AttrValue::I64(v),
            FillValue::U32(v) => AttrValue::U32(v),
            FillValue::U8(v) => AttrValue::I32s(vec![i32::from(v)]),
        }
    }
}

/// Storage options of a variable.
#[derive(Clone, Debug)]
pub struct VariableOptions {
    pub(crate) chunk: Option<Vec<usize>>,
    pub(crate) deflate: Option<u32>,
    pub(crate) shuffle: bool,
    pub(crate) fill: Option<FillValue>,
}

impl VariableOptions {
    /// One unfiltered block, written whole with [`Nc4Writer::put_values`].
    /// Not allowed on variables with an unlimited dimension.
    pub fn contiguous() -> Self {
        Self {
            chunk: None,
            deflate: None,
            shuffle: false,
            fill: None,
        }
    }

    /// Chunked storage with the given chunk shape, one entry per dimension.
    /// On a variable with the unlimited dimension first, every other chunk
    /// extent must equal the variable's extent, so records are appended whole.
    pub fn chunked(chunk_shape: Vec<usize>) -> Self {
        Self {
            chunk: Some(chunk_shape),
            deflate: None,
            shuffle: false,
            fill: None,
        }
    }

    /// Deflate every chunk at `level` (1 to 9); 0 disables it.
    pub fn deflate(mut self, level: u32) -> Self {
        self.deflate = (level > 0).then_some(level.min(9));
        self
    }

    /// Byte-shuffle every chunk before deflating it.
    pub fn shuffle(mut self, enabled: bool) -> Self {
        self.shuffle = enabled;
        self
    }

    /// Fill value of an `F32` variable, also written as `_FillValue`.
    pub fn fill_value_f32(mut self, value: f32) -> Self {
        self.fill = Some(FillValue::F32(value));
        self
    }

    /// Fill value of an `F64` variable, also written as `_FillValue`.
    pub fn fill_value_f64(mut self, value: f64) -> Self {
        self.fill = Some(FillValue::F64(value));
        self
    }

    /// Fill value of an `I32` variable, also written as `_FillValue`.
    pub fn fill_value_i32(mut self, value: i32) -> Self {
        self.fill = Some(FillValue::I32(value));
        self
    }

    /// Fill value of an `I64` variable, also written as `_FillValue`.
    pub fn fill_value_i64(mut self, value: i64) -> Self {
        self.fill = Some(FillValue::I64(value));
        self
    }

    /// Fill value of a `U32` variable, also written as `_FillValue`.
    pub fn fill_value_u32(mut self, value: u32) -> Self {
        self.fill = Some(FillValue::U32(value));
        self
    }

    /// Fill value of a `U8` variable, also written as `_FillValue`.
    pub fn fill_value_u8(mut self, value: u8) -> Self {
        self.fill = Some(FillValue::U8(value));
        self
    }
}
