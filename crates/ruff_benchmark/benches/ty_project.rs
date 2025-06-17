#![allow(clippy::disallowed_names)]
use rayon::ThreadPoolBuilder;
use ruff_benchmark::criterion;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ruff_benchmark::real_world_projects::RealWorldProject;
use ruff_db::system::{InMemorySystem, SystemPath, TestSystem};
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

#[track_caller]
fn bench_project(project: RealWorldProject, criterion: &mut Criterion, max_diagnostics: usize) {
    setup_rayon();

    let setup_project = project.setup().expect("Failed to setup project");

    // Create system and metadata (expensive, done once)
    let fs = setup_project.memory_fs().clone();
    let system = TestSystem::new(InMemorySystem::from_memory_fs(fs));

    let src_root = SystemPath::new("/");
    let mut metadata = ProjectMetadata::discover(src_root, &system).unwrap();

    metadata.apply_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(setup_project.config.python_version)),
            python: Some(RelativePathBuf::cli(SystemPath::new(".venv"))),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let check_paths = setup_project.check_paths();

    fn setup(
        metadata: &ProjectMetadata,
        system: &TestSystem,
        check_paths: &[&SystemPath],
    ) -> ProjectDatabase {
        // Create new database instance and collect files for this instance
        let mut db = ProjectDatabase::new(metadata.clone(), system.clone()).unwrap();

        db.project().set_included_paths(
            &mut db,
            check_paths
                .into_iter()
                .map(|path| path.to_path_buf())
                .collect(),
        );
        db
    }

    fn check_project(db: &mut ProjectDatabase, max_diagnostics: usize) {
        let result = db.check();
        // Don't assert specific diagnostic count for real-world projects
        // as they may have legitimate type issues
        let diagnostics = result.len();

        assert!(diagnostics > 1 && diagnostics <= max_diagnostics);
    }

    criterion.bench_function(&setup_project.config.name, |b| {
        b.iter_batched_ref(
            || setup(&metadata, &system, &check_paths),
            |db| check_project(db, max_diagnostics),
            BatchSize::SmallInput,
        );
    });
}

fn colour_science(criterion: &mut Criterion) {
    // Setup the colour-science project (expensive, done once)
    let project = RealWorldProject {
        name: "colour-science",
        location: "https://github.com/colour-science/colour",
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

    bench_project(project, criterion, 477);
}

fn pydantic(criterion: &mut Criterion) {
    // Setup the colour-science project (expensive, done once)
    let project = RealWorldProject {
        name: "pydantic",
        location: "https://github.com/pydantic/pydantic",
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

    bench_project(project, criterion, 1000);
}

fn freqtrade(criterion: &mut Criterion) {
    // Setup the colour-science project (expensive, done once)
    let project = RealWorldProject {
        name: "freqtrade",
        location: "https://github.com/freqtrade/freqtrade",
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

    bench_project(project, criterion, 10000);
}

fn hydra(criterion: &mut Criterion) {
    // Setup the colour-science project (expensive, done once)
    let project = RealWorldProject {
        name: "hydra-zen",
        location: "https://github.com/mit-ll-responsible-ai/hydra-zen",
        commit: "dd2b50a9614c6f8c46c5866f283c8f7e7a960aa8",
        paths: &[SystemPath::new("src")],
        dependencies: &["pydantic", "beartype", "hydra-core"],
        max_dep_date: "2025-06-17",
        python_version: PythonVersion::PY313,
    };

    bench_project(project, criterion, 100000);
}

static RAYON_INITIALIZED: std::sync::Once = std::sync::Once::new();

fn setup_rayon() {
    // Initialize the rayon thread pool outside the benchmark because it has a significant cost.
    // We limit the thread pool to only one (the current thread) because we're focused on
    // where ty spends time and less about how well the code runs concurrently.
    // We might want to add a benchmark focusing on concurrency to detect congestion in the future.
    RAYON_INITIALIZED.call_once(|| {
        ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
            .unwrap();
    });
}

criterion_group!(real_world, pydantic, freqtrade, hydra);
criterion_main!(real_world);
