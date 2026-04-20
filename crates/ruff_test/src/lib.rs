use anyhow::anyhow;
use camino::Utf8Path;

use db::Db;
use mdtest::matcher::{self};
use mdtest::parser::{self};
use mdtest::{Failures, FileFailures, MarkdownEdit, TestFile, TestOutcome, attempt_test};
use ruff_db::Db as _;
use ruff_db::diagnostic::{FileResolver, Input, UnifiedFile};
use ruff_db::files::{File, FileRootKind, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{DbWithWritableSystem as _, SystemPathBuf};
use ruff_linter::source_kind::SourceKind;
use ruff_linter::test::test_contents;
use ruff_notebook::NotebookIndex;
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
    let suite =
        parse(short_title, source).map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

    let mut db = Db::setup();

    mdtest::run(
        absolute_fixture_path,
        relative_fixture_path,
        source,
        test_name,
        crate_name,
        &suite,
        |test, _assertion, _output_format| {
            run_test(&mut db, relative_fixture_path, snapshot_path, test)
        },
    )
}

fn run_test(
    db: &mut Db,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &parser::MarkdownTest<Options>,
) -> Result<(TestOutcome, Vec<MarkdownEdit>), Failures> {
    // Initialize the system and remove all files and directories to reset the system to a clean state.
    db.use_in_memory_system();

    let project_root = SystemPathBuf::from("/src");
    db.create_directory_all(&project_root)
        .expect("Creating the project root to succeed");
    db.files()
        .try_add_root(db, &project_root, FileRootKind::Project);

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(embedded.lang, "py" | "pyi" | "python"),
                "Supported file types are: py (or python), pyi, and ignore"
            );

            let full_path = embedded.full_path(&project_root);

            db.write_file(&full_path, &*embedded.code).unwrap();

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

            all_diagnostics.extend(diagnostics);

            failure
        })
        .collect();

    test.check_panic(panic_info);
    test.snapshot_diagnostics(
        db,
        "ruff",
        relative_fixture_path,
        snapshot_path,
        &all_diagnostics,
        |_| true,
    );

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
