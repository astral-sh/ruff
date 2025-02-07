use std::path::PathBuf;

pub mod criterion;

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

/// Relative size of a test case. Benchmarks can use it to configure the time for how long a benchmark should run to get stable results.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TestCaseSpeed {
    /// A test case that is fast to run
    Fast,

    /// A normal test case
    Normal,

    /// A slow test case
    Slow,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    file: TestFile,
    speed: TestCaseSpeed,
}

impl TestCase {
    pub const fn fast(file: TestFile) -> Self {
        Self {
            file,
            speed: TestCaseSpeed::Fast,
        }
    }

    pub const fn normal(file: TestFile) -> Self {
        Self {
            file,
            speed: TestCaseSpeed::Normal,
        }
    }

    pub const fn slow(file: TestFile) -> Self {
        Self {
            file,
            speed: TestCaseSpeed::Slow,
        }
    }

    pub fn code(&self) -> &str {
        self.file.code
    }

    pub fn name(&self) -> &str {
        self.file.name
    }

    pub fn speed(&self) -> TestCaseSpeed {
        self.speed
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
}
