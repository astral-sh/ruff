use std::fmt::Write;

use anyhow::anyhow;
use camino::Utf8Path;
use colored::Colorize;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use mdtest::parser::{EmbeddedFileSourceMap, MdtestConfig};
use mdtest::{Failures, FileFailures, TestFile};
use ruff_db::Db as _;
use ruff_db::files::{FileRootKind, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{DbWithWritableSystem as _, SystemPathBuf};
use ruff_linter::message::EmitterContext;
use ruff_linter::source_kind::SourceKind;
use ruff_linter::test::test_contents;
use ruff_source_file::LineIndex;
use ruff_workspace::configuration::Configuration;
use ruff_workspace::options::Options;
use ty_module_resolver::SearchPaths;
use ty_python_core::platform::PythonPlatform;
use ty_python_core::program::{Program, ProgramSettings};
use ty_python_semantic::PythonVersionWithSource;
use ty_static::EnvVars;

pub use mdtest::OutputFormat;

mod db;

#[derive(Clone, Default, Deserialize)]
#[serde(transparent)]
struct RuffOptions(Options);

impl MdtestConfig for RuffOptions {
    fn has_dependencies(&self) -> bool {
        false
    }
}

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
pub fn run(
    relative_fixture_path: &Utf8Path,
    source: &str,
    snapshot_path: &Utf8Path,
    short_title: &str,
    test_name: &str,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let suite = mdtest::parser::parse::<RuffOptions>(short_title, source)
        .map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

    let mut db = db::Db::setup();

    let filter = std::env::var(EnvVars::MDTEST_TEST_FILTER).ok();
    let mut any_failures = false;
    let mut assertion = String::new();
    for test in suite.tests() {
        if filter
            .as_ref()
            .is_some_and(|f| !(test.uncontracted_name().contains(f) || test.name() == *f))
        {
            continue;
        }

        let result = run_test(&mut db, relative_fixture_path, snapshot_path, &test);

        let this_test_failed = result.is_err();
        any_failures = any_failures || this_test_failed;

        if this_test_failed && output_format.is_cli() {
            let _ = writeln!(assertion, "\n\n{}\n", test.name().bold().underline());
        }

        if let Err(failures) = result {
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
                                    "Found a trailing assertion comment \
                                        (e.g., `# revealed:` or `# error:`) \
                                        not followed by any statement.",
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

        if this_test_failed && output_format.is_cli() {
            let escaped_test_name = test.name().replace('\'', "\\'");
            let _ = writeln!(
                assertion,
                "\nTo rerun this specific test, \
                set the environment variable: {}='{escaped_test_name}'",
                EnvVars::MDTEST_TEST_FILTER,
            );
            let _ = writeln!(
                assertion,
                "{}='{escaped_test_name}' cargo test -p ruff_linter \
                --test mdtest -- {test_name}",
                EnvVars::MDTEST_TEST_FILTER,
            );

            let _ = writeln!(assertion, "\n{}", "-".repeat(50));
        }
    }

    assert!(!any_failures, "{}", &assertion);

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestOutcome {
    Success,
}

fn run_test(
    db: &mut db::Db,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &mdtest::parser::MarkdownTest<RuffOptions>,
) -> Result<TestOutcome, Failures> {
    // Initialize the system and remove all files and directories to reset the system to a clean state.
    db.use_in_memory_system();

    let project_root = SystemPathBuf::from("/src");
    db.create_directory_all(&project_root)
        .expect("Creating the project root to succeed");
    db.files()
        .try_add_root(db, &project_root, FileRootKind::Project);

    let src_path = project_root.clone();

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(
                    embedded.lang,
                    "py" | "pyi" | "python" | "text" | "cfg" | "pth"
                ),
                "Supported file types are: py (or python), pyi, text, cfg and ignore"
            );

            let full_path = embedded.full_path(&project_root);

            let temp_string;
            let to_write = if embedded.lang == "pth" && !embedded.code.starts_with('/') {
                // Make any relative .pths be relative to src_path
                temp_string = format!("{src_path}/{}", embedded.code);
                &*temp_string
            } else {
                &*embedded.code
            };

            db.write_file(&full_path, to_write).unwrap();

            if !(full_path.starts_with(&src_path)
                && matches!(embedded.lang, "py" | "python" | "pyi"))
            {
                // These files need to be written to the file system (above), but we don't run any checks on them.
                return None;
            }

            let file = system_path_to_file(db, full_path).unwrap();

            Some(TestFile {
                file,
                backtick_offsets: embedded.backtick_offsets.clone(),
            })
        })
        .collect();

    let settings = Configuration::from_options(
        test.configuration().0.clone(),
        None,
        project_root.as_std_path(),
    )
    .expect("Failed to construct configuration from options")
    .into_settings(project_root.as_std_path())
    .expect("Failed to construct settings");

    let program_settings = ProgramSettings {
        python_version: PythonVersionWithSource::default(),
        search_paths: SearchPaths::empty(ty_vendored::file_system()),
        python_platform: PythonPlatform::All,
    };

    Program::init_or_update(db, program_settings);

    // When snapshot testing is enabled, this is populated with
    // all diagnostics. Otherwise it remains empty.
    let mut snapshot_diagnostics = vec![];

    let failures: Failures = test_files
        .iter()
        .filter_map(|test_file| {
            let source_kind = SourceKind::Python {
                code: source_text(db, test_file.file).as_str().to_string(),
                is_stub: test_file.file.is_stub(db),
            };
            let path = test_file
                .file
                .path(db)
                .as_system_path()
                .expect("mdtest files are on the system")
                .as_std_path();
            let mut diagnostics = test_contents(&source_kind, path, &settings.linter).0;

            diagnostics.sort_by(|left, right| {
                left.rendering_sort_key(db)
                    .cmp(&right.rendering_sort_key(db))
            });

            let failure = match mdtest::matcher::match_file(db, test_file.file, &diagnostics) {
                Ok(()) => None,
                Err(line_failures) => Some(FileFailures {
                    backtick_offsets: test_file.backtick_offsets.clone(),
                    by_line: line_failures,
                }),
            };

            if test.should_snapshot_diagnostics() {
                snapshot_diagnostics.extend(diagnostics);
            }

            failure
        })
        .collect();

    if snapshot_diagnostics.is_empty() && test.should_snapshot_diagnostics() {
        panic!(
            "Test `{}` requested snapshotting diagnostics but it didn't produce any.",
            test.name()
        );
    } else if !snapshot_diagnostics.is_empty() {
        let notebook_indexes = FxHashMap::default();
        let resolver = EmitterContext::new(&notebook_indexes);
        let snapshot = mdtest::create_diagnostic_snapshot(
            &resolver,
            "ruff",
            relative_fixture_path,
            test,
            snapshot_diagnostics,
        );
        let name = test.name().replace(' ', "_").replace(':', "__");
        insta::with_settings!(
            {
                snapshot_path => snapshot_path,
                input_file => name.clone(),
                filters => vec![(r"\\", "/")],
                prepend_module_to_snapshot => false,
            },
            { insta::assert_snapshot!(name, snapshot) }
        );
    }

    if failures.is_empty() {
        Ok(TestOutcome::Success)
    } else {
        Err(failures)
    }
}
