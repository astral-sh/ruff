use std::path::PathBuf;

use crate::python_platform::PythonPlatform;
use crate::python_version::PythonVersion;

#[derive(Debug)]
pub(crate) struct ExecutionEnvironment {
    /// The root directory of the execution environment.
    pub(crate) root: PathBuf,

    /// The Python version of the execution environment.
    pub(crate) python_version: PythonVersion,

    /// The Python platform of the execution environment.
    pub(crate) python_platform: PythonPlatform,

    /// The extra search paths of the execution environment.
    pub(crate) extra_paths: Vec<PathBuf>,
}
