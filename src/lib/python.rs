use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Datelike, Timelike, Utc};
use ndarray::Array1;
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyDateTime, PyModule},
    wrap_pyfunction, Bound,
};
use strum::IntoEnumIterator;

use crate::{
    constants::NODATAVAL,
    models::{
        input::{Input, InputElement},
        output::{Output, OutputVariableName},
    },
    modules::risico::{
        config::RISICOModelConfig,
        models::{
            RISICOCellPropertiesContainer, RISICOProperties, RISICOState, RISICOVegetation,
            RISICOWarmState,
        },
    },
};

#[pyclass(name = "Vegetation")]
pub struct PyVegetation {
    inner: Arc<RISICOVegetation>,
}

#[pymethods]
impl PyVegetation {
    #[new]
    #[pyo3(
        signature = (id, d0=None, d1=None, hhv=None, umid=None, v0=None, t0=None, sat=None, name=None, use_ndvi=None)
    )]
    fn new(
        id: String,
        d0: Option<f32>,
        d1: Option<f32>,
        hhv: Option<f32>,
        umid: Option<f32>,
        v0: Option<f32>,
        t0: Option<f32>,
        sat: Option<f32>,
        name: Option<String>,
        use_ndvi: Option<bool>,
    ) -> Self {
        let mut vegetation = RISICOVegetation::default();
        vegetation.id = id.clone();
        if let Some(value) = d0 {
            vegetation.d0 = value;
        }
        if let Some(value) = d1 {
            vegetation.d1 = value;
        }
        if let Some(value) = hhv {
            vegetation.hhv = value;
        }
        if let Some(value) = umid {
            vegetation.umid = value;
        }
        if let Some(value) = v0 {
            vegetation.v0 = value;
        }
        if let Some(value) = t0 {
            vegetation.T0 = value;
        }
        if let Some(value) = sat {
            vegetation.sat = value;
        }
        if let Some(flag) = use_ndvi {
            vegetation.use_ndvi = flag;
        }
        vegetation.name = name.unwrap_or_else(|| id.clone());
        Self {
            inner: Arc::new(vegetation),
        }
    }

    #[getter]
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }
}

#[pyclass(name = "WarmState")]
#[derive(Clone)]
pub struct PyWarmState {
    inner: RISICOWarmState,
}

#[pymethods]
impl PyWarmState {
    #[new]
    #[allow(non_snake_case)]
    #[pyo3(
        signature = (
            dffm=None,
            snow_cover=None,
            snow_cover_time=None,
            msi=None,
            msi_ttl=None,
            ndvi=None,
            ndvi_time=None,
            ndwi=None,
            ndwi_time=None
        )
    )]
    fn new(
        dffm: Option<f32>,
        snow_cover: Option<f32>,
        snow_cover_time: Option<f32>,
        msi: Option<f32>,
        msi_ttl: Option<f32>,
        ndvi: Option<f32>,
        ndvi_time: Option<f32>,
        ndwi: Option<f32>,
        ndwi_time: Option<f32>,
    ) -> Self {
        let mut inner = RISICOWarmState::default();
        if let Some(value) = dffm {
            inner.dffm = value;
        }
        if let Some(value) = snow_cover {
            inner.snow_cover = value;
        }
        if let Some(value) = snow_cover_time {
            inner.snow_cover_time = value;
        }
        if let Some(value) = msi {
            inner.MSI = value;
        }
        if let Some(value) = msi_ttl {
            inner.MSI_TTL = value;
        }
        if let Some(value) = ndvi {
            inner.NDVI = value;
        }
        if let Some(value) = ndvi_time {
            inner.NDVI_TIME = value;
        }
        if let Some(value) = ndwi {
            inner.NDWI = value;
        }
        if let Some(value) = ndwi_time {
            inner.NDWI_TIME = value;
        }
        Self { inner }
    }
}

#[pyclass(name = "InputElement")]
pub struct PyInputElement {
    inner: InputElement,
}

