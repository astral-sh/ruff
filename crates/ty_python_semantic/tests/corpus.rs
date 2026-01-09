use std::sync::Arc;

use anyhow::{Context, anyhow};
use ruff_db::Db;
use ruff_db::files::{File, Files, system_path_to_file};
use ruff_db::system::{DbWithTestSystem, System, SystemPath, SystemPathBuf, TestSystem};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;

use ty_module_resolver::SearchPathSettings;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::pull_types::pull_types;
use ty_python_semantic::{
    AnalysisSettings, Program, ProgramSettings, PythonPlatform, PythonVersionSource,
    PythonVersionWithSource, default_lint_registry,
};

use test_case::test_case;

fn get_cargo_workspace_root() -> anyhow::Result<SystemPathBuf> {
    Ok(SystemPathBuf::from(String::from_utf8(
        std::process::Command::new("cargo")
            .args(["locate-project", "--workspace", "--message-format", "plain"])
            .output()?
            .stdout,
    )?)
    .parent()
    .unwrap()
    .to_owned())
}

/// Test that all snippets in testcorpus can be checked without panic (except for [`KNOWN_FAILURES`])
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

        let (py_expected_to_fail, pyi_expected_to_fail) = KNOWN_FAILURES
            .iter()
            .find_map(|(path, py_fail, pyi_fail)| {
                if *path == relative_path.as_str().replace('\\', "/") {
                    Some((*py_fail, *pyi_fail))
                } else {
                    None
                }
            })
            .unwrap_or((false, false));

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

            let result = std::panic::catch_unwind(|| pull_types(&db, file));

            let expected_to_fail = if path.extension().map(|e| e == "pyi").unwrap_or(false) {
                pyi_expected_to_fail
            } else {
                py_expected_to_fail
            };
            if let Err(err) = result {
                if !expected_to_fail {
                    println!(
                        "Check failed for {relative_path:?}. Consider fixing it or adding it to KNOWN_FAILURES"
                    );
                    std::panic::resume_unwind(err);
                }
            } else {
                assert!(
                    !expected_to_fail,
                    "Expected to panic, but did not. Consider removing this path from KNOWN_FAILURES"
                );
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

/// Whether or not the .py/.pyi version of this file is expected to fail
#[rustfmt::skip]
const KNOWN_FAILURES: &[(&str, bool, bool)] = &[
];

#[salsa::db]
#[derive(Clone)]
pub struct CorpusDb {
    storage: salsa::Storage<Self>,
    files: Files,
    rule_selection: RuleSelection,
    system: TestSystem,
    vendored: VendoredFileSystem,
    analysis_settings: Arc<AnalysisSettings>,
}

impl CorpusDb {
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        let db = Self {
            storage: salsa::Storage::new(None),
            system: TestSystem::default(),
            vendored: ty_vendored::file_system().clone(),
            rule_selection: RuleSelection::from_registry(default_lint_registry()),
            files: Files::default(),
            analysis_settings: Arc::new(AnalysisSettings::default()),
        };

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: PythonVersion::latest_ty(),
                    source: PythonVersionSource::default(),
                },
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings::new(vec![])
                    .to_search_paths(db.system(), db.vendored())
                    .unwrap(),
            },
        );

        db
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

    fn python_version(&self) -> PythonVersion {
        Program::get(self).python_version(self)
    }
}

#[salsa::db]
impl ty_module_resolver::Db for CorpusDb {
    fn search_paths(&self) -> &ty_module_resolver::SearchPaths {
        Program::get(self).search_paths(self)
    }
}

#[salsa::db]
impl ty_python_semantic::Db for CorpusDb {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
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

    fn analysis_settings(&self) -> &AnalysisSettings {
        &self.analysis_settings
    }
}

#[salsa::db]
impl salsa::Database for CorpusDb {}
