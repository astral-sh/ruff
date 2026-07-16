use divan::{Bencher, bench};
use std::fmt::{Display, Formatter};

use rayon::ThreadPoolBuilder;
use ruff_benchmark::real_world_projects::{InstalledProject, RealWorldProject, TY_ECOSYSTEM_PIN};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};

use ruff_db::testing::setup_logging_with_filter;
use ruff_ranged_value::RangedValue;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::python_version::SupportedPythonVersion;
use ty_project::metadata::value::RelativePathBuf;
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

        metadata.apply_override_options(Options {
            environment: Some(EnvironmentOptions {
                python_version: Some(RangedValue::cli(installed_project.config.python_version)),
                python: Some(RelativePathBuf::cli(SystemPath::new(".venv"))),
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        });

        let mut db = ProjectDatabase::fallible(metadata, system).unwrap();

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

#[track_caller]
#[expect(clippy::cast_precision_loss)]
fn check_project(db: &ProjectDatabase, project_name: &str, max_diagnostics: usize) {
    let result = db.check();
    let diagnostics = result.len();

    assert!(
        diagnostics > 1 && diagnostics <= max_diagnostics,
        "Expected between 1 and {max_diagnostics} diagnostics on project '{project_name}' but got {diagnostics}",
    );

    if (max_diagnostics - diagnostics) as f64 / max_diagnostics as f64 > 0.10 {
        tracing::warn!(
            "The expected diagnostics for project `{project_name}` can be reduced: expected {max_diagnostics} but got {diagnostics}"
        );
    }
}

static ALTAIR: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "altair",
        repository: "https://github.com/vega/altair",
        commit: "a9765713566095349cb1cfbbe85d6ad258c84245",
        paths: &["altair", "tests"],
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
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    3,
);

static COLOUR_SCIENCE: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "colour-science",
        repository: "https://github.com/colour-science/colour",
        commit: "4ee3b72a2c6205c1d0cd964075621ec19290d571",
        paths: &["colour"],
        dependencies: &[
            "matplotlib",
            "numpy",
            "pandas-stubs",
            "pytest",
            "scipy-stubs",
        ],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    450,
);

static FREQTRADE: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "freqtrade",
        repository: "https://github.com/freqtrade/freqtrade",
        commit: "9fca9c818529c51ba19a1e76bc9428cf3ad56e4b",
        paths: &["freqtrade", "scripts"],
        dependencies: &[
            "numpy",
            "pandas-stubs",
            "pydantic",
            "sqlalchemy",
            "types-cachetools",
            "types-filelock",
            "types-python-dateutil",
            "requests",
            "types-tabulate",
        ],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    717,
);

static PANDAS: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "pandas",
        repository: "https://github.com/pandas-dev/pandas",
        commit: "19b0ecf5d5b7fa0b8391a6d2cc1e2a7d1ea5f660",
        paths: &["pandas"],
        dependencies: &[
            "numpy",
            "types-python-dateutil",
            "types-pytz",
            "types-PyMySQL",
            "types-setuptools",
            "pytest",
        ],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    6700,
);

static PYDANTIC: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "pydantic",
        repository: "https://github.com/pydantic/pydantic",
        commit: "7974d591c7d22d6667ea9d09831ba9caddcbb373",
        paths: &["pydantic"],
        dependencies: &["annotated-types", "pydantic-core", "typing-inspection"],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    // TODO: Reduce this once legacy generic method targets are quantified correctly.
    1620,
);

static SYMPY: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "sympy",
        repository: "https://github.com/sympy/sympy",
        commit: "8381f0c42956f60caed72aedb2ca4e82420b9992",
        paths: &["sympy"],
        dependencies: &["mpmath"],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    16500,
);

static TANJUN: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "tanjun",
        repository: "https://github.com/FasterSpeeding/Tanjun",
        commit: "88d43a267a5bcd995d4929f62b14fbe5c18e59de",
        paths: &["tanjun"],
        dependencies: &["hikari", "alluka"],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    110,
);

static STATIC_FRAME: Benchmark = Benchmark::new(
    RealWorldProject {
        name: "static-frame",
        repository: "https://github.com/static-frame/static-frame",
        commit: "0b1e2fc2e819cde1b9b99be7cc57be08ee43d8de",
        paths: &["static_frame"],
        // N.B. `arraykit` is installed as a dependency during ecosystem runs,
        // but it takes much longer to be installed in a Codspeed run
        // (seems to be built from source on the Codspeed CI runners for some reason).
        dependencies: &["numpy"],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    },
    1950,
);

#[track_caller]
fn run_single_threaded(bencher: Bencher, benchmark: &Benchmark) {
    bencher
        .with_inputs(|| benchmark.setup_iteration())
        .bench_local_refs(|db| {
            check_project(db, benchmark.project.name, benchmark.max_diagnostics);
        });
}

#[bench(sample_size = 2, sample_count = 3)]
fn altair(bencher: Bencher) {
    run_single_threaded(bencher, &ALTAIR);
}

#[bench(sample_size = 2, sample_count = 3)]
fn freqtrade(bencher: Bencher) {
    run_single_threaded(bencher, &FREQTRADE);
}

#[bench(sample_size = 2, sample_count = 3)]
fn tanjun(bencher: Bencher) {
    run_single_threaded(bencher, &TANJUN);
}

#[bench(sample_size = 2, sample_count = 3)]
fn pydantic(bencher: Bencher) {
    run_single_threaded(bencher, &PYDANTIC);
}

#[bench(sample_size = 1, sample_count = 3)]
fn static_frame(bencher: Bencher) {
    run_single_threaded(bencher, &STATIC_FRAME);
}

#[bench(sample_size = 1, sample_count = 2)]
fn colour_science(bencher: Bencher) {
    run_single_threaded(bencher, &COLOUR_SCIENCE);
}

#[bench(sample_size = 1, sample_count = 2)]
fn pandas(bencher: Bencher) {
    run_single_threaded(bencher, &PANDAS);
}

#[bench(sample_size = 1, sample_count = 2)]
fn sympy(bencher: Bencher) {
    run_single_threaded(bencher, &SYMPY);
}

#[bench(sample_size = 3, sample_count = 8)]
fn multithreaded(bencher: Bencher) {
    let thread_pool = ThreadPoolBuilder::new().build().unwrap();

    bencher
        .with_inputs(|| ALTAIR.setup_iteration())
        .bench_local_values(|db| {
            thread_pool.install(|| {
                check_project(&db, ALTAIR.project.name, ALTAIR.max_diagnostics);
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
