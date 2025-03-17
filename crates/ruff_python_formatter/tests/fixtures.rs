use crate::normalizer::Normalizer;
use itertools::Itertools;
use ruff_formatter::FormatOptions;
use ruff_python_ast::comparable::ComparableMod;
use ruff_python_formatter::{format_module_source, format_range, PreviewMode, PyFormatOptions};
use ruff_python_parser::{parse, ParseOptions, UnsupportedSyntaxError};
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashMap;
use similar::TextDiff;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::fmt::{Formatter, Write};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::BufReader;
use std::ops::Range;
use std::path::Path;
use std::{fmt, fs};

mod normalizer;

#[test]
fn black_compatibility() {
    let test_file = |input_path: &Path| {
        let content = fs::read_to_string(input_path).unwrap();

        let options_path = input_path.with_extension("options.json");

        let options: PyFormatOptions = if let Ok(options_file) = fs::File::open(&options_path) {
            let reader = BufReader::new(options_file);
            serde_json::from_reader(reader).unwrap_or_else(|_| {
                panic!("Expected option file {options_path:?} to be a valid Json file")
            })
        } else {
            PyFormatOptions::from_extension(input_path)
        };

        let first_line = content.lines().next().unwrap_or_default();
        let formatted_code = if first_line.starts_with("# flags:")
            && first_line.contains("--line-ranges=")
        {
            let line_index = LineIndex::from_source_text(&content);

            let ranges = first_line
                .split_ascii_whitespace()
                .filter_map(|chunk| {
                    let (_, lines) = chunk.split_once("--line-ranges=")?;
                    let (lower, upper) = lines.split_once('-')?;

                    let lower = lower
                        .parse::<OneIndexed>()
                        .expect("Expected a valid line number");
                    let upper = upper
                        .parse::<OneIndexed>()
                        .expect("Expected a valid line number");

                    let range_start = line_index.line_start(lower, &content);
                    let range_end = line_index.line_end(upper, &content);

                    Some(TextRange::new(range_start, range_end))
                })
                .rev();

            let mut formatted_code = content.clone();

            for range in ranges {
                let formatted =
                    format_range(&content, range, options.clone()).unwrap_or_else(|err| {
                        panic!(
                            "Range-formatting of {} to succeed but encountered error {err}",
                            input_path.display()
                        )
                    });

                let range = formatted.source_range();

                formatted_code.replace_range(Range::<usize>::from(range), formatted.as_code());
            }

            // We can't do stability checks for range formatting because we don't know the updated rangs.

            formatted_code
        } else {
            let printed = format_module_source(&content, options.clone()).unwrap_or_else(|err| {
                panic!(
                    "Formatting of {} to succeed but encountered error {err}",
                    input_path.display()
                )
            });

            let formatted_code = printed.into_code();

            ensure_stability_when_formatting_twice(&formatted_code, &options, input_path);

            formatted_code
        };

        let extension = input_path
            .extension()
            .expect("Test file to have py or pyi extension")
            .to_string_lossy();
        let expected_path = input_path.with_extension(format!("{extension}.expect"));
        let expected_output = fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("Expected Black output file '{expected_path:?}' to exist"));

        ensure_unchanged_ast(&content, &formatted_code, &options, input_path);

        if formatted_code == expected_output {
            // Black and Ruff formatting matches. Delete any existing snapshot files because the Black output
            // already perfectly captures the expected output.
            // The following code mimics insta's logic generating the snapshot name for a test.
            let workspace_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();

            let mut components = input_path.components().rev();
            let file_name = components.next().unwrap();
            let test_suite = components.next().unwrap();

            let snapshot_name = format!(
                "black_compatibility@{}__{}.snap",
                test_suite.as_os_str().to_string_lossy(),
                file_name.as_os_str().to_string_lossy()
            );

            let snapshot_path = Path::new(&workspace_path)
                .join("tests/snapshots")
                .join(snapshot_name);
            if snapshot_path.exists() && snapshot_path.is_file() {
                // SAFETY: This is a convenience feature. That's why we don't want to abort
                // when deleting a no longer needed snapshot fails.
                fs::remove_file(&snapshot_path).ok();
            }

            let new_snapshot_path = snapshot_path.with_extension("snap.new");
            if new_snapshot_path.exists() && new_snapshot_path.is_file() {
                // SAFETY: This is a convenience feature. That's why we don't want to abort
                // when deleting a no longer needed snapshot fails.
                fs::remove_file(&new_snapshot_path).ok();
            }
        } else {
            // Black and Ruff have different formatting. Write out a snapshot that covers the differences
            // today.
            let mut snapshot = String::new();
            write!(snapshot, "{}", Header::new("Input")).unwrap();
            write!(snapshot, "{}", CodeFrame::new("python", &content)).unwrap();

            write!(snapshot, "{}", Header::new("Black Differences")).unwrap();

            let diff = TextDiff::from_lines(expected_output.as_str(), &formatted_code)
                .unified_diff()
                .header("Black", "Ruff")
                .to_string();

            write!(snapshot, "{}", CodeFrame::new("diff", &diff)).unwrap();

            write!(snapshot, "{}", Header::new("Ruff Output")).unwrap();
            write!(snapshot, "{}", CodeFrame::new("python", &formatted_code)).unwrap();

            write!(snapshot, "{}", Header::new("Black Output")).unwrap();
            write!(snapshot, "{}", CodeFrame::new("python", &expected_output)).unwrap();

            insta::with_settings!({
                omit_expression => true,
                input_file => input_path,
                prepend_module_to_snapshot => false,
            }, {
                insta::assert_snapshot!(snapshot);
            });
        }
    };

    insta::glob!(
        "../resources",
        "test/fixtures/black/**/*.{py,pyi}",
        test_file
    );
}

