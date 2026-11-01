use itertools::{EitherOrBoth, Itertools};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::Tokens;
use ruff_python_ast::whitespace::trailing_lines_end;
use ruff_python_ast::{PySourceType, PythonVersion, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_trivia::{PythonWhitespace, leading_indentation, textwrap::indent};
use ruff_source_file::{LineRanges, UniversalNewlines};
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::line_width::LineWidthBuilder;
use crate::package::PackageRoot;
use crate::rules::isort::block::Block;
use crate::rules::isort::{comments, format_imports};
use crate::settings::LinterSettings;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// De-duplicates, groups, and sorts imports based on the provided `isort` settings.
///
/// By default, imports are categorized into the following sections in order:
/// 1. `future` (`__future__`)
/// 2. `standard-library` (e.g., `sys`, `os`, `json`)
/// 3. `third-party` (e.g., `numpy`, `pandas`, `boto3`)
/// 4. `first-party` (modules within the current project or package)
/// 5. `local-folder` (relative imports starting with `.`, e.g., `from . import utils`)
///
/// Each section is separated by a single blank line. Within each section, imports are
/// sorted alphabetically (by default, straight imports like `import sys` precede `from`
/// imports like `from os import path`).
///
/// Following PEP 8, the rule also enforces blank lines after the import block:
/// - By default, two blank lines are expected between an import block and a top-level
///   function or class definition.
/// - One blank line is expected between an import block and any other top-level code
///   (such as variable assignments).
/// - In typing stub files (`.pyi`), at most one blank line is expected.
///
/// ## Why is this bad?
/// Consistency is good. Grouping and sorting imports according to standard conventions
/// makes module dependencies easier to scan at a glance, reduces merge conflicts, and
/// makes codebases more readable.
///
/// ## Example
/// ```python
/// import boto3
/// import json
///
/// def lambda_handler(event, context):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import json
///
/// import boto3
///
///
/// def lambda_handler(event, context):
///     pass
/// ```
///
/// ## Options
/// - `lint.isort.lines-after-imports`
/// - `lint.isort.section-order`
/// - `lint.isort.default-section`
/// - `lint.isort.no-sections`
/// - `lint.isort.known-first-party`
/// - `lint.isort.known-third-party`
/// - `lint.isort.extra-standard-library`
/// - `lint.isort.sections`
///
/// ## References
/// - [PEP 8: Imports](https://peps.python.org/pep-0008/#imports)
/// - [isort documentation](https://pycqa.github.io/isort/)
///
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.110", category = Category::Style)]
pub(crate) struct UnsortedImports;

impl Violation for UnsortedImports {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Import block is un-sorted or un-formatted".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Organize imports".to_string())
    }
}

fn extract_range(body: &[&Stmt]) -> TextRange {
    let location = body.first().unwrap().start();
    let end_location = body.last().unwrap().end();
    TextRange::new(location, end_location)
}

fn extract_indentation_range(body: &[&Stmt], locator: &Locator) -> TextRange {
    let location = body.first().unwrap().start();

    TextRange::new(locator.line_start(location), location)
}

/// Compares two strings, returning true if they are equal modulo whitespace
/// at the start of each line.
fn matches_ignoring_indentation(val1: &str, val2: &str) -> bool {
    val1.universal_newlines()
        .zip_longest(val2.universal_newlines())
        .all(|pair| match pair {
            EitherOrBoth::Both(line1, line2) => {
                line1.trim_whitespace_start() == line2.trim_whitespace_start()
            }
            _ => false,
        })
}

#[expect(clippy::too_many_arguments)]
/// I001
pub(crate) fn organize_imports(
    block: &Block,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    settings: &LinterSettings,
    package: Option<PackageRoot<'_>>,
    source_type: PySourceType,
    tokens: &Tokens,
    target_version: PythonVersion,
    context: &LintContext,
) {
    let indentation = locator.slice(extract_indentation_range(&block.imports, locator));
    let indentation = leading_indentation(indentation);

    let range = extract_range(&block.imports);

    // Special-cases: there's leading or trailing content in the import block. These
    // are too hard to get right, and relatively rare, so flag but don't fix.
    if indexer.preceded_by_multi_statement_line(block.imports.first().unwrap(), locator.contents())
        || indexer
            .followed_by_multi_statement_line(block.imports.last().unwrap(), locator.contents())
    {
        context.report_diagnostic(UnsortedImports, range);
        return;
    }

    // Extract comments. Take care to grab any inline comments from the last line.
    // Also extend the start backward to include any import heading comments above
    // the first import, so they're collected and can be stripped/re-added correctly.
    let import_headings = &settings.isort.import_headings;
    let (comment_start, fix_start) = if import_headings.is_empty() {
        // Preserve original behavior: comments from import start, fix range from line start.
        (range.start(), locator.line_start(range.start()))
    } else {
        // Heading comments are already formatted as "# {heading}" in settings.
        // Walk backward through comment ranges to find adjacent heading comments above the first import.
        let comment_ranges: &[TextRange] = indexer.comment_ranges();
        let import_line_start = locator.line_start(range.start());
        let partition =
            comment_ranges.partition_point(|comment| comment.start() < import_line_start);

        let mut earliest = import_line_start;
        for comment_range in comment_ranges[..partition].iter().rev() {
            // The comment's line must end right where 'earliest' starts (adjacent).
            if locator.full_line_end(comment_range.end()) != earliest {
                break;
            }

            let comment_text = locator.slice(*comment_range);
            if import_headings
                .values()
                .any(|header| comment_text == header.as_str())
            {
                earliest = locator.line_start(comment_range.start());
            } else {
                break;
            }
        }
        (earliest, earliest)
    };

    let comments = comments::collect_comments(
        TextRange::new(comment_start, locator.full_line_end(range.end())),
        locator,
        indexer.comment_ranges(),
    );

    let trailing_line_end = if block.trailer.is_none() {
        locator.full_line_end(range.end())
    } else {
        trailing_lines_end(block.imports.last().unwrap(), locator.contents())
    };

    // Generate the sorted import block.
    let expected = format_imports(
        block,
        comments,
        locator,
        settings.line_length,
        LineWidthBuilder::new(settings.tab_size).add_str(indentation),
        stylist,
        &settings.src,
        package,
        source_type,
        target_version,
        &settings.isort,
        tokens,
    );

    // Expand the span the entire range, including leading and trailing space.
    let fix_range = TextRange::new(fix_start, trailing_line_end);
    let actual = locator.slice(fix_range);
    if matches_ignoring_indentation(actual, &expected) {
        return;
    }
    let mut diagnostic = context.report_diagnostic(UnsortedImports, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        indent(&expected, indentation).to_string(),
        fix_range,
    )));
}
