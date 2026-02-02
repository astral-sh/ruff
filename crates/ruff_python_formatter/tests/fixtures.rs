use crate::normalizer::Normalizer;
use anyhow::anyhow;
use datatest_stable::Utf8Path;
use insta::assert_snapshot;
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig,
    DisplayDiagnostics, DummyFileResolver, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_formatter::FormatOptions;
use ruff_python_ast::Mod;
use ruff_python_ast::comparable::ComparableMod;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_formatter::{PreviewMode, PyFormatOptions, format_module_source, format_range};
use ruff_python_parser::{ParseOptions, Parsed, UnsupportedSyntaxError, parse};
use ruff_source_file::{LineIndex, OneIndexed, SourceFileBuilder};
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

#[expect(clippy::needless_pass_by_value)]
fn black_compatibility(input_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let test_name = input_path
        .strip_prefix("./resources/test/fixtures/black")
        .unwrap_or(input_path)
        .as_str();

    let options_path = input_path.with_extension("options.json");

    let options: PyFormatOptions = if let Ok(options_file) = fs::File::open(&options_path) {
        let reader = BufReader::new(options_file);
        serde_json::from_reader(reader).map_err(|err| {
            anyhow!("Expected option file {options_path:?} to be a valid Json file: {err}")
        })?
    } else {
        PyFormatOptions::from_extension(input_path.as_std_path())
    };

    let first_line = content.lines().next().unwrap_or_default();
    let formatted_code =
        if first_line.starts_with("# flags:") && first_line.contains("--line-ranges=") {
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
                let formatted = format_range(&content, range, options.clone()).map_err(|err| {
                    anyhow!("Range-formatting to succeed but encountered error {err}")
                })?;

                let range = formatted.source_range();

                formatted_code.replace_range(Range::<usize>::from(range), formatted.as_code());
            }

            // We can't do stability checks for range formatting because we don't know the updated rangs.

            formatted_code
        } else {
            let printed = format_module_source(&content, options.clone())
                .map_err(|err| anyhow!("Formatting to succeed but encountered error {err}"))?;

            let formatted_code = printed.into_code();

            ensure_stability_when_formatting_twice(&formatted_code, &options, input_path);

            formatted_code
        };

    let extension = input_path
        .extension()
        .expect("Test file to have py or pyi extension");
    let expected_path = input_path.with_extension(format!("{extension}.expect"));
    let expected_output = fs::read_to_string(&expected_path)
        .unwrap_or_else(|_| panic!("Expected Black output file '{expected_path:?}' to exist"));

    let unsupported_syntax_errors =
        ensure_unchanged_ast(&content, &formatted_code, &options, input_path);

    // Black and Ruff formatting matches. Delete any existing snapshot files because the Black output
    // already perfectly captures the expected output.
    // The following code mimics insta's logic generating the snapshot name for a test.
    let workspace_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    let full_snapshot_name = format!("black_compatibility@{test_name}.snap",);

    let snapshot_path = Path::new(&workspace_path)
        .join("tests/snapshots")
        .join(full_snapshot_name);

    if formatted_code == expected_output {
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

        if !unsupported_syntax_errors.is_empty() {
            write!(snapshot, "{}", Header::new("New Unsupported Syntax Errors")).unwrap();
            writeln!(
                snapshot,
                "{}",
                DisplayDiagnostics::new(
                    &DummyFileResolver,
                    &DisplayDiagnosticConfig::default().format(DiagnosticFormat::Full),
                    &unsupported_syntax_errors
                )
            )
            .unwrap();
        }

        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.set_input_file(input_path);
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_suffix(test_name);
        let _settings = settings.bind_to_scope();

        assert_snapshot!(snapshot);
    }
    Ok(())
}

