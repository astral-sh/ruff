use divan::{Bencher, bench};
use std::fmt::{Display, Formatter};

use rayon::ThreadPoolBuilder;
use ruff_benchmark::real_world_projects::{InstalledProject, RealWorldProject};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};

use ruff_db::testing::setup_logging_with_filter;
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

struct Benchmark<'a> {
    project: RealWorldProject<'a>,
    installed_project: std::sync::OnceLock<InstalledProject<'a>>,
    max_diagnostics: usize,
}

impl<'a> Benchmark<'a> {
    const fn new(project: RealWorldProject<'a>, max_diagnostics: usize) -> Self {
        Self {
            project,
            installed_project: std::sync::OnceLock::new(),
            max_diagnostics,
        }
    }

    fn installed_project(&self) -> &InstalledProject<'a> {
        self.installed_project.get_or_init(|| {
            self.project
                .clone()
                .setup()
                .expect("Failed to setup project")
        })
    }

    fn setup_iteration(&self) -> ProjectDatabase {
        let installed_project = self.installed_project();
        let root = SystemPathBuf::from_path_buf(installed_project.path.clone()).unwrap();
        let system = OsSystem::new(&root);

        let mut metadata = ProjectMetadata::discover(&root, &system).unwrap();

        metadata.apply_options(Options {
            environment: Some(EnvironmentOptions {
                python_version: Some(RangedValue::cli(installed_project.config.python_version)),
                python: Some(RelativePathBuf::cli(SystemPath::new(".venv"))),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        let mut db = ProjectDatabase::new(metadata, system).unwrap();

        db.project().set_included_paths(
            &mut db,
            installed_project
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
        self.project.name.fmt(f)
    }
}

fn check_project(db: &ProjectDatabase, project_name: &str, max_diagnostics: usize) {
    let result = db.check();
    let diagnostics = result.len();

    assert!(
        diagnostics > 1 && diagnostics <= max_diagnostics,
        "Expected between 1 and {max_diagnostics} diagnostics on project '{project_name}' but got {diagnostics}",
    );
}

static ALTAIR: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "altair",
        repository: "https://github.com/vega/altair",
        commit: "d1f4a1ef89006e5f6752ef1f6df4b7a509336fba",
        paths: &["altair"],
        dependencies: &[
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
);

static COLOUR_SCIENCE: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "colour-science",
        repository: "https://github.com/colour-science/colour",
        commit: "a17e2335c29e7b6f08080aa4c93cfa9b61f84757",
        paths: &["colour"],
        dependencies: &[
            "matplotlib",
            "numpy",
            "pandas-stubs",
            "pytest",
            "scipy-stubs",
        ],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY310,
    },
    1070,
);

static FREQTRADE: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "freqtrade",
        repository: "https://github.com/freqtrade/freqtrade",
        commit: "2d842ea129e56575852ee0c45383c8c3f706be19",
        paths: &["freqtrade"],
        dependencies: &[
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
    600,
);

static PANDAS: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "pandas",
        repository: "https://github.com/pandas-dev/pandas",
        commit: "5909621e2267eb67943a95ef5e895e8484c53432",
        paths: &["pandas"],
        dependencies: &[
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
    4000,
);

static PYDANTIC: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "pydantic",
        repository: "https://github.com/pydantic/pydantic",
        commit: "0c4a22b64b23dfad27387750cf07487efc45eb05",
        paths: &["pydantic"],
        dependencies: &[
            "annotated-types",
            "pydantic-core",
            "typing-extensions",
            "typing-inspection",
        ],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY39,
    },
    7000,
);

static SYMPY: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "sympy",
        repository: "https://github.com/sympy/sympy",
        commit: "22fc107a94eaabc4f6eb31470b39db65abb7a394",
        paths: &["sympy"],
        dependencies: &["mpmath"],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY312,
    },
    13000,
);

static TANJUN: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "tanjun",
        repository: "https://github.com/FasterSpeeding/Tanjun",
        commit: "69f40db188196bc59516b6c69849c2d85fbc2f4a",
        paths: &["tanjun"],
        dependencies: &["hikari", "alluka"],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY312,
    },
    320,
);

static STATIC_FRAME: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "static-frame",
        repository: "https://github.com/static-frame/static-frame",
        commit: "34962b41baca5e7f98f5a758d530bff02748a421",
        paths: &["static_frame"],
        // N.B. `arraykit` is installed as a dependency during mypy_primer runs,
        // but it takes much longer to be installed in a Codspeed run than it does in a mypy_primer run
        // (seems to be built from source on the Codspeed CI runners for some reason).
        dependencies: &["numpy"],
        max_dep_date: "2025-08-09",
        python_version: PythonVersion::PY311,
    },
    950,
);

#[track_caller]
fn run_single_threaded(bencher: Bencher, benchmark: &Benchmark) {
    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_refs(|db| {
            check_project(db, benchmark.project.name, benchmark.max_diagnostics);
        });
}

#[bench(args=[&ALTAIR, &FREQTRADE, &TANJUN], sample_size=2, sample_count=3)]
fn small(bencher: Bencher, benchmark: &Benchmark) {
    run_single_threaded(bencher, benchmark);
}

#[bench(args=[&COLOUR_SCIENCE, &PANDAS, &STATIC_FRAME], sample_size=1, sample_count=3)]
fn medium(bencher: Bencher, benchmark: &Benchmark) {
    run_single_threaded(bencher, benchmark);
}

#[bench(args=[&SYMPY, &PYDANTIC], sample_size=1, sample_count=2)]
fn large(bencher: Bencher, benchmark: &Benchmark) {
    run_single_threaded(bencher, benchmark);
}

#[bench(args=[&ALTAIR], sample_size=3, sample_count=8)]
fn multithreaded(bencher: Bencher, benchmark: &Benchmark) {
    let thread_pool = ThreadPoolBuilder::new().build().unwrap();

    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_values(|db| {
            thread_pool.install(|| {
                check_project(&db, benchmark.project.name, benchmark.max_diagnostics);
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
        check_project(&db, TANJUN.project.name, TANJUN.max_diagnostics);
    }

    divan::main();
}
