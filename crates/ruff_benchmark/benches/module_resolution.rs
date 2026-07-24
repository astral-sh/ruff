//! Measures a batch of cold module queries on a fresh database as the number of extra search paths
//! grows. Queries within the batch share lower-level resolver caches.

use std::hint::black_box;

use divan::{Bencher, bench};

use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{SystemPath, SystemPathBuf, TestSystem};
use ruff_ranged_value::RangedValue;
use ty_module_resolver::{ImportingFile, ModuleName, resolve_module};
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::python_version::SupportedPythonVersion;
use ty_project::metadata::value::RelativePathBuf;
use ty_project::{Db as _, ProjectDatabase, ProjectMetadata};

const SEEDED_TARGETS: &[&str] = &["target_0", "target_1", "target_2", "target_3", "target_4"];
// Exercise stub-overlay discovery followed by normal fallback.
const NONEXISTENT_NAMES: &[&str] = &[
    "nonexistent_0.child",
    "nonexistent_1.child",
    "nonexistent_2.child",
    "nonexistent_3.child",
    "nonexistent_4.child",
    "nonexistent_5.child",
    "nonexistent_6.child",
    "nonexistent_7.child",
];
const STDLIB_NAMES: &[&str] = &[
    "os",
    "sys",
    "typing",
    "collections",
    "itertools",
    "functools",
];

struct Case {
    db: ProjectDatabase,
    importing_file: File,
    resolves: Vec<ModuleName>,
}

fn setup_case(n: usize) -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();

    let mut extra_paths = Vec::with_capacity(n);
    for i in 0..n {
        let dir = format!("/extra/p{i}");
        let filler = SystemPathBuf::from(format!("{dir}/mod{i}.py"));
        fs.write_file_all(&filler, "x = 0").unwrap();
        extra_paths.push(RelativePathBuf::cli(SystemPath::new(&dir)));

        if let Some(target) = SEEDED_TARGETS.get(i) {
            let target_path = SystemPathBuf::from(format!("{dir}/{target}.py"));
            fs.write_file_all(&target_path, "x = 0").unwrap();
        }
    }

    let importing_path = SystemPathBuf::from("/src/test.py");
    fs.write_file_all(&importing_path, "").unwrap();

    let mut metadata = ProjectMetadata::discover(SystemPath::new("/src"), &system).unwrap();
    metadata.apply_override_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(SupportedPythonVersion::Py312)),
            extra_paths: Some(extra_paths),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let db = ProjectDatabase::fallible(metadata, system).unwrap();
    let importing_file = system_path_to_file(&db, &importing_path).unwrap();

    let resolves = SEEDED_TARGETS
        .iter()
        .chain(NONEXISTENT_NAMES.iter())
        .chain(STDLIB_NAMES.iter())
        .map(|name| ModuleName::new_static(name).unwrap())
        .collect();

    Case {
        db,
        importing_file,
        resolves,
    }
}

#[bench(consts = [5, 25, 125, 600])]
fn ty_module_resolver<const PATHS: usize>(bencher: Bencher) {
    bencher
        .with_inputs(|| setup_case(PATHS))
        .bench_local_refs(|case| {
            let environment = case
                .db
                .project()
                .program(&case.db)
                .resolver_environment(&case.db);
            for name in &case.resolves {
                black_box(resolve_module(
                    &case.db,
                    ImportingFile::File(case.importing_file, environment),
                    name,
                ));
            }
        });
}

fn main() {
    divan::main();
}
