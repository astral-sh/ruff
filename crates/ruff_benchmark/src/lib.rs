use std::path::PathBuf;

#[cfg(any(feature = "ruff_instrumented", feature = "ty_instrumented"))]
pub mod criterion;
#[cfg(any(feature = "ty_instrumented", feature = "ty_walltime"))]
pub mod real_world_projects;

pub static NUMPY_GLOBALS: TestFile = TestFile::new(
    "numpy/globals.py",
    include_str!("../resources/numpy/globals.py"),
);

pub static UNICODE_PYPINYIN: TestFile = TestFile::new(
    "unicode/pypinyin.py",
    include_str!("../resources/pypinyin.py"),
);

pub static PYDANTIC_TYPES: TestFile = TestFile::new(
    "pydantic/types.py",
    include_str!("../resources/pydantic/types.py"),
);

pub static NUMPY_CTYPESLIB: TestFile = TestFile::new(
    "numpy/ctypeslib.py",
    include_str!("../resources/numpy/ctypeslib.py"),
);

// "https://raw.githubusercontent.com/DHI/mikeio/b7d26418f4db2909b0aa965253dbe83194d7bb5b/tests/test_dataset.py"
pub static LARGE_DATASET: TestFile = TestFile::new(
    "large/dataset.py",
    include_str!("../resources/large/dataset.py"),
);

#[derive(Debug, Clone)]
pub struct TestFile {
    name: &'static str,
    code: &'static str,
}

impl TestFile {
    pub const fn new(name: &'static str, code: &'static str) -> Self {
        Self { name, code }
    }

    pub fn code(&self) -> &str {
        self.code
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(file!())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("resources")
            .join(self.name())
    }
}