#[test]
fn format() {
    let test_file = |input_path: &Path| {
        let content = fs::read_to_string(input_path).unwrap();

        let mut snapshot = format!("## Input\n{}", CodeFrame::new("python", &content));
        let options_path = input_path.with_extension("options.json");

        if let Ok(options_file) = fs::File::open(&options_path) {
            let reader = BufReader::new(options_file);
            let options: Vec<PyFormatOptions> =
                serde_json::from_reader(reader).unwrap_or_else(|_| {
                    panic!("Expected option file {options_path:?} to be a valid Json file")
                });

            writeln!(snapshot, "## Outputs").unwrap();

            for (i, options) in options.into_iter().enumerate() {
                let formatted_code = format_file(&content, &options, input_path);

                writeln!(
                    snapshot,
                    "### Output {}\n{}{}",
                    i + 1,
                    CodeFrame::new("", &DisplayPyOptions(&options)),
                    CodeFrame::new("python", &formatted_code)
                )
                .unwrap();

                if options.preview().is_enabled() {
                    continue;
                }

                // We want to capture the differences in the preview style in our fixtures
                let options_preview = options.with_preview(PreviewMode::Enabled);
                let formatted_preview = format_file(&content, &options_preview, input_path);

                if formatted_code != formatted_preview {
                    // Having both snapshots makes it hard to see the difference, so we're keeping only
                    // diff.
                    writeln!(
                        snapshot,
                        "#### Preview changes\n{}",
                        CodeFrame::new(
                            "diff",
                            TextDiff::from_lines(&formatted_code, &formatted_preview)
                                .unified_diff()
                                .header("Stable", "Preview")
                        )
                    )
                    .unwrap();
                }
            }
        } else {
            // We want to capture the differences in the preview style in our fixtures
            let options = PyFormatOptions::from_extension(input_path);
            let formatted_code = format_file(&content, &options, input_path);

            let options_preview = options.with_preview(PreviewMode::Enabled);
            let formatted_preview = format_file(&content, &options_preview, input_path);

            if formatted_code == formatted_preview {
                writeln!(
                    snapshot,
                    "## Output\n{}",
                    CodeFrame::new("python", &formatted_code)
                )
                .unwrap();
            } else {
                // Having both snapshots makes it hard to see the difference, so we're keeping only
                // diff.
                writeln!(
                    snapshot,
                    "## Output\n{}\n## Preview changes\n{}",
                    CodeFrame::new("python", &formatted_code),
                    CodeFrame::new(
                        "diff",
                        TextDiff::from_lines(&formatted_code, &formatted_preview)
                            .unified_diff()
                            .header("Stable", "Preview")
                    )
                )
                .unwrap();
            }
        }

        insta::with_settings!({
            omit_expression => true,
            input_file => input_path,
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_snapshot!(snapshot);
        });
    };

    insta::glob!(
        "../resources",
        "test/fixtures/ruff/**/*.{py,pyi}",
        test_file
    );
}

