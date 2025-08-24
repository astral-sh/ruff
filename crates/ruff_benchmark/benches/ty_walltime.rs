use std::fmt::{Display, Formatter};

use divan::{Bencher, bench};

use rayon::ThreadPoolBuilder;
use ruff_benchmark::real_world_projects::{InstalledProject, RealWorldProject};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};

use ruff_db::testing::setup_logging_with_filter;
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

struct Benchmark<'a> {
    project: InstalledProject<'a>,
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
                python: Some(RelativePathBuf::cli(SystemPath::new(".venv"))),
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

static TANJUN: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "tanjun",
            repository: "https://github.com/FasterSpeeding/Tanjun",
            commit: "69f40db188196bc59516b6c69849c2d85fbc2f4a",
            paths: vec![SystemPath::new("tanjun")],
            dependencies: vec!["hikari", "alluka"],
            max_dep_date: "2025-06-17",
            python_version: PythonVersion::PY312,
        },
        100,
    )
});

static STATIC_FRAME: std::sync::LazyLock<Benchmark<'static>> = std::sync::LazyLock::new(|| {
    Benchmark::new(
        RealWorldProject {
            name: "static-frame",
            repository: "https://github.com/static-frame/static-frame",
            commit: "34962b41baca5e7f98f5a758d530bff02748a421",
            paths: vec![SystemPath::new("static_frame")],
            // N.B. `arraykit` is installed as a dependency during mypy_primer runs,
            // but it takes much longer to be installed in a Codspeed run than it does in a mypy_primer run
            // (seems to be built from source on the Codspeed CI runners for some reason).
            dependencies: vec!["numpy"],
            max_dep_date: "2025-08-09",
            python_version: PythonVersion::PY311,
        },
        500,
    )
});

#[track_caller]
fn run_single_threaded(bencher: Bencher, benchmark: &Benchmark) {
    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_refs(|db| {
            check_project(db, benchmark.max_diagnostics);
        });
}

#[bench(args=[&*ALTAIR, &*FREQTRADE, &*PYDANTIC, &*TANJUN], sample_size=2, sample_count=3)]
fn small(bencher: Bencher, benchmark: &Benchmark) {
    run_single_threaded(bencher, benchmark);
}

#[bench(args=[&*COLOUR_SCIENCE, &*PANDAS, &*STATIC_FRAME], sample_size=1, sample_count=3)]
fn medium(bencher: Bencher, benchmark: &Benchmark) {
    run_single_threaded(bencher, benchmark);
}

#[bench(args=[&*SYMPY], sample_size=1, sample_count=2)]
fn large(bencher: Bencher, benchmark: &Benchmark) {
    run_single_threaded(bencher, benchmark);
}

#[bench(args=[&*PYDANTIC], sample_size=3, sample_count=8)]
fn multithreaded(bencher: Bencher, benchmark: &Benchmark) {
    let thread_pool = ThreadPoolBuilder::new().build().unwrap();

    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_values(|db| {
            thread_pool.install(|| {
                check_project(&db, benchmark.max_diagnostics);
                db
            })
        });
}

fn main() {
    ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .unwrap();

    let filter =
        std::env::var("TY_LOG").unwrap_or("ty_walltime=info,ruff_benchmark=info".to_string());

    let _logging = setup_logging_with_filter(&filter).expect("Filter to be valid");

    // Salsa uses an optimized lookup for the ingredient index when using only a single database.
    // This optimization results in at least a 10% speedup compared to when using multiple databases.
    // To reduce noise, run one benchmark so that all benchmarks take the less optimized "not the first db"
    // branch when looking up the ingredient index.
    {
        let db = TANJUN.setup_iteration();
        check_project(&db, TANJUN.max_diagnostics);
    }

    divan::main();
}
