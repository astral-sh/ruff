//! Fuzzer harness that runs the type checker to catch for panics for source code containing
//! syntax errors.

#![no_main]

use std::sync::{Mutex, OnceLock};

use libfuzzer_sys::{fuzz_target, Corpus};

use red_knot_python_semantic::types::check_types;
use red_knot_python_semantic::{
    default_lint_registry, lint::RuleSelection, Db as SemanticDb, Program, ProgramSettings,
    PythonVersion, SearchPathSettings,
};
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::system::{DbWithTestSystem, System, SystemPathBuf, TestSystem};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use ruff_python_parser::{parse_unchecked, Mode};

/// Database that can be used for testing.
///
/// Uses an in memory filesystem and it stubs out the vendored files by default.
#[salsa::db]
struct TestDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
    events: std::sync::Arc<std::sync::Mutex<Vec<salsa::Event>>>,
    rule_selection: std::sync::Arc<RuleSelection>,
}

impl TestDb {
    fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            system: TestSystem::default(),
            vendored: red_knot_vendored::file_system().clone(),
            events: std::sync::Arc::default(),
            files: Files::default(),
            rule_selection: RuleSelection::from_registry(&default_lint_registry()).into(),
        }
    }
}

#[salsa::db]
impl SourceDb for TestDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &self.vendored
    }

    fn system(&self) -> &dyn System {
        &self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

impl DbWithTestSystem for TestDb {
    fn test_system(&self) -> &TestSystem {
        &self.system
    }

    fn test_system_mut(&mut self) -> &mut TestSystem {
        &mut self.system
    }
}

impl Upcast<dyn SourceDb> for TestDb {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }
    fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
        self
    }
}

#[salsa::db]
impl SemanticDb for TestDb {
    fn is_file_open(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }

    fn rule_selection(&self) -> &RuleSelection {
        &self.rule_selection
    }
}

#[salsa::db]
impl salsa::Database for TestDb {
    fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
        let event = event();
        tracing::trace!("event: {:?}", event);
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }
}

fn setup_db() -> TestDb {
    let db = TestDb::new();

    let src_root = SystemPathBuf::from("/src");
    db.memory_file_system()
        .create_directory_all(&src_root)
        .unwrap();

    Program::from_settings(
        &db,
        &ProgramSettings {
            python_version: PythonVersion::default(),
            search_paths: SearchPathSettings::new(src_root),
        },
    )
    .expect("Valid search path settings");

    db
}

static TEST_DB: OnceLock<Mutex<TestDb>> = OnceLock::new();

fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(code) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    let parsed = parse_unchecked(code, Mode::Module);
    if parsed.is_valid() {
        return Corpus::Reject;
    }

    let mut db = TEST_DB
        .get_or_init(|| Mutex::new(setup_db()))
        .lock()
        .unwrap();

    for path in &["/src/a.py", "/src/a.pyi"] {
        db.write_file(path, code).unwrap();
        let file = system_path_to_file(&*db, path).unwrap();
        check_types(&*db, file);
        db.memory_file_system().remove_file(path).unwrap();
        file.sync(&mut *db);
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
