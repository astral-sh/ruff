use std::fmt::Write;

use anyhow::{Context, anyhow, bail};
use camino::Utf8Path;
use colored::Colorize;
use mdtest::matcher::{self, Failure};
use mdtest::parser::{self, EmbeddedFileSourceMap};
use mdtest::{Failures, FileFailures, MDTEST_TEST_FILTER, MarkdownEdit, TestFile, output_format};
use ruff_db::Db as _;
use ruff_db::files::{FileRootKind, Files, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{
    DbWithTestSystem, DbWithWritableSystem as _, System, SystemPathBuf, TestSystem,
};
use ruff_db::vendored::VendoredFileSystem;
use ruff_linter::linter::{ParseSource, lint_only};
use ruff_linter::registry::Rule;
use ruff_linter::settings::LinterSettings;
use ruff_linter::settings::flags;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, PythonVersion};
use ruff_source_file::LineIndex;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct MarkdownTestConfig {
    rules: Vec<String>,
}

#[salsa::db]
#[derive(Default, Clone)]
struct TestDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
}

impl TestDb {
    fn setup() -> Self {
        Self::default()
    }
}

#[salsa::db]
impl ruff_db::Db for TestDb {
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
        PythonVersion::latest()
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

#[salsa::db]
impl salsa::Database for TestDb {}

struct RuffTestFile<'a> {
    test_file: TestFile<'a>,
    path: SystemPathBuf,
}

#[expect(clippy::needless_pass_by_value)]
fn mdtest(fixture_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let short_title = fixture_path
        .file_name()
        .ok_or_else(|| anyhow!("Expected fixture path to have a file name"))?;

    let crate_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let absolute_fixture_path = crate_dir.join(fixture_path);
    let workspace_relative_fixture_path = Utf8Path::new("crates/ruff_linter")
        .join(fixture_path.strip_prefix(".").unwrap_or(fixture_path));

    let test_name = fixture_path
        .strip_prefix("./resources/test/mdtest")
        .unwrap_or(fixture_path)
        .as_str();

    run(
        &absolute_fixture_path,
        &workspace_relative_fixture_path,
        &content,
        short_title,
        test_name,
    )?;

    Ok(())
}

fn run(
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    source: &str,
    short_title: &str,
    test_name: &str,
) -> anyhow::Result<()> {
    let output_format = output_format();

    let suite =
        parse(short_title, source).map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

    let filter = std::env::var(MDTEST_TEST_FILTER).ok();
    let mut markdown_edits = Vec::new();
    let mut any_failures = false;
    let mut assertion = String::new();

    for test in suite.tests() {
        if filter
            .as_ref()
            .is_some_and(|f| !(test.uncontracted_name().contains(f) || test.name() == *f))
        {
            continue;
        }

        let result = run_test(&test);
        let this_test_failed = result.is_err();
        any_failures |= this_test_failed;

        if this_test_failed && output_format.is_cli() {
            let _ = writeln!(assertion, "\n\n{}\n", test.name().bold().underline());
        }

        match result {
            Ok(edits) => markdown_edits.extend(edits),
            Err(failures) => {
                let md_index = LineIndex::from_source_text(source);

                for test_failures in failures {
                    let source_map =
                        EmbeddedFileSourceMap::new(&md_index, test_failures.backtick_offsets);

                    for (relative_line_number, failures) in test_failures.by_line.iter() {
                        let file = relative_fixture_path.as_str();

                        let absolute_line_number =
                            match source_map.to_absolute_line_number(relative_line_number) {
                                Ok(line_number) => line_number,
                                Err(last_line_number) => {
                                    output_format.write_error(
                                        &mut assertion,
                                        file,
                                        last_line_number,
                                        &Failure::new(
                                            "Found a trailing assertion comment \
                                            (e.g., `# revealed:` or `# error:`) \
                                            not followed by any statement.",
                                        ),
                                    );

                                    continue;
                                }
                            };

                        for failure in failures {
                            output_format.write_error(
                                &mut assertion,
                                file,
                                absolute_line_number,
                                failure,
                            );
                        }
                    }
                }
            }
        }

        if this_test_failed && output_format.is_cli() {
            let escaped_test_name = test.name().replace('\'', "\\'");
            let _ = writeln!(
                assertion,
                "\nTo rerun this specific test, \
                set the environment variable: {MDTEST_TEST_FILTER}='{escaped_test_name}'",
            );
            let _ = writeln!(
                assertion,
                "{MDTEST_TEST_FILTER}='{escaped_test_name}' cargo test -p ruff_linter \
                --test mdtest -- {test_name}",
            );

            let _ = writeln!(assertion, "\n{}", "-".repeat(50));
        }
    }

    if !markdown_edits.is_empty() {
        mdtest::try_apply_markdown_edits(absolute_fixture_path, source, markdown_edits);
    }

    assert!(!any_failures, "{}", &assertion);

    Ok(())
}

