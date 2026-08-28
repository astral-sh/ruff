use std::sync::Arc;

use anyhow::{Context, anyhow};
use ruff_db::files::{File, Files, system_path_to_file};
use ruff_db::system::{DbWithTestSystem, System, SystemPath, SystemPathBuf, TestSystem};
use ruff_db::vendored::VendoredFileSystem;

use ty_python_core::program::ProgramSettings;
use ty_python_semantic::dependency::DependencyMetadata;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::pull_types::pull_types;
use ty_python_semantic::{
    AnalysisSettings, Db as _, PythonVersionWithSource, check_file_unwrap, default_lint_registry,
};

use ruff_db::diagnostic::Diagnostic;
use test_case::test_case;
use ty_python_core::{Db as _, ProgramFile, TestProgramDb};

fn get_cargo_workspace_root() -> anyhow::Result<&'static SystemPath> {
    SystemPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(SystemPath::parent)
        .context("Failed to determine the Cargo workspace root")
}

/// Test that all snippets in testcorpus can be checked without panic.
#[test]
fn corpus_no_panic() -> anyhow::Result<()> {
    let crate_root = String::from(env!("CARGO_MANIFEST_DIR"));
    run_corpus_tests(&format!("{crate_root}/resources/corpus/**/*.py"))
}

#[test]
fn parser_no_panic() -> anyhow::Result<()> {
    let workspace_root = get_cargo_workspace_root()?;
    run_corpus_tests(&format!(
        "{workspace_root}/crates/ruff_python_parser/resources/**/*.py"
    ))
}

#[test_case("a-e")]
#[test_case("f")]
#[test_case("g-o")]
#[test_case("p")]
#[test_case("q-z")]
#[test_case("!a-z")]
fn linter_no_panic(range: &str) -> anyhow::Result<()> {
    let workspace_root = get_cargo_workspace_root()?;
    run_corpus_tests(&format!(
        "{workspace_root}/crates/ruff_linter/resources/test/fixtures/[{range}]*/**/*.py"
    ))
}

#[test]
fn linter_stubs_no_panic() -> anyhow::Result<()> {
    let workspace_root = get_cargo_workspace_root()?;
    run_corpus_tests(&format!(
        "{workspace_root}/crates/ruff_linter/resources/test/fixtures/**/*.pyi"
    ))
}

#[test_case("a-e")]
#[test_case("f-k")]
#[test_case("l-p")]
#[test_case("q-z")]
#[test_case("!a-z")]
fn typeshed_no_panic(range: &str) -> anyhow::Result<()> {
    let workspace_root = get_cargo_workspace_root()?;
    run_corpus_tests(&format!(
        "{workspace_root}/crates/ty_vendored/vendor/typeshed/stdlib/[{range}]*.pyi"
    ))
}

#[expect(clippy::print_stdout)]
fn run_corpus_tests(pattern: &str) -> anyhow::Result<()> {
    let root = SystemPathBuf::from("/src");

    let mut db = CorpusDb::new();
    db.memory_file_system().create_directory_all(&root)?;

    let workspace_root = get_cargo_workspace_root()?;
    let workspace_root = workspace_root.to_string();

    let corpus = glob::glob(pattern).context("Failed to compile pattern")?;

    for path in corpus {
        let path = path.context("Failed to glob path")?;
        let path = SystemPathBuf::from_path_buf(path).map_err(|path| {
            anyhow!(
                "Failed to convert path '{path}' to system path",
                path = path.display()
            )
        })?;

        let relative_path = path.strip_prefix(&workspace_root)?;

        let source = path.as_path();
        let source_filename = source.file_name().unwrap();

        let code = std::fs::read_to_string(source)
            .with_context(|| format!("Failed to read test file: {path}"))?;

        let mut check_with_file_name = |path: &SystemPath| {
            db.memory_file_system().write_file_all(path, &code).unwrap();
            File::sync_path(&mut db, path);

            // this test is only asserting that we can pull every expression type without a panic
            // (and some non-expressions that clearly define a single type)
            let file = system_path_to_file(&db, path).unwrap();

            if let Err(err) = std::panic::catch_unwind(|| {
                pull_types(&db, db.program_file(file));
            }) {
                println!("Check failed for {relative_path:?}.");
                std::panic::resume_unwind(err);
            }

            db.memory_file_system().remove_file(path).unwrap();
            file.sync(&mut db);
        };

        if source.extension() == Some("pyi") {
            println!("checking {relative_path}");
            let pyi_dest = root.join(source_filename);
            check_with_file_name(&pyi_dest);
        } else {
            println!("checking {relative_path}");
            let py_dest = root.join(source_filename);
            check_with_file_name(&py_dest);

            let pyi_dest = root.join(format!("{source_filename}i"));
            println!("re-checking as stub file: {pyi_dest}");
            check_with_file_name(&pyi_dest);
        }
    }

    Ok(())
}

#[salsa::db]
#[derive(Clone)]
pub struct CorpusDb {
    storage: salsa::Storage<Self>,
    files: Files,
    rule_selection: RuleSelection,
    system: TestSystem,
    vendored: VendoredFileSystem,
    analysis_settings: Arc<AnalysisSettings>,
    program_settings: ProgramSettings,
}

impl CorpusDb {
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        let vendored = ty_vendored::file_system().clone();
        let program_settings = ProgramSettings::empty(&vendored);
        Self {
            storage: salsa::Storage::new(None),
            system: TestSystem::default(),
            vendored,
            rule_selection: RuleSelection::from_registry(default_lint_registry()),
            files: Files::default(),
            analysis_settings: Arc::new(AnalysisSettings::default()),
            program_settings,
        }
    }
}

impl DbWithTestSystem for CorpusDb {
    fn test_system(&self) -> &TestSystem {
        &self.system
    }

    fn test_system_mut(&mut self) -> &mut TestSystem {
        &mut self.system
    }
}

#[salsa::db]
impl ruff_db::Db for CorpusDb {
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

#[salsa::db]
impl ty_module_resolver::Db for CorpusDb {}

#[salsa::db]
impl ty_python_core::Db for CorpusDb {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }
}

#[salsa::db]
impl TestProgramDb for CorpusDb {
    fn program_settings(&self) -> &ProgramSettings {
        &self.program_settings
    }
}

#[salsa::db]
impl ty_python_semantic::Db for CorpusDb {
    fn check_file(&self, file: File) -> Vec<Diagnostic> {
        if self.should_check_file(file) {
            check_file_unwrap(self, self.program_file(file))
        } else {
            Vec::new()
        }
    }

    fn program_file(&self, file: File) -> ProgramFile<'_> {
        self.program().program_file(self, file)
    }

    fn python_version_with_source(&self, _file: File) -> &PythonVersionWithSource {
        &self.program_settings.python_version
    }

    fn rule_selection(&self, _file: File) -> &RuleSelection {
        &self.rule_selection
    }

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }

    fn verbose(&self) -> bool {
        false
    }

    fn is_open_file(&self, _file: File) -> bool {
        false
    }

    fn analysis_settings(&self, _file: File) -> &AnalysisSettings {
        &self.analysis_settings
    }

    fn dependency_metadata(&self, _file: File) -> Option<&DependencyMetadata> {
        None
    }

    fn dyn_clone(&self) -> Box<dyn ty_python_semantic::Db> {
        Box::new(self.clone())
    }
}

#[salsa::db]
impl salsa::Database for CorpusDb {}
