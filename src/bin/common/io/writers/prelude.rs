use risico::models::output::Output;

use crate::common::{
    helpers::RISICOError,
    io::models::{grid::RegularGrid, output::OutputVariable},
};

/// Trait implemented by concrete output sinks (e.g. ZBIN, NetCDF) that persist model variables.
pub trait OutputSink: Send {
    fn write(
        &mut self,
        output: &Output,
        lats: &[f32],
        lons: &[f32],
        grid: &RegularGrid,
        variables: &[OutputVariable],
    ) -> Result<(), RISICOError>;
}
