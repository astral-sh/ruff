use std::fmt::Write;

use anyhow::anyhow;
use camino::Utf8Path;
use colored::Colorize;

use db::Db;
use mdtest::matcher::{self, Failure};
use mdtest::parser::{self, EmbeddedFileSourceMap};
use mdtest::{
    Failures, FileFailures, MDTEST_TEST_FILTER, MarkdownEdit, OutputFormat, TestFile, attempt_test,
    output_format,
};
use ruff_db::Db as _;
use ruff_db::diagnostic::{FileResolver, Input, UnifiedFile};
use ruff_db::files::{File, FileRootKind, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{DbWithWritableSystem as _, SystemPathBuf};
use ruff_linter::source_kind::SourceKind;
use ruff_linter::test::test_contents;
use ruff_notebook::NotebookIndex;
use ruff_source_file::LineIndex;
use ruff_workspace::configuration::Configuration;
use ruff_workspace::options::Options;

mod db;

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
pub fn run(
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    source: &str,
    snapshot_path: &Utf8Path,
    short_title: &str,
    test_name: &str,
    crate_name: &str,
) -> anyhow::Result<()> {
    let output_format = output_format();

    let suite =
        parse(short_title, source).map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

    let mut db = Db::setup();
    let mut markdown_edits = vec![];

    let filter = std::env::var(MDTEST_TEST_FILTER).ok();
    let mut any_failures = false;
    let mut assertion = String::new();
    for test in suite.tests() {
        if filter
            .as_ref()
            .is_some_and(|f| !(test.uncontracted_name().contains(f) || test.name() == *f))
        {
            continue;
        }

        let result = run_test(
            &mut db,
            absolute_fixture_path,
            relative_fixture_path,
            snapshot_path,
            &test,
            &mut assertion,
            output_format,
        );

        let this_test_failed = result.is_err();
        any_failures = any_failures || this_test_failed;

        if this_test_failed && output_format.is_cli() {
            let _ = writeln!(assertion, "\n\n{}\n", test.name().bold().underline());
        }

        match result {
            Ok((_, edits)) => markdown_edits.extend(edits),
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
                "{MDTEST_TEST_FILTER}='{escaped_test_name}' cargo test -p {crate_name} \
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestOutcome {
    Success,
}

fn run_test(
    db: &mut Db,
    _absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &parser::MarkdownTest<Options>,
    _assertion: &mut String,
    _output_format: OutputFormat,
) -> Result<(TestOutcome, Vec<MarkdownEdit>), Failures> {
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
                code_blocks: embedded.python_code_blocks.clone(),
            })
        })
        .collect();

    let settings = Configuration::from_options(
        test.configuration().clone(),
        None,
        project_root.as_std_path(),
    )
    .expect("Failed to construct configuration from options")
    .into_settings(project_root.as_std_path())
    .expect("Failed to construct settings");

    let mut all_diagnostics = vec![];

    // Edits for updating changed inline snapshots.
    let mut markdown_edits = vec![];

    let resolver = RuffResolver(db);

    let mut panic_info = None;

    let failures: Failures = test_files
        .iter()
        .filter_map(|test_file| {
            let mdtest_result = attempt_test(
                |file| {
                    let source_kind = SourceKind::Python {
                        code: source_text(db, file).as_str().to_string(),
                        is_stub: file.is_stub(db),
                    };
                    let path = file
                        .path(db)
                        .as_system_path()
                        .expect("mdtest files are on the system")
                        .as_std_path();
                    test_contents(&source_kind, path, &settings.linter).0
                },
                test_file,
            );

            let diagnostics = match mdtest_result {
                Ok(diagnostics) => diagnostics,
                Err(failures) => {
                    if test.should_expect_panic().is_ok() {
                        panic_info = Some(failures.info);
                        return None;
                    }

                    return Some(failures.into_file_failures(db, "run mdtest", None));
                }
            };

            let failure = match matcher::match_file(db, test_file.file, &diagnostics).and_then(
                |inline_diagnostics| {
                    mdtest::validate_inline_snapshot(
                        &resolver,
                        "ruff",
                        test_file,
                        &inline_diagnostics,
                        &mut markdown_edits,
                    )
                },
            ) {
                Ok(()) => None,
                Err(line_failures) => Some(FileFailures {
                    backtick_offsets: test_file.to_code_block_backtick_offsets(),
                    by_line: line_failures,
                }),
            };

            if test.should_snapshot_diagnostics() {
                all_diagnostics.extend(diagnostics);
            }

            failure
        })
        .collect();

    if all_diagnostics.is_empty() && test.should_snapshot_diagnostics() {
        panic!(
            "Test `{}` requested snapshotting diagnostics but it didn't produce any.",
            test.name()
        );
    } else if !all_diagnostics.is_empty() {
        let snapshot = mdtest::create_diagnostic_snapshot(
            &resolver,
            "ruff",
            relative_fixture_path,
            test,
            all_diagnostics.iter(),
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
        Ok((TestOutcome::Success, markdown_edits))
    } else {
        Err(failures)
    }
}

/// Wrap the db to avoid panicking when provided a Ruff file like the blanket `FileResolver`
/// implementation.
struct RuffResolver<'a>(&'a Db);

impl FileResolver for RuffResolver<'_> {
    fn path(&self, file: File) -> &str {
        self.0.path(file)
    }

    fn input(&self, file: File) -> Input {
        self.0.input(file)
    }

    fn current_directory(&self) -> &std::path::Path {
        self.0.current_directory()
    }

    fn notebook_index(&self, _file: &UnifiedFile) -> Option<NotebookIndex> {
        None
    }

    fn is_notebook(&self, _file: &UnifiedFile) -> bool {
        false
    }
}

fn parse<'s>(
    short_title: &'s str,
    source: &'s str,
) -> anyhow::Result<parser::MarkdownTestSuite<'s, Options>> {
    parser::parse::<Options>(short_title, source, |_| Ok(()))
}
