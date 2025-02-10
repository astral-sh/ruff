#[derive(Debug, Eq, PartialOrd, Ord, PartialEq)]
pub(super) struct PythonVersion {
    major: u8,
    minor: u8,
}

impl PythonVersion {
    pub(super) const PY38: PythonVersion = PythonVersion { major: 3, minor: 8 };
    pub(super) const PY310: PythonVersion = PythonVersion {
        major: 3,
        minor: 10,
    };
}