#[expect(clippy::needless_pass_by_value)]
fn format(input_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let test_name = input_path
        .strip_prefix("./resources/test/fixtures/ruff")
        .unwrap_or(input_path)
        .as_str();

    let mut snapshot = format!("## Input\n{}", CodeFrame::new("python", &content));
    let options_path = input_path.with_extension("options.json");

    if let Ok(options_file) = fs::File::open(&options_path) {
        let reader = BufReader::new(options_file);
        let options: Vec<PyFormatOptions> = serde_json::from_reader(reader).map_err(|_| {
            anyhow!("Expected option file {options_path:?} to be a valid Json file")
        })?;

        writeln!(snapshot, "## Outputs").unwrap();

        for (i, options) in options.into_iter().enumerate() {
            let (formatted_code, unsupported_syntax_errors) =
                format_file(&content, &options, input_path);

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
            let (formatted_preview, _) = format_file(&content, &options_preview, input_path);

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

            if !unsupported_syntax_errors.is_empty() {
                writeln!(
                    snapshot,
                    "### Unsupported Syntax Errors\n{}",
                    DisplayDiagnostics::new(
                        &DummyFileResolver,
                        &DisplayDiagnosticConfig::default().format(DiagnosticFormat::Full),
                        &unsupported_syntax_errors
                    )
                )
                .unwrap();
            }
        }
    } else {
        // We want to capture the differences in the preview style in our fixtures
        let options = PyFormatOptions::from_extension(input_path.as_std_path());
        let (formatted_code, unsupported_syntax_errors) =
            format_file(&content, &options, input_path);

        let options_preview = options.with_preview(PreviewMode::Enabled);
        let (formatted_preview, _) = format_file(&content, &options_preview, input_path);

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

        if !unsupported_syntax_errors.is_empty() {
            writeln!(
                snapshot,
                "## Unsupported Syntax Errors\n{}",
                DisplayDiagnostics::new(
                    &DummyFileResolver,
                    &DisplayDiagnosticConfig::default().format(DiagnosticFormat::Full),
                    &unsupported_syntax_errors
                )
            )
            .unwrap();
        }
    }

    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    settings.set_input_file(input_path);
    settings.set_prepend_module_to_snapshot(false);
    settings.set_snapshot_suffix(test_name);
    let _settings = settings.bind_to_scope();

    assert_snapshot!(snapshot);

    Ok(())
}

datatest_stable::harness! {
    { test = black_compatibility, root = "./resources/test/fixtures/black", pattern = r".+\.pyi?$" },
    { test = format, root="./resources/test/fixtures/ruff", pattern = r".+\.pyi?$" }
}

fn format_file(
    source: &str,
    options: &PyFormatOptions,
    input_path: &Utf8Path,
) -> (String, Vec<Diagnostic>) {
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
                        "Range-formatting of {input_path} to succeed but encountered error {err}",
                    )
                });

            content.replace_range(
                Range::<usize>::from(formatted.source_range()),
                formatted.as_code(),
            );
        }

        (Cow::Owned(without_markers), content)
    } else {
        let printed = format_module_source(source, options.clone()).unwrap_or_else(|err| {
            panic!("Formatting `{input_path} was expected to succeed but it failed: {err}",)
        });
        let formatted_code = printed.into_code();

        ensure_stability_when_formatting_twice(&formatted_code, options, input_path);

        (Cow::Borrowed(source), formatted_code)
    };

    let unsupported_syntax_errors =
        ensure_unchanged_ast(&unformatted, &formatted_code, options, input_path);

    (formatted_code, unsupported_syntax_errors)
}

