use std::path::Path;

use anyhow::Result;
use path_absolutize::path_dedot;
use pyo3::prelude::*;
use pyo3::types::PyString;
use pythonize::depythonize;
use rustpython_parser::lexer::LexResult;

use crate::checks::{Check, CheckCode};
use crate::lib_native::resolve;
use crate::linter::check_path;
use crate::rustpython_helpers::tokenize;
use crate::settings::configuration::Configuration;
use crate::settings::options::Options;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::{directives, packages};

#[pyclass]
#[derive(Clone)]
struct Location {
    #[pyo3(get)]
    row: usize,
    #[pyo3(get)]
    column: usize,
}

#[pyclass]
struct Message {
    #[pyo3(get)]
    code: CheckCode,
    #[pyo3(get)]
    message: String,
    #[pyo3(get)]
    location: Location,
    #[pyo3(get)]
    end_location: Location,
    // TODO(rgerecke): Include fix
}

// Using `#[pyclass]` on the `CheckCode` enum is incompatible with serde,
// because this generates unsafe code.
// TODO(rgerecke): Perhaps we want to generate module-level constants instead?
impl IntoPy<PyObject> for CheckCode {
    fn into_py(self, py: Python<'_>) -> PyObject {
        PyString::new(py, self.as_ref()).into()
    }
}

fn inner_check(
    contents: &str,
    path: Option<&Path>,
    options: Option<Options>,
) -> Result<Vec<Check>> {
    let filename = path.unwrap_or_else(|| Path::new("<filename>"));
    let path = path.unwrap_or(&path_dedot::CWD);

    let settings = match options {
        Some(opt) => Settings::from_configuration(Configuration::from_options(opt, path)?, path)?,
        None => resolve(path)?,
    };

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = SourceCodeLocator::new(contents);

    // Detect the current code style (lazily).
    let stylist = SourceCodeStyleDetector::from_contents(contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(&tokens, &locator, directives::Flags::empty());

    // Generate checks.
    let checks = check_path(
        filename,
        packages::detect_package_root(path),
        contents,
        tokens,
        &locator,
        &stylist,
        &directives,
        &settings,
        flags::Autofix::Enabled,
        flags::Noqa::Enabled,
    )?;

    Ok(checks)
}

#[pyfunction]
fn check(contents: &str, path: Option<&str>, options: Option<&PyAny>) -> PyResult<Vec<Message>> {
    let path = path.map(Path::new);
    let options = match options {
        Some(v) => depythonize(v)?,
        None => None,
    };

    Ok(inner_check(contents, path, options).map(|r| {
        r.iter()
            .map(|check| Message {
                code: check.kind.code().clone(),
                message: check.kind.body(),
                location: Location {
                    row: check.location.row(),
                    column: check.location.column(),
                },
                end_location: Location {
                    row: check.end_location.row(),
                    column: check.end_location.column(),
                },
            })
            .collect::<Vec<_>>()
    })?)
}

#[pymodule]
pub fn _ruff(_: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_class::<Message>()?;
    Ok(())
}
