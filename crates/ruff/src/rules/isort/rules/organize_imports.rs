use std::path::Path;

use itertools::{EitherOrBoth, Itertools};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::whitespace::trailing_lines_end;
use ruff_python_ast::{PySourceType, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_trivia::{leading_indentation, textwrap::indent, PythonWhitespace};
use ruff_source_file::{Locator, UniversalNewlines};
use ruff_text_size::{Ranged, TextRange};

use crate::line_width::LineWidthBuilder;
use crate::registry::AsRule;
use crate::settings::Settings;

use super::super::block::Block;
use super::super::{comments, format_imports};

/// ## What it does
/// De-duplicates, groups, and sorts imports based on the provided `isort` settings.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for imports to make your code
/// more readable and idiomatic.
///
/// ## Example
/// ```python
/// import pandas
/// import numpy as np
/// ```
///
/// Use instead:
/// ```python
/// import numpy as np
/// import pandas
/// ```
#[violation]
pub struct UnsortedImports;

impl Violation for UnsortedImports {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Import block is un-sorted or un-formatted")
    }

    fn autofix_title(&self) -> Option<String> {
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

#[allow(clippy::cast_sign_loss)]
/// I001
pub(crate) fn organize_imports(
    block: &Block,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    settings: &Settings,
    package: Option<&Path>,
    source_type: PySourceType,
) -> Option<Diagnostic> {
    let indentation = locator.slice(extract_indentation_range(&block.imports, locator));
    let indentation = leading_indentation(indentation);

    let range = extract_range(&block.imports);

    // Special-cases: there's leading or trailing content in the import block. These
    // are too hard to get right, and relatively rare, so flag but don't fix.
    if indexer.preceded_by_multi_statement_line(block.imports.first().unwrap(), locator)
        || indexer.followed_by_multi_statement_line(block.imports.last().unwrap(), locator)
    {
        return Some(Diagnostic::new(UnsortedImports, range));
    }

    // Extract comments. Take care to grab any inline comments from the last line.
    let comments = comments::collect_comments(
        TextRange::new(range.start(), locator.full_line_end(range.end())),
        locator,
        indexer,
    );

    let trailing_line_end = if block.trailer.is_none() {
        locator.full_line_end(range.end())
    } else {
        trailing_lines_end(block.imports.last().unwrap(), locator)
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
        settings.isort.combine_as_imports,
        settings.isort.force_single_line,
        settings.isort.force_sort_within_sections,
        settings.isort.case_sensitive,
        settings.isort.force_wrap_aliases,
        &settings.isort.force_to_top,
        &settings.isort.known_modules,
        settings.isort.order_by_type,
        settings.isort.detect_same_package,
        settings.isort.relative_imports_order,
        &settings.isort.single_line_exclusions,
        settings.isort.split_on_trailing_comma,
        &settings.isort.classes,
        &settings.isort.constants,
        &settings.isort.variables,
        &settings.isort.no_lines_before,
        settings.isort.lines_after_imports,
        settings.isort.lines_between_types,
        &settings.isort.forced_separate,
        settings.target_version,
        &settings.isort.section_order,
    );

    // Expand the span the entire range, including leading and trailing space.
    let range = TextRange::new(locator.line_start(range.start()), trailing_line_end);
    let actual = locator.slice(range);
    if matches_ignoring_indentation(actual, &expected) {
        return None;
    }

    let mut diagnostic = Diagnostic::new(UnsortedImports, range);
    if settings.rules.should_fix(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            indent(&expected, indentation).to_string(),
            range,
        )));
    }
    Some(diagnostic)
}
