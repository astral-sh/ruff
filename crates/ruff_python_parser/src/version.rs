#[derive(Clone, Copy, Debug, Eq, PartialOrd, Ord, PartialEq)]
pub struct PythonVersion {
    pub major: u8,
    pub minor: u8,
}

impl PythonVersion {
    pub const PY38: PythonVersion = PythonVersion { major: 3, minor: 8 };
    pub const PY310: PythonVersion = PythonVersion {
        major: 3,
        minor: 10,
    };
}
