use rustpython_ast::{Location, Stmt};
use textwrap::{dedent, indent};

use crate::ast::helpers::{count_trailing_lines, match_leading_content, match_trailing_content};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::autofix::Fix;
use crate::checks::CheckKind;
use crate::isort::track::Block;
use crate::isort::{comments, format_imports};
use crate::{Check, Settings, SourceCodeLocator};

fn extract_range(body: &[&Stmt]) -> Range {
    let location = body.first().unwrap().location;
    let end_location = body.last().unwrap().end_location.unwrap();
    Range {
        location,
        end_location,
    }
}

fn extract_indentation(body: &[&Stmt], locator: &SourceCodeLocator) -> String {
    let location = body.first().unwrap().location;
    let range = Range {
        location: Location::new(location.row(), 0),
        end_location: location,
    };
    let existing = locator.slice_source_code_range(&range);
    leading_space(&existing)
}

/// I001
pub fn check_imports(
    block: &Block,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: bool,
) -> Option<Check> {
    let range = extract_range(&block.imports);
    let indentation = extract_indentation(&block.imports, locator);

    // Extract comments. Take care to grab any inline comments from the last line.
    let comments = comments::collect_comments(
        &Range {
            location: range.location,
            end_location: Location::new(range.end_location.row() + 1, 0),
        },
        locator,
    );

    // Special-cases: there's leading or trailing content in the import block.
    let has_leading_content = match_leading_content(block.imports.first().unwrap(), locator);
    let has_trailing_content = match_trailing_content(block.imports.last().unwrap(), locator);
    let num_trailing_lines = if block.trailer.is_none() {
        0
    } else {
        count_trailing_lines(block.imports.last().unwrap(), locator)
    };

    // Generate the sorted import block.
    let expected = format_imports(
        block,
        comments,
        settings.line_length - indentation.len(),
        &settings.src,
        &settings.isort.known_first_party,
        &settings.isort.known_third_party,
        &settings.isort.extra_standard_library,
        settings.isort.combine_as_imports,
        settings.isort.force_wrap_aliases,
    );

    if has_leading_content || has_trailing_content {
        let mut check = Check::new(CheckKind::UnsortedImports, range);
        if autofix && settings.fixable.contains(check.kind.code()) {
            let mut content = String::new();
            if has_leading_content {
                content.push('\n');
            }
            content.push_str(&indent(&expected, &indentation));
            check.amend(Fix::replacement(
                content,
                // Preserve leading prefix (but put the imports on a new line).
                if has_leading_content {
                    range.location
                } else {
                    Location::new(range.location.row(), 0)
                },
                // TODO(charlie): Preserve trailing suffixes. Right now, we strip them.
                Location::new(range.end_location.row() + 1 + num_trailing_lines, 0),
            ));
        }
        Some(check)
    } else {
        // Expand the span the entire range, including leading and trailing space.
        let range = Range {
            location: Location::new(range.location.row(), 0),
            end_location: Location::new(range.end_location.row() + 1 + num_trailing_lines, 0),
        };
        let actual = dedent(&locator.slice_source_code_range(&range));
        if actual == expected {
            None
        } else {
            let mut check = Check::new(CheckKind::UnsortedImports, range);
            if autofix && settings.fixable.contains(check.kind.code()) {
                check.amend(Fix::replacement(
                    indent(&expected, &indentation),
                    range.location,
                    range.end_location,
                ));
            }
            Some(check)
        }
    }
}