fn format_file(source: &str, options: &PyFormatOptions, input_path: &Path) -> String {
    let (unformatted, formatted_code) = if source.contains("<RANGE_START>") {
        let mut content = source.to_string();
        let without_markers = content
            .replace("<RANGE_START>", "")
            .replace("<RANGE_END>", "");

        while let Some(range_start_marker) = content.find("<RANGE_START>") {
            // Remove the start marker
            content.replace_range(
                range_start_marker..range_start_marker + "<RANGE_START>".len(),
                "",
            );

            let range_end_marker = content[range_start_marker..]
                .find("<RANGE_END>")
                .expect("Matching <RANGE_END> marker for <RANGE_START> to exist")
                + range_start_marker;

            content.replace_range(range_end_marker..range_end_marker + "<RANGE_END>".len(), "");

            // Replace all other markers to get a valid Python input
            let format_input = content
                .replace("<RANGE_START>", "")
                .replace("<RANGE_END>", "");

            let range = TextRange::new(
                TextSize::try_from(range_start_marker).unwrap(),
                TextSize::try_from(range_end_marker).unwrap(),
            );

            let formatted =
                format_range(&format_input, range, options.clone()).unwrap_or_else(|err| {
                    panic!(
                        "Range-formatting of {} to succeed but encountered error {err}",
                        input_path.display()
                    )
                });

            content.replace_range(
                Range::<usize>::from(formatted.source_range()),
                formatted.as_code(),
            );
        }

        (Cow::Owned(without_markers), content)
    } else {
        let printed = format_module_source(source, options.clone()).expect("Formatting to succeed");
        let formatted_code = printed.into_code();

        ensure_stability_when_formatting_twice(&formatted_code, options, input_path);

        (Cow::Borrowed(source), formatted_code)
    };

    ensure_unchanged_ast(&unformatted, &formatted_code, options, input_path);

    formatted_code
}

/// Format another time and make sure that there are no changes anymore
fn ensure_stability_when_formatting_twice(
    formatted_code: &str,
    options: &PyFormatOptions,
    input_path: &Path,
) {
    let reformatted = match format_module_source(formatted_code, options.clone()) {
        Ok(reformatted) => reformatted,
        Err(err) => {
            panic!(
                "Expected formatted code of {} to be valid syntax: {err}:\
                    \n---\n{formatted_code}---\n",
                input_path.display()
            );
        }
    };

    if reformatted.as_code() != formatted_code {
        let diff = TextDiff::from_lines(formatted_code, reformatted.as_code())
            .unified_diff()
            .header("Formatted once", "Formatted twice")
            .to_string();
        panic!(
            r#"Reformatting the formatted code of {input_path} a second time resulted in formatting changes.

Options:
{options}
---
{diff}---

Formatted once:
---
{formatted_code}---

Formatted twice:
---
{reformatted}---"#,
            input_path = input_path.display(),
            options = &DisplayPyOptions(options),
            reformatted = reformatted.as_code(),
        );
    }
}

/// Ensure that formatting doesn't change the AST and doesn't introduce any new unsupported syntax errors.
///
/// Like Black, there are a few exceptions to this "invariant" which are encoded in
/// [`NormalizedMod`] and related structs. Namely, formatting can change indentation within strings,
/// and can also flatten tuples within `del` statements.
fn ensure_unchanged_ast(
    unformatted_code: &str,
    formatted_code: &str,
    options: &PyFormatOptions,
    input_path: &Path,
) {
    let source_type = options.source_type();

    // Parse the unformatted code.
    let unformatted_parsed = parse(
        unformatted_code,
        ParseOptions::from(source_type).with_target_version(options.target_version()),
    )
    .expect("Unformatted code to be valid syntax");

    let unformatted_unsupported_syntax_errors =
        collect_unsupported_syntax_errors(unformatted_parsed.unsupported_syntax_errors());
    let mut unformatted_ast = unformatted_parsed.into_syntax();

    Normalizer.visit_module(&mut unformatted_ast);
    let unformatted_ast = ComparableMod::from(&unformatted_ast);

    // Parse the formatted code.
    let formatted_parsed = parse(
        formatted_code,
        ParseOptions::from(source_type).with_target_version(options.target_version()),
    )
    .expect("Formatted code to be valid syntax");

    // Assert that there are no new unsupported syntax errors
    let mut formatted_unsupported_syntax_errors =
        collect_unsupported_syntax_errors(formatted_parsed.unsupported_syntax_errors());

    formatted_unsupported_syntax_errors
        .retain(|fingerprint, _| !unformatted_unsupported_syntax_errors.contains_key(fingerprint));

    if !formatted_unsupported_syntax_errors.is_empty() {
        let index = LineIndex::from_source_text(formatted_code);
        panic!(
            "Formatted code `{}` introduced new unsupported syntax errors:\n---\n{}\n---",
            input_path.display(),
            formatted_unsupported_syntax_errors
                .into_values()
                .map(|error| {
                    let location = index.source_location(error.start(), formatted_code);
                    format!(
                        "{row}:{col} {error}",
                        row = location.row,
                        col = location.column
                    )
                })
                .join("\n")
        );
    }

    let mut formatted_ast = formatted_parsed.into_syntax();
    Normalizer.visit_module(&mut formatted_ast);
    let formatted_ast = ComparableMod::from(&formatted_ast);

    if formatted_ast != unformatted_ast {
        let diff = TextDiff::from_lines(
            &format!("{unformatted_ast:#?}"),
            &format!("{formatted_ast:#?}"),
        )
        .unified_diff()
        .header("Unformatted", "Formatted")
        .to_string();
        panic!(
            r#"Reformatting the unformatted code of {} resulted in AST changes.
---
{diff}
"#,
            input_path.display(),
        );
    }
}

