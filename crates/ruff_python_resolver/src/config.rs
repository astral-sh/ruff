use std::path::PathBuf;

use crate::python_version::PythonVersion;

pub(crate) struct Config {
    /// Path to python interpreter.
    pub(crate) python_path: Option<PathBuf>,

    /// Path to use for typeshed definitions.
    pub(crate) typeshed_path: Option<PathBuf>,

    /// Path to custom typings (stub) modules.
    pub(crate) stub_path: Option<PathBuf>,

    /// Path to a directory containing one or more virtual environment
    /// directories. This is used in conjunction with the "venv" name in
    /// the config file to identify the python environment used for resolving
    /// third-party modules.
    pub(crate) venv_path: Option<PathBuf>,

    /// Default venv environment.
    pub(crate) venv: Option<PathBuf>,

    /// Default Python version. Can be overridden by ExecutionEnvironment.
    pub(crate) default_python_version: Option<PythonVersion>,
}
