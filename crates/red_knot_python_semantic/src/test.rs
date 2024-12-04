use crate::db::tests::TestDb;
use crate::program::{Program, SearchPathSettings};
use crate::python_version::PythonVersion;
use crate::ProgramSettings;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

pub(crate) fn setup_db() -> TestDb {
    let db = TestDb::new();

    let src_root = SystemPathBuf::from("/src");
    db.memory_file_system()
        .create_directory_all(&src_root)
        .unwrap();

    Program::from_settings(
        &db,
        &ProgramSettings {
            target_version: PythonVersion::default(),
            search_paths: SearchPathSettings::new(src_root),
        },
    )
    .expect("Valid search path settings");

    db
}
