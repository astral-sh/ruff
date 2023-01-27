use std::path::Path;

use ruff_macros::derive_message_formats;
use rustpython_ast::{Location, Stmt};
use textwrap::{dedent, indent};

use super::super::track::Block;
use super::super::{comments, format_imports};
use crate::ast::helpers::{
    count_trailing_lines, followed_by_multi_statement_line, preceded_by_multi_statement_line,
};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnsortedImports;
);
impl AlwaysAutofixableViolation for UnsortedImports {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Import block is un-sorted or un-formatted")
    }

    fn autofix_title(&self) -> String {
        "Organize imports".to_string()
    }
}

fn extract_range(body: &[&Stmt]) -> Range {
    let location = body.first().unwrap().location;
    let end_location = body.last().unwrap().end_location.unwrap();
    Range::new(location, end_location)
}

fn extract_indentation_range(body: &[&Stmt]) -> Range {
    let location = body.first().unwrap().location;
    Range::new(Location::new(location.row(), 0), location)
}

/// I001
pub fn organize_imports(
    block: &Block,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    settings: &Settings,
    autofix: flags::Autofix,
    package: Option<&Path>,
) -> Option<Diagnostic> {
    let indentation = locator.slice_source_code_range(&extract_indentation_range(&block.imports));
    let indentation = leading_space(indentation);

    let range = extract_range(&block.imports);

    // Special-cases: there's leading or trailing content in the import block. These
    // are too hard to get right, and relatively rare, so flag but don't fix.
    if preceded_by_multi_statement_line(block.imports.first().unwrap(), locator, indexer)
        || followed_by_multi_statement_line(block.imports.last().unwrap(), locator)
    {
        return Some(Diagnostic::new(UnsortedImports, range));
    }

    // Extract comments. Take care to grab any inline comments from the last line.
    let comments = comments::collect_comments(
        &Range::new(
            range.location,
            Location::new(range.end_location.row() + 1, 0),
        ),
        locator,
    );

    let num_trailing_lines = if block.trailer.is_none() {
        0
    } else {
        count_trailing_lines(block.imports.last().unwrap(), locator)
    };

    // Generate the sorted import block.
    let expected = format_imports(
        block,
        comments,
        locator,
        settings.line_length - indentation.len(),
        stylist,
        &settings.src,
        package,
        settings.isort.combine_as_imports,
        &settings.isort.extra_standard_library,
        settings.isort.force_single_line,
        settings.isort.force_sort_within_sections,
        settings.isort.force_wrap_aliases,
        &settings.isort.known_first_party,
        &settings.isort.known_third_party,
        settings.isort.order_by_type,
        settings.isort.relative_imports_order,
        &settings.isort.single_line_exclusions,
        settings.isort.split_on_trailing_comma,
        &settings.isort.classes,
        &settings.isort.constants,
        &settings.isort.variables,
        &settings.isort.no_lines_before,
    );

    // Expand the span the entire range, including leading and trailing space.
    let range = Range::new(
        Location::new(range.location.row(), 0),
        Location::new(range.end_location.row() + 1 + num_trailing_lines, 0),
    );
    let actual = dedent(locator.slice_source_code_range(&range));
    if actual == dedent(&expected) {
        None
    } else {
        let mut diagnostic = Diagnostic::new(UnsortedImports, range);
        if matches!(autofix, flags::Autofix::Enabled)
            && settings.rules.should_fix(diagnostic.kind.rule())
        {
            diagnostic.amend(Fix::replacement(
                indent(&expected, indentation),
                range.location,
                range.end_location,
            ));
        }
        Some(diagnostic)
    }
}
