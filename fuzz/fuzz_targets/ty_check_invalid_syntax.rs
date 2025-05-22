//! Fuzzer harness that runs the type checker to catch for panics for source code containing
//! syntax errors.

#![no_main]

use std::sync::{Arc, Mutex, OnceLock};

use libfuzzer_sys::{Corpus, fuzz_target};

use ruff_db::files::{File, Files, system_path_to_file};
use ruff_db::system::{
    DbWithTestSystem, DbWithWritableSystem as _, System, SystemPathBuf, TestSystem,
};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use ruff_python_ast::PythonVersion;
use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};
use ty_python_semantic::lint::LintRegistry;
use ty_python_semantic::types::check_types;
use ty_python_semantic::{
    Db as SemanticDb, Program, ProgramSettings, PythonPlatform, SearchPathSettings,
    default_lint_registry, lint::RuleSelection, PythonVersionWithSource,
};

/// Database that can be used for testing.
///
/// Uses an in memory filesystem and it stubs out the vendored files by default.
#[salsa::db]
#[derive(Clone)]
struct TestDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
    rule_selection: Arc<RuleSelection>,
}

impl TestDb {
    fn new() -> Self {
        Self {
            storage: salsa::Storage::new(Some(Box::new({
                move |event| {
                    tracing::trace!("event: {:?}", event);
                }
            }))),
            system: TestSystem::default(),
            vendored: ty_vendored::file_system().clone(),
            files: Files::default(),
            rule_selection: RuleSelection::from_registry(default_lint_registry()).into(),
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

    fn python_version(&self) -> PythonVersion {
        Program::get(self).python_version(self)
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

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }
}

#[salsa::db]
impl salsa::Database for TestDb {}

fn setup_db() -> TestDb {
    let db = TestDb::new();

    let src_root = SystemPathBuf::from("/src");
    db.memory_file_system()
        .create_directory_all(&src_root)
        .unwrap();

    Program::from_settings(
        &db,
        ProgramSettings {
            python_version: PythonVersionWithSource::default(),
            python_platform: PythonPlatform::default(),
            search_paths: SearchPathSettings::new(vec![src_root]),
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

    let parsed = parse_unchecked(code, ParseOptions::from(Mode::Module));
    if parsed.has_valid_syntax() {
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
