use std::path::PathBuf;

pub(crate) struct Config {
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
}