#[pymethods]
impl PyInputElement {
    #[new]
    #[pyo3(
        signature = (
            temperature=None,
            rain=None,
            wind_speed=None,
            wind_dir=None,
            humidity=None,
            snow_cover=None,
            temp_dew=None,
            vpd=None,
            ndvi=None,
            ndwi=None,
            msi=None,
            swi=None
        )
    )]
    fn new(
        temperature: Option<f32>,
        rain: Option<f32>,
        wind_speed: Option<f32>,
        wind_dir: Option<f32>,
        humidity: Option<f32>,
        snow_cover: Option<f32>,
        temp_dew: Option<f32>,
        vpd: Option<f32>,
        ndvi: Option<f32>,
        ndwi: Option<f32>,
        msi: Option<f32>,
        swi: Option<f32>,
    ) -> Self {
        let mut inner = InputElement::default();
        if let Some(value) = temperature {
            inner.temperature = value;
        }
        if let Some(value) = rain {
            inner.rain = value;
        }
        if let Some(value) = wind_speed {
            inner.wind_speed = value;
        }
        if let Some(value) = wind_dir {
            inner.wind_dir = value;
        }
        if let Some(value) = humidity {
            inner.humidity = value;
        }
        if let Some(value) = snow_cover {
            inner.snow_cover = value;
        }
        if let Some(value) = temp_dew {
            inner.temp_dew = value;
        }
        if let Some(value) = vpd {
            inner.vpd = value;
        }
        if let Some(value) = ndvi {
            inner.ndvi = value;
        }
        if let Some(value) = ndwi {
            inner.ndwi = value;
        }
        if let Some(value) = msi {
            inner.msi = value;
        }
        if let Some(value) = swi {
            inner.swi = value;
        }
        Self { inner }
    }
}

impl PyInputElement {
    fn inner(&self) -> InputElement {
        InputElement {
            temperature: self.inner.temperature,
            rain: self.inner.rain,
            wind_speed: self.inner.wind_speed,
            wind_dir: self.inner.wind_dir,
            humidity: self.inner.humidity,
            snow_cover: self.inner.snow_cover,
            temp_dew: self.inner.temp_dew,
            vpd: self.inner.vpd,
            ndvi: self.inner.ndvi,
            ndwi: self.inner.ndwi,
            msi: self.inner.msi,
            swi: self.inner.swi,
        }
    }
}

#[pyclass(name = "Input")]
pub struct PyInput {
    time: DateTime<Utc>,
    elements: Vec<InputElement>,
}

#[pymethods]
impl PyInput {
    #[new]
    #[pyo3(signature = (time, elements))]
    fn new(
        py: Python<'_>,
        time: DateTime<Utc>,
        elements: Vec<Py<PyInputElement>>,
    ) -> PyResult<Self> {
        if elements.is_empty() {
            return Err(PyValueError::new_err(
                "Input requires at least one input element",
            ));
        }

        let inputs = elements
            .into_iter()
            .map(|elem| elem.borrow(py).inner())
            .collect();

        Ok(Self {
            time,
            elements: inputs,
        })
    }

    fn len(&self) -> usize {
        self.elements.len()
    }

    fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    fn time<'py>(&self, py: Python<'py>) -> PyResult<Py<PyDateTime>> {
        datetime_to_py(py, &self.time)
    }
}

impl PyInput {
    fn to_input(&self) -> Input {
        let cloned = self
            .elements
            .iter()
            .map(|elem| InputElement {
                temperature: elem.temperature,
                rain: elem.rain,
                wind_speed: elem.wind_speed,
                wind_dir: elem.wind_dir,
                humidity: elem.humidity,
                snow_cover: elem.snow_cover,
                temp_dew: elem.temp_dew,
                vpd: elem.vpd,
                ndvi: elem.ndvi,
                ndwi: elem.ndwi,
                msi: elem.msi,
                swi: elem.swi,
            })
            .collect();
        Input {
            time: self.time,
            data: Array1::from_vec(cloned),
        }
    }
}

#[pyclass(name = "Properties")]
pub struct PyProperties {
    pub(crate) inner: RISICOProperties,
}