/// Format another time and make sure that there are no changes anymore
fn ensure_stability_when_formatting_twice(
    formatted_code: &str,
    options: &PyFormatOptions,
    input_path: &Utf8Path,
) {
    let reformatted = match format_module_source(formatted_code, options.clone()) {
        Ok(reformatted) => reformatted,
        Err(err) => {
            let mut diag = Diagnostic::from(&err);
            if let Some(range) = err.range() {
                let file = SourceFileBuilder::new(input_path.as_str(), formatted_code).finish();
                let span = Span::from(file).with_range(range);
                diag.annotate(Annotation::primary(span));
            }
            panic!(
                "Expected formatted code of {input_path} to be valid syntax: {err}:\
                    \n---\n{formatted_code}---\n{}",
                diag.display(&DummyFileResolver, &DisplayDiagnosticConfig::default()),
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
///
/// Returns any new [`UnsupportedSyntaxError`]s in the formatted code as [`Diagnostic`]s for
/// snapshotting.
///
/// As noted in the sub-diagnostic message, new syntax errors should only be accepted when they are
/// the result of an existing syntax error in the input. For example, the formatter knows that
/// escapes in f-strings are only allowed after Python 3.12, so it can replace escaped quotes with
/// reused outer quote characters, which are also valid after 3.12, even if the configured Python
/// version is lower. Such cases disrupt the fingerprint filter because the syntax error, and thus
/// its fingerprint, is different from the input syntax error. More typical cases like using a
/// t-string before 3.14 will be filtered out and not included in snapshots.
fn ensure_unchanged_ast(
    unformatted_code: &str,
    formatted_code: &str,
    options: &PyFormatOptions,
    input_path: &Utf8Path,
) -> Vec<Diagnostic> {
    let source_type = options.source_type();

    // Parse the unformatted code.
    let unformatted_parsed = parse(
        unformatted_code,
        ParseOptions::from(source_type).with_target_version(options.target_version()),
    )
    .expect("Unformatted code to be valid syntax");

    let unformatted_unsupported_syntax_errors =
        collect_unsupported_syntax_errors(&unformatted_parsed);
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
        collect_unsupported_syntax_errors(&formatted_parsed);

    formatted_unsupported_syntax_errors
        .retain(|fingerprint, _| !unformatted_unsupported_syntax_errors.contains_key(fingerprint));

    let file = SourceFileBuilder::new(input_path.file_name().unwrap(), formatted_code).finish();
    let diagnostics = formatted_unsupported_syntax_errors
        .values()
        .map(|error| {
            let mut diag = Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Error, error);
            let span = Span::from(file.clone()).with_range(error.range());
            diag.annotate(Annotation::primary(span));
            let sub = SubDiagnostic::new(
                SubDiagnosticSeverity::Warning,
                "Only accept new syntax errors if they are also present in the input. \
                    The formatter should not introduce syntax errors.",
            );
            diag.sub(sub);
            diag
        })
        .collect::<Vec<_>>();

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
            r#"Reformatting the unformatted code of {input_path} resulted in AST changes.
---
{diff}
"#,
        );
    }

    diagnostics
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

/// A visitor to collect a sequence of node IDs for fingerprinting [`UnsupportedSyntaxError`]s.
///
/// It visits each statement in the AST in source order and saves its range. The index of the node
/// enclosing a syntax error's range can then be retrieved with the `node_id` method. This `node_id`
/// should be stable across formatting runs since the formatter won't add or remove statements.
struct StmtVisitor {
    nodes: Vec<TextRange>,
}

impl StmtVisitor {
    fn new(parsed: &Parsed<Mod>) -> Self {
        let mut visitor = Self { nodes: Vec::new() };
        visitor.visit_mod(parsed.syntax());
        visitor
    }

    /// Return the index of the statement node that contains `range`.
    fn node_id(&self, range: TextRange) -> usize {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.contains_range(range))
            .min_by_key(|(_, node)| node.len())
            .expect("Expected an enclosing node in the AST")
            .0
    }
}

impl<'a> SourceOrderVisitor<'a> for StmtVisitor {
    fn visit_stmt(&mut self, stmt: &'a ruff_python_ast::Stmt) {
        self.nodes.push(stmt.range());
        ruff_python_ast::visitor::source_order::walk_stmt(self, stmt);
    }
}

/// Collects the unsupported syntax errors and assigns a unique hash to each error.
fn collect_unsupported_syntax_errors(
    parsed: &Parsed<Mod>,
) -> FxHashMap<u64, UnsupportedSyntaxError> {
    let mut collected = FxHashMap::default();

    if parsed.unsupported_syntax_errors().is_empty() {
        return collected;
    }

    let visitor = StmtVisitor::new(parsed);

    for error in parsed.unsupported_syntax_errors() {
        let node_id = visitor.node_id(error.range);
        let mut error_fingerprint = fingerprint_unsupported_syntax_error(error, node_id, 0);

        // Make sure that we do not get a fingerprint that is already in use
        // by adding in the previously generated one.
        loop {
            match collected.entry(error_fingerprint) {
                Entry::Occupied(_) => {
                    error_fingerprint =
                        fingerprint_unsupported_syntax_error(error, node_id, error_fingerprint);
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

fn fingerprint_unsupported_syntax_error(
    error: &UnsupportedSyntaxError,
    node_id: usize,
    salt: u64,
) -> u64 {
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
    node_id.hash(&mut hasher);

    hasher.finish()
}
