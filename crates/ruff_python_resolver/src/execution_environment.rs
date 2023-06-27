use std::path::PathBuf;

use crate::python_platform::PythonPlatform;
use crate::python_version::PythonVersion;

#[derive(Debug)]
pub struct ExecutionEnvironment {
    /// The root directory of the execution environment.
    pub root: PathBuf,

    /// The Python version of the execution environment.
    pub python_version: PythonVersion,

    /// The Python platform of the execution environment.
    pub python_platform: PythonPlatform,

    /// The extra search paths of the execution environment.
    pub extra_paths: Vec<PathBuf>,
}
