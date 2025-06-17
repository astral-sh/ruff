use ::criterion::SamplingMode;
use rayon::ThreadPoolBuilder;
use ruff_benchmark::criterion;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ruff_benchmark::real_world_projects::RealWorldProject;
use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

#[derive(Copy, Clone)]
enum Size {
    Small,
    Medium,
    Large,
}

#[track_caller]
fn bench_project(
    project: RealWorldProject,
    criterion: &mut Criterion,
    max_diagnostics: usize,
    size: Size,
) {
    fn setup(
        metadata: &ProjectMetadata,
        system: &OsSystem,
        check_paths: &[&SystemPath],
    ) -> ProjectDatabase {
        // Create new database instance and collect files for this instance
        let mut db = ProjectDatabase::new(metadata.clone(), system.clone()).unwrap();

        db.project().set_included_paths(
            &mut db,
            check_paths
                .iter()
                .map(|path| SystemPath::absolute(path, system.current_directory()))
                .collect(),
        );
        db
    }

    fn check_project(db: &mut ProjectDatabase, max_diagnostics: usize) {
        let result = db.check();
        // Don't assert specific diagnostic count for real-world projects
        // as they may have legitimate type issues
        let diagnostics = result.len();

        assert!(
            diagnostics > 1 && diagnostics <= max_diagnostics,
            "Expected between {} and {} diagnostics but got {}",
            1,
            max_diagnostics,
            diagnostics
        );
    }

    let setup_project = project.setup().expect("Failed to setup project");

    let root = SystemPathBuf::from_path_buf(setup_project.path.clone()).unwrap();
    let system = OsSystem::new(&root);

    let mut metadata = ProjectMetadata::discover(&root, &system).unwrap();

    metadata.apply_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(setup_project.config.python_version)),
            python: (!setup_project.config().dependencies.is_empty())
                .then_some(RelativePathBuf::cli(SystemPath::new(".venv"))),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let check_paths = setup_project.check_paths();

    let mut group = criterion.benchmark_group("project");
    group.sampling_mode(SamplingMode::Flat);

    if cfg!(feature = "codspeed") {
        group.sample_size(10);
    } else {
        group.sample_size(match size {
            Size::Small => 30,
            Size::Medium => 20,
            Size::Large => 10,
        });
    }

    group.bench_function(setup_project.config.name, |b| {
        b.iter_batched_ref(
            || setup(&metadata, &system, check_paths),
            |db| check_project(db, max_diagnostics),
            BatchSize::SmallInput,
        );
    });
}

fn colour_science(criterion: &mut Criterion) {
    let project = RealWorldProject {
        name: "colour-science",
        repository: "https://github.com/colour-science/colour",
        commit: "a17e2335c29e7b6f08080aa4c93cfa9b61f84757",
        paths: &[SystemPath::new("colour")],
        dependencies: &[
            "matplotlib",
            "numpy",
            "pandas-stubs",
            "pytest",
            "scipy-stubs",
        ],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY310,
    };

    bench_project(project, criterion, 477, Size::Medium);
}

fn pydantic(criterion: &mut Criterion) {
    let project = RealWorldProject {
        name: "pydantic",
        repository: "https://github.com/pydantic/pydantic",
        commit: "0c4a22b64b23dfad27387750cf07487efc45eb05",
        paths: &[SystemPath::new("pydantic")],
        dependencies: &[
            "annotated-types",
            "pydantic-core",
            "typing-extensions",
            "typing-inspection",
        ],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY39,
    };

    bench_project(project, criterion, 1000, Size::Small);
}

fn freqtrade(criterion: &mut Criterion) {
    let project = RealWorldProject {
        name: "freqtrade",
        repository: "https://github.com/freqtrade/freqtrade",
        commit: "2d842ea129e56575852ee0c45383c8c3f706be19",
        paths: &[SystemPath::new("freqtrade")],
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
    };

    bench_project(project, criterion, 400, Size::Small);
}

fn pandas(criterion: &mut Criterion) {
    let project = RealWorldProject {
        name: "pandas",
        repository: "https://github.com/pandas-dev/pandas",
        commit: "5909621e2267eb67943a95ef5e895e8484c53432",
        paths: &[SystemPath::new("pandas")],
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
    };

    bench_project(project, criterion, 3000, Size::Large);
}

fn sympy(criterion: &mut Criterion) {
    let project = RealWorldProject {
        name: "sympy",
        repository: "https://github.com/sympy/sympy",
        commit: "22fc107a94eaabc4f6eb31470b39db65abb7a394",
        paths: &[SystemPath::new("sympy")],
        dependencies: &["mpmath"],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY312,
    };

    bench_project(project, criterion, 13000, Size::Large);
}

criterion_group!(project, colour_science, freqtrade, pandas, pydantic, sympy);
criterion_main!(project);
