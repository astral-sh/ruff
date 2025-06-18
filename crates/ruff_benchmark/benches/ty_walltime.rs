use std::fmt::{Display, Formatter};

use divan::{Bencher, bench};

use rayon::ThreadPoolBuilder;
use ruff_benchmark::real_world_projects::{RealWorldProject, SetupProject};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};

use ruff_db::testing::setup_logging_with_filter;
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

struct Benchmark<'a> {
    project: SetupProject<'a>,
    max_diagnostics: usize,
}

impl<'a> Benchmark<'a> {
    fn new(project: RealWorldProject<'a>, max_diagnostics: usize) -> Self {
        let setup_project = project.setup().expect("Failed to setup project");

        Self {
            project: setup_project,
            max_diagnostics,
        }
    }

    fn setup_iteration(&self) -> ProjectDatabase {
        let root = SystemPathBuf::from_path_buf(self.project.path.clone()).unwrap();
        let system = OsSystem::new(&root);

        let mut metadata = ProjectMetadata::discover(&root, &system).unwrap();

        metadata.apply_options(Options {
            environment: Some(EnvironmentOptions {
                python_version: Some(RangedValue::cli(self.project.config.python_version)),
                python: (!self.project.config().dependencies.is_empty())
                    .then_some(RelativePathBuf::cli(SystemPath::new(".venv"))),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        let mut db = ProjectDatabase::new(metadata, system).unwrap();

        db.project().set_included_paths(
            &mut db,
            self.project
                .check_paths()
                .iter()
                .map(|path| SystemPath::absolute(path, &root))
                .collect(),
        );
        db
    }
}

impl Display for Benchmark<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.project.config.name)
    }
}

fn check_project(db: &ProjectDatabase, max_diagnostics: usize) {
    let result = db.check();
    let diagnostics = result.len();

    assert!(
        diagnostics > 1 && diagnostics <= max_diagnostics,
        "Expected between {} and {} diagnostics but got {}",
        1,
        max_diagnostics,
        diagnostics
    );
}

static ALTAIR: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "altair",
            repository: "https://github.com/vega/altair",
            commit: "d1f4a1ef89006e5f6752ef1f6df4b7a509336fba",
            paths: vec![SystemPath::new("altair")],
            dependencies: vec![
                "jinja2",
                "narwhals",
                "numpy",
                "packaging",
                "pandas-stubs",
                "pyarrow-stubs",
                "pytest",
                "scipy-stubs",
                "types-jsonschema",
            ],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY312,
        },
        1000,
    )
});

static COLOUR_SCIENCE: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "colour-science",
            repository: "https://github.com/colour-science/colour",
            commit: "a17e2335c29e7b6f08080aa4c93cfa9b61f84757",
            paths: vec![SystemPath::new("colour")],
            dependencies: vec![
                "matplotlib",
                "numpy",
                "pandas-stubs",
                "pytest",
                "scipy-stubs",
            ],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY310,
        },
        477,
    )
});

static FREQTRADE: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "freqtrade",
            repository: "https://github.com/freqtrade/freqtrade",
            commit: "2d842ea129e56575852ee0c45383c8c3f706be19",
            paths: vec![SystemPath::new("freqtrade")],
            dependencies: vec![
                "numpy",
                "pandas-stubs",
                "pydantic",
                "sqlalchemy",
                "types-cachetools",
                "types-filelock",
                "types-python-dateutil",
                "types-requests",
                "types-tabulate",
            ],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY312,
        },
        400,
    )
});

static PANDAS: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "pandas",
            repository: "https://github.com/pandas-dev/pandas",
            commit: "5909621e2267eb67943a95ef5e895e8484c53432",
            paths: vec![SystemPath::new("pandas")],
            dependencies: vec![
                "numpy",
                "types-python-dateutil",
                "types-pytz",
                "types-PyMySQL",
                "types-setuptools",
                "pytest",
            ],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY312,
        },
        3000,
    )
});

static PYDANTIC: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "pydantic",
            repository: "https://github.com/pydantic/pydantic",
            commit: "0c4a22b64b23dfad27387750cf07487efc45eb05",
            paths: vec![SystemPath::new("pydantic")],
            dependencies: vec![
                "annotated-types",
                "pydantic-core",
                "typing-extensions",
                "typing-inspection",
            ],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY39,
        },
        1000,
    )
});

static SYMPY: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "sympy",
            repository: "https://github.com/sympy/sympy",
            commit: "22fc107a94eaabc4f6eb31470b39db65abb7a394",
            paths: vec![SystemPath::new("sympy")],
            dependencies: vec!["mpmath"],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY312,
        },
        13000,
    )
});

#[bench(args=[&*ALTAIR, &*FREQTRADE, &*PYDANTIC], sample_size=2, sample_count=3)]
fn small(bencher: Bencher, benchmark: &Benchmark) {
    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_refs(|db| {
            check_project(db, benchmark.max_diagnostics);
        });
}

#[bench(args=[&*COLOUR_SCIENCE, &*PANDAS], sample_size=1, sample_count=3)]
fn medium(bencher: Bencher, benchmark: &Benchmark) {
    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_refs(|db| {
            check_project(db, benchmark.max_diagnostics);
        });
}

#[bench(args=[&*SYMPY], sample_size=1, sample_count=2)]
fn large(bencher: Bencher, benchmark: &Benchmark) {
    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_refs(|db| {
            check_project(db, benchmark.max_diagnostics);
        });
}

fn main() {
    let filter =
        std::env::var("TY_LOG").unwrap_or("ty_walltime=info,ruff_benchmark=info".to_string());

    let _logging = setup_logging_with_filter(&filter).expect("Filter to be valid");

    // Disable multithreading for now due to
    // https://github.com/salsa-rs/salsa/issues/918.
    //
    // Salsa has a fast-path for the first db when looking up ingredients.
    // It seems that this fast-path becomes extremelly slow for all db's other
    // than the first one, especially when using multithreading (10x slower than the first run).
    tracing::info!(
        "Pre-warm Salsa running Altair, see https://github.com/salsa-rs/salsa/issues/918"
    );
    ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .unwrap();

    divan::main();
}