#[pymethods]
impl PyProperties {
    #[new]
    #[pyo3(
        signature = (
            lons,
            lats,
            slopes,
            aspects,
            vegetations,
            ppf_summer=None,
            ppf_winter=None,
            vegetation_defs=None
        )
    )]
    fn new(
        py: Python<'_>,
        lons: Vec<f32>,
        lats: Vec<f32>,
        slopes: Vec<f32>,
        aspects: Vec<f32>,
        vegetations: Vec<String>,
        ppf_summer: Option<Vec<f32>>,
        ppf_winter: Option<Vec<f32>>,
        vegetation_defs: Option<Vec<Py<PyVegetation>>>,
    ) -> PyResult<Self> {
        let len = lons.len();
        if len == 0 {
            return Err(PyValueError::new_err(
                "Properties require at least one cell definition",
            ));
        }

        if lats.len() != len
            || slopes.len() != len
            || aspects.len() != len
            || vegetations.len() != len
        {
            return Err(PyValueError::new_err(
                "All property vectors must share the same length",
            ));
        }

        let ppf_summer = ppf_summer.unwrap_or_else(|| vec![1.0; len]);
        let ppf_winter = ppf_winter.unwrap_or_else(|| vec![1.0; len]);

        if ppf_summer.len() != len || ppf_winter.len() != len {
            return Err(PyValueError::new_err(
                "PPF summer and winter vectors must match coordinates length",
            ));
        }

        let mut vegetation_map: HashMap<String, Arc<RISICOVegetation>> = HashMap::new();
        if let Some(defs) = vegetation_defs {
            for item in defs {
                let veg = item.borrow(py);
                vegetation_map.insert(veg.inner.id.clone(), veg.inner.clone());
            }
        }

        let container = RISICOCellPropertiesContainer {
            lons,
            lats,
            slopes,
            aspects,
            vegetations,
        };

        let props = RISICOProperties::new(container, vegetation_map, ppf_summer, ppf_winter);

        Ok(Self { inner: props })
    }

    fn len(&self) -> usize {
        self.inner.len
    }

    fn coordinates(&self) -> (Vec<f32>, Vec<f32>) {
        self.inner.get_coords()
    }
}

#[pyclass(name = "State")]
pub struct PyState {
    inner: RISICOState,
}

#[pymethods]
impl PyState {
    #[new]
    #[pyo3(signature = (warm_states, time, config="v2023"))]
    fn new(
        py: Python<'_>,
        warm_states: Vec<Py<PyWarmState>>,
        time: DateTime<Utc>,
        config: &str,
    ) -> PyResult<Self> {
        if warm_states.is_empty() {
            return Err(PyValueError::new_err(
                "State creation requires at least one warm state element",
            ));
        }

        let warm_data: Vec<RISICOWarmState> = warm_states
            .into_iter()
            .map(|state| state.borrow(py).inner.clone())
            .collect();

        let config = RISICOModelConfig::new(config);
        let state = RISICOState::new(&warm_data, &time, config);
        Ok(Self { inner: state })
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn update(&mut self, props: &PyProperties, input: &PyInput) {
        let input_inner = input.to_input();
        self.inner.update(&props.inner, &input_inner);
    }

    fn output(&self, props: &PyProperties, input: &PyInput) -> PyOutput {
        let input_inner = input.to_input();
        let output = self.inner.output(&props.inner, &input_inner);
        PyOutput { inner: output }
    }
}

#[pyclass(name = "Output")]
pub struct PyOutput {
    inner: Output,
}

#[pymethods]
impl PyOutput {
    fn len(&self) -> usize {
        self.inner.data.len()
    }

    fn time<'py>(&self, py: Python<'py>) -> PyResult<Py<PyDateTime>> {
        datetime_to_py(py, &self.inner.time)
    }

    fn get(&self, name: &str) -> PyResult<Option<Vec<f32>>> {
        let variable = name
            .parse::<OutputVariableName>()
            .map_err(|_| PyValueError::new_err(format!("Unknown output variable '{name}'")))?;
        Ok(self.inner.get(&variable).map(|array| array.to_vec()))
    }

    fn available_variables(&self) -> Vec<String> {
        OutputVariableName::iter().map(|v| v.to_string()).collect()
    }
}

#[pyfunction]
fn available_output_variables() -> Vec<String> {
    OutputVariableName::iter().map(|v| v.to_string()).collect()
}

#[pyfunction]
fn nodata_value() -> f32 {
    NODATAVAL
}

fn datetime_to_py(py: Python<'_>, dt: &DateTime<Utc>) -> PyResult<Py<PyDateTime>> {
    let bound = PyDateTime::new(
        py,
        dt.year(),
        dt.month() as u8,
        dt.day() as u8,
        dt.hour() as u8,
        dt.minute() as u8,
        dt.second() as u8,
        dt.timestamp_subsec_micros(),
        None,
    )?;
    Ok(bound.into())
}

#[pymodule]
fn risico_py(py: Python<'_>, module: Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyVegetation>()?;
    module.add_class::<PyWarmState>()?;
    module.add_class::<PyInputElement>()?;
    module.add_class::<PyInput>()?;
    module.add_class::<PyProperties>()?;
    module.add_class::<PyState>()?;
    module.add_class::<PyOutput>()?;

    module.add_function(wrap_pyfunction!(available_output_variables, py)?)?;
    module.add_function(wrap_pyfunction!(nodata_value, py)?)?;

    module.add("RISICO_VERSION", crate::version::FULL_VERSION)?;
    Ok(())
}
