use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct Config {
    /// Path to use for typeshed definitions.
    pub typeshed_path: Option<PathBuf>,

    /// Path to custom typings (stub) modules.
    pub stub_path: Option<PathBuf>,

    /// Path to a directory containing one or more virtual environment
    /// directories. This is used in conjunction with the "venv" name in
    /// the config file to identify the python environment used for resolving
    /// third-party modules.
    pub venv_path: Option<PathBuf>,

    /// Default venv environment.
    pub venv: Option<PathBuf>,
}
