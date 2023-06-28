/// Enum to represent a Python platform.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PythonPlatform {
    Darwin,
    Linux,
    Windows,
}
