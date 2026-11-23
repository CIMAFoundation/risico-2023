pub mod models;
pub mod readers;
pub mod static_data;
pub mod streaming;
pub mod warm_state;
pub mod writers;
#[cfg(not(target_os = "wasi"))]
pub mod zarr;

/// Open a NetCDF file. Where the reader has no positioned file access
/// (WASI, the browser), the file is read into memory first.
pub fn open_netcdf(
    path: impl AsRef<std::path::Path>,
) -> Result<netcdf_reader::NcFile, netcdf_reader::Error> {
    #[cfg(any(unix, windows))]
    {
        netcdf_reader::NcFile::open(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        netcdf_reader::NcFile::from_vec(std::fs::read(path)?)
    }
}
