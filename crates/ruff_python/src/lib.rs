//! Python bindings for Ruff's formatting API.
//!
//! Exposes `ruff.format()` as a native Python function via PyO3,
//! allowing downstream tools to format Python code programmatically
//! without shelling out to the CLI.

use std::path::Path;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use ruff_python_ast::PySourceType;
use ruff_python_formatter::{PyFormatOptions, format_module_source};

/// Format Python source code using Ruff's formatter.
///
/// Args:
///     source: The Python source code to format.
///     filename: Optional filename hint used to determine source type
///         (e.g. `.pyi` files use stub formatting rules). Defaults to
///         standard `.py` formatting when not provided.
///
/// Returns:
///     The formatted source code as a string.
///
/// Raises:
///     ValueError: If the source code contains a syntax error or
///         formatting otherwise fails.
///
/// Example:
///     >>> import ruff
///     >>> ruff.format("x = 1+2\n")
///     'x = 1 + 2\n'
#[pyfunction]
#[pyo3(signature = (source, *, filename=None))]
fn format(source: &str, filename: Option<&str>) -> PyResult<String> {
    let source_type = match filename {
        Some(name) => PySourceType::from(Path::new(name)),
        None => PySourceType::default(),
    };

    let options = PyFormatOptions::from_source_type(source_type);

    match format_module_source(source, options) {
        Ok(printed) => Ok(printed.into_code()),
        Err(err) => Err(PyValueError::new_err(format!(
            "Failed to format source: {err}"
        ))),
    }
}

/// Ruff Python API module.
///
/// Provides native access to Ruff's formatting capabilities without
/// requiring subprocess invocation.
#[pymodule]
fn _ruff_api(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(format, m)?)?;
    Ok(())
}