fn run_test(
    test: &parser::MarkdownTest<'_, '_, MarkdownTestConfig>,
) -> Result<Vec<MarkdownEdit>, Failures> {
    let mut db = TestDb::setup();
    let project_root = SystemPathBuf::from("/src");
    db.files()
        .try_add_root(&db, &project_root, FileRootKind::Project);

    let settings = settings(test.configuration())
        .with_context(|| format!("Invalid configuration for `{}`", test.uncontracted_name()))
        .unwrap();

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(embedded.lang, "py" | "python" | "pyi"),
                "Supported file types are: py (or python), pyi and ignore"
            );

            let full_path = embedded.full_path(&project_root);
            db.write_file(&full_path, &embedded.code).unwrap();
            let file = system_path_to_file(&db, &full_path).unwrap();

            Some(RuffTestFile {
                test_file: TestFile {
                    file,
                    code_blocks: embedded.python_code_blocks.clone(),
                },
                path: full_path,
            })
        })
        .collect();

    let mut markdown_edits = Vec::new();
    let failures: Failures = test_files
        .iter()
        .filter_map(|ruff_test_file| {
            let diagnostics = lint_file(&db, ruff_test_file, &settings);

            match matcher::match_file(&db, ruff_test_file.test_file.file, &diagnostics).and_then(
                |inline_diagnostics| {
                    mdtest::validate_inline_snapshot(
                        &db,
                        "ruff",
                        &ruff_test_file.test_file,
                        &inline_diagnostics,
                        &mut markdown_edits,
                    )
                },
            ) {
                Ok(()) => None,
                Err(by_line) => Some(FileFailures {
                    backtick_offsets: ruff_test_file.test_file.to_code_block_backtick_offsets(),
                    by_line,
                }),
            }
        })
        .collect();

    if failures.is_empty() {
        Ok(markdown_edits)
    } else {
        Err(failures)
    }
}

fn lint_file(
    db: &TestDb,
    ruff_test_file: &RuffTestFile,
    settings: &LinterSettings,
) -> Vec<ruff_db::diagnostic::Diagnostic> {
    let path = ruff_test_file.path.as_std_path();
    let source_type = PySourceType::from(path);
    let source = source_text(db, ruff_test_file.test_file.file);
    let source_kind = SourceKind::Python {
        code: source.as_str().to_string(),
        is_stub: source_type.is_stub(),
    };

    lint_only(
        path,
        None,
        settings,
        flags::Noqa::Enabled,
        &source_kind,
        source_type,
        ParseSource::None,
    )
    .diagnostics
}

fn settings(config: &MarkdownTestConfig) -> anyhow::Result<LinterSettings> {
    if config.rules.is_empty() {
        bail!("Expected at least one rule in `rules`");
    }

    let rules = config
        .rules
        .iter()
        .map(|rule| Rule::from_code(rule).with_context(|| format!("Unknown rule code `{rule}`")))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(LinterSettings::for_rules(rules))
}

fn parse<'s>(
    short_title: &'s str,
    source: &'s str,
) -> anyhow::Result<parser::MarkdownTestSuite<'s, MarkdownTestConfig>> {
    parser::parse::<MarkdownTestConfig>(short_title, source, |config| {
        settings(config)?;
        Ok(())
    })
}

datatest_stable::harness! {
    { test = mdtest, root = "./resources/test/mdtest", pattern = r"\.md$" },
}
