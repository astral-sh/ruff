use std::path::Path;

use rustc_hash::FxHashSet;

use ruff_benchmark::real_world_projects::copy_directory_recursive;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{SystemPath, SystemPathBuf, TestSystem};
use ruff_ranged_value::RangedValue;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::python_version::SupportedPythonVersion;
use ty_project::metadata::value::RelativePathBuf;
use ty_project::{CheckMode, Db, ProjectDatabase, ProjectMetadata};

mod rayon;

pub(super) use self::rayon::setup_rayon;

pub(super) struct Case {
    pub(super) db: ProjectDatabase,
}

pub(super) fn setup_micro_case(code: &str) -> Case {
    setup_micro_case_inner(code, None)
}

pub(super) fn setup_micro_case_inner(code: &str, venv_path: Option<&Path>) -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();

    let python = venv_path.map(|venv_path| {
        // Copy the on-disk venv into the in-memory filesystem.
        // ProjectMetadata::discover walks up from /src and uses / as the project root,
        // so the venv must be at /.venv for the `python = ".venv"` option to resolve correctly.
        copy_directory_recursive(&fs, venv_path, SystemPath::new("/.venv"))
            .expect("Failed to copy venv to memory filesystem");

        RelativePathBuf::cli(SystemPath::new(".venv"))
    });

    let file_path = "src/test.py";
    fs.write_file_all(
        SystemPathBuf::from(file_path),
        &*ruff_python_trivia::textwrap::dedent(code),
    )
    .unwrap();

    let src_root = SystemPath::new("/src");
    let mut metadata = ProjectMetadata::discover(src_root, &system).unwrap();
    metadata.set_override_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(SupportedPythonVersion::Py312)),
            python,
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let mut db = ProjectDatabase::fallible(metadata, system).unwrap();
    let file = system_path_to_file(&db, SystemPathBuf::from(file_path)).unwrap();

    db.set_check_mode(CheckMode::OpenFiles);
    db.project()
        .set_open_files(&mut db, FxHashSet::from_iter([file]));

    Case { db }
}
