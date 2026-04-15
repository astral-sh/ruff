use std::fmt::Write;
use std::path::Path;

use anyhow::anyhow;
use camino::Utf8Path;
use colored::Colorize;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use mdtest::matcher::{self, Failure};
use mdtest::parser::{EmbeddedFileSourceMap, MdtestConfig};
use mdtest::{Failures, FileFailures, MDTEST_TEST_FILTER, MarkdownEdit, TestFile, output_format};
use ruff_db::diagnostic::{FileResolver, UnifiedFile};
use ruff_db::system::SystemPathBuf;
use ruff_linter::message::EmitterContext;
use ruff_linter::source_kind::SourceKind;
use ruff_linter::test::test_contents;
use ruff_source_file::LineIndex;
use ruff_source_file::SourceFileBuilder;
use ruff_workspace::configuration::Configuration;
use ruff_workspace::options::Options;

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
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    source: &str,
    snapshot_path: &Utf8Path,
    short_title: &str,
    test_name: &str,
) -> anyhow::Result<()> {
    let output_format = output_format();

    let suite = mdtest::parser::parse::<RuffOptions>(short_title, source)
        .map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

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

        let result = run_test(relative_fixture_path, snapshot_path, &test);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestOutcome {
    Success,
}

fn run_test(
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &mdtest::parser::MarkdownTest<RuffOptions>,
) -> Result<(TestOutcome, Vec<MarkdownEdit>), Failures> {
    let project_root = SystemPathBuf::from("/src");
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

            if !(full_path.starts_with(&src_path)
                && matches!(embedded.lang, "py" | "python" | "pyi"))
            {
                // These files can be referenced by a test configuration, but we don't run checks on them.
                return None;
            }

            Some(TestFile {
                file: UnifiedFile::Ruff(
                    SourceFileBuilder::new(full_path.as_str(), embedded.code.as_ref()).finish(),
                ),
                code_blocks: embedded.python_code_blocks.clone(),
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

    // When snapshot testing is enabled, this is populated with
    // all diagnostics. Otherwise it remains empty.
    let mut snapshot_diagnostics = vec![];

    // Edits for updating changed inline snapshots.
    let mut markdown_edits = vec![];

    // Construct a separate `FileResolver` for rendering diagnostics.
    let notebook_indexes = FxHashMap::default();
    let resolver = EmitterContext::new(&notebook_indexes);

    let failures: Failures = test_files
        .iter()
        .filter_map(|test_file| {
            let UnifiedFile::Ruff(source_file) = &test_file.file else {
                unreachable!("ruff mdtests should always use Ruff files")
            };

            let source_kind = SourceKind::Python {
                code: source_file.source_text().to_string(),
                is_stub: Path::new(source_file.name())
                    .extension()
                    .is_some_and(|ext| ext == "pyi"),
            };
            let path = Path::new(source_file.name());
            let (mut diagnostics, _, parsed) = test_contents(&source_kind, path, &settings.linter);
            let resolver: &dyn FileResolver = &resolver;

            diagnostics.sort_by(|left, right| {
                left.rendering_sort_key(resolver)
                    .cmp(&right.rendering_sort_key(resolver))
            });

            let failure =
                match matcher::match_file(resolver, &test_file.file, parsed.tokens(), &diagnostics)
                    .and_then(|inline_diagnostics| {
                        mdtest::validate_inline_snapshot(
                            resolver,
                            "ruff",
                            test_file,
                            &inline_diagnostics,
                            &mut markdown_edits,
                        )
                    }) {
                    Ok(()) => None,
                    Err(line_failures) => Some(FileFailures {
                        backtick_offsets: test_file.to_code_block_backtick_offsets(),
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
        Ok((TestOutcome::Success, markdown_edits))
    } else {
        Err(failures)
    }
}
