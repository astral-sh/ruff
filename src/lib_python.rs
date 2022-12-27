use pyo3::prelude::*;

#[pymodule]
pub fn _ruff(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    Ok(())
}
