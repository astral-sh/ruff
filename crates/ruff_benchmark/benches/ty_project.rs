#![allow(clippy::disallowed_names)]
use ruff_benchmark::criterion;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ruff_benchmark::real_world_projects::RealWorldProject;
use ruff_db::system::{InMemorySystem, SystemPath, TestSystem};
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::{RangedValue, RelativePathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

fn bench_project(project: RealWorldProject, criterion: &mut Criterion) {
    let setup_project = project
        .setup()
        .expect("Failed to setup colour-science project");

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

    fn check_project(db: &mut ProjectDatabase) {
        let result = db.check();
        // Don't assert specific diagnostic count for real-world projects
        // as they may have legitimate type issues
        let diagnostics = result.len();

        assert!(diagnostics > 1 && diagnostics <= 477);
    }

    criterion.bench_function(&setup_project.config.name, |b| {
        b.iter_batched_ref(
            || setup(&metadata, &system, &check_paths),
            check_project,
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_real_world_colour_science(criterion: &mut Criterion) {
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
        python_version: PythonVersion::PY311,
    };

    bench_project(project, criterion);
}

criterion_group!(real_world, benchmark_real_world_colour_science);
criterion_main!(real_world);