struct Header<'a> {
    title: &'a str,
}

impl<'a> Header<'a> {
    fn new(title: &'a str) -> Self {
        Self { title }
    }
}

impl std::fmt::Display for Header<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "## {}", self.title)?;
        writeln!(f)
    }
}

struct CodeFrame<'a> {
    language: &'a str,
    code: &'a dyn std::fmt::Display,
}

impl<'a> CodeFrame<'a> {
    fn new(language: &'a str, code: &'a dyn std::fmt::Display) -> Self {
        Self { language, code }
    }
}

impl std::fmt::Display for CodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "```{}", self.language)?;
        write!(f, "{}", self.code)?;
        writeln!(f, "```")?;
        writeln!(f)
    }
}

struct DisplayPyOptions<'a>(&'a PyFormatOptions);

impl fmt::Display for DisplayPyOptions<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            r#"indent-style               = {indent_style}
line-width                 = {line_width}
indent-width               = {indent_width}
quote-style                = {quote_style:?}
line-ending                = {line_ending:?}
magic-trailing-comma       = {magic_trailing_comma:?}
docstring-code             = {docstring_code:?}
docstring-code-line-width  = {docstring_code_line_width:?}
preview                    = {preview:?}
target_version             = {target_version}
source_type                = {source_type:?}"#,
            indent_style = self.0.indent_style(),
            indent_width = self.0.indent_width().value(),
            line_width = self.0.line_width().value(),
            quote_style = self.0.quote_style(),
            line_ending = self.0.line_ending(),
            magic_trailing_comma = self.0.magic_trailing_comma(),
            docstring_code = self.0.docstring_code(),
            docstring_code_line_width = self.0.docstring_code_line_width(),
            preview = self.0.preview(),
            target_version = self.0.target_version(),
            source_type = self.0.source_type()
        )
    }
}

/// Collects the unsupported syntax errors and assigns a unique hash to each error.
fn collect_unsupported_syntax_errors(
    errors: &[UnsupportedSyntaxError],
) -> FxHashMap<u64, UnsupportedSyntaxError> {
    let mut collected = FxHashMap::default();

    for error in errors {
        let mut error_fingerprint = fingerprint_unsupported_syntax_error(error, 0);

        // Make sure that we do not get a fingerprint that is already in use
        // by adding in the previously generated one.
        loop {
            match collected.entry(error_fingerprint) {
                Entry::Occupied(_) => {
                    error_fingerprint =
                        fingerprint_unsupported_syntax_error(error, error_fingerprint);
                }
                Entry::Vacant(entry) => {
                    entry.insert(error.clone());
                    break;
                }
            }
        }
    }

    collected
}

fn fingerprint_unsupported_syntax_error(error: &UnsupportedSyntaxError, salt: u64) -> u64 {
    let mut hasher = DefaultHasher::new();

    let UnsupportedSyntaxError {
        kind,
        target_version,
        // Don't hash the range because the location between the formatted and unformatted code
        // is likely to be different
        range: _,
    } = error;

    salt.hash(&mut hasher);
    kind.hash(&mut hasher);
    target_version.hash(&mut hasher);

    hasher.finish()
}
