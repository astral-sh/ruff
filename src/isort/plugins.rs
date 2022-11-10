use path_absolutize::path_dedot;
use rustpython_ast::{Location, Stmt};
use textwrap::{dedent, indent};

use crate::ast::types::Range;
use crate::autofix::{fixer, Fix};
use crate::checks::CheckKind;
use crate::docstrings::helpers::leading_space;
use crate::isort::sort_imports;
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

fn match_leading_content(body: &[&Stmt], locator: &SourceCodeLocator) -> bool {
    let location = body.first().unwrap().location;
    let range = Range {
        location: Location::new(location.row(), 0),
        end_location: location,
    };
    let prefix = locator.slice_source_code_range(&range);
    prefix.chars().any(|char| !char.is_whitespace())
}

fn match_trailing_content(body: &[&Stmt], locator: &SourceCodeLocator) -> bool {
    let end_location = body.last().unwrap().end_location.unwrap();
    let range = Range {
        location: end_location,
        end_location: Location::new(end_location.row() + 1, 0),
    };
    let suffix = locator.slice_source_code_range(&range);
    suffix.chars().any(|char| !char.is_whitespace())
}

/// I001
pub fn check_imports(
    body: Vec<&Stmt>,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: &fixer::Mode,
) -> Option<Check> {
    let range = extract_range(&body);
    let indentation = extract_indentation(&body, locator);

    // Special-cases: there's leading or trailing content in the import block.
    let has_leading_content = match_leading_content(&body, locator);
    let has_trailing_content = match_trailing_content(&body, locator);

    // Generate the sorted import block.
    let expected = sort_imports(
        body,
        &settings.line_length,
        &settings.src_paths,
        &settings.isort.known_first_party,
        &settings.isort.known_third_party,
        &settings.isort.extra_standard_library,
    );

    if has_leading_content || has_trailing_content {
        let mut check = Check::new(CheckKind::UnsortedImports, range);
        if autofix.patch() {
            let mut content = String::new();
            if has_leading_content {
                // TODO(charlie): Strip semicolon.
                content.push('\n');
            }
            content.push_str(&indent(&expected, &indentation));
            if has_trailing_content {
                // TODO(charlie): Strip semicolon.
                content.push('\n');
            }
            check.amend(Fix::replacement(
                content,
                range.location,
                range.end_location,
            ));
        }
        Some(check)
    } else {
        let actual = dedent(&locator.slice_source_code_range(&range));
        if actual != expected {
            let mut check = Check::new(CheckKind::UnsortedImports, range);
            if autofix.patch() {
                check.amend(Fix::replacement(
                    indent(&expected, &indentation),
                    range.location,
                    range.end_location,
                ));
            }
            Some(check)
        } else {
            None
        }
    }
}

// STOPSHIP(charlie): Exists for testing.
fn actual(body: &[&Stmt], locator: &SourceCodeLocator) -> String {
    let range = extract_range(body);
    let existing = locator.slice_source_code_range(&range);
    dedent(&existing)
}

// STOPSHIP(charlie): Exists for testing.
fn expected(body: Vec<&Stmt>, locator: &SourceCodeLocator) -> String {
    let range = extract_range(&body);
    let existing = locator.slice_source_code_range(&range);
    let indentation = leading_space(&existing);
    let expected = sort_imports(
        body,
        &100,
        &[path_dedot::CWD.clone()],
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );
    indent(&expected, &indentation)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_ast::Stmt;
    use rustpython_parser::parser;

    use crate::isort::plugins::{actual, expected};
    use crate::SourceCodeLocator;

    #[test]
    fn single() -> Result<()> {
        let contents = r#"import os
"#;
        let suite = parser::parse_program(contents, "<filename>")?;
        let locator = SourceCodeLocator::new(&contents);

        let body: Vec<&Stmt> = suite.iter().collect();

        let actual = actual(&body, &locator);
        let expected = expected(body, &locator);

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn double() -> Result<()> {
        let contents = r#"import sys
import os
"#;
        let suite = parser::parse_program(contents, "<filename>")?;
        let locator = SourceCodeLocator::new(&contents);
        let body: Vec<&Stmt> = suite.iter().collect();

        let actual = actual(&body, &locator);
        assert_eq!(
            actual,
            r#"import sys
import os
"#
        );

        let expected = expected(body, &locator);
        assert_eq!(
            expected,
            r#"import os
import sys
"#
        );

        Ok(())
    }

    #[test]
    fn indented() -> Result<()> {
        let contents = r#"    import sys
    import os
"#;
        let suite = parser::parse_program(contents, "<filename>")?;
        let locator = SourceCodeLocator::new(&contents);
        let body: Vec<&Stmt> = suite.iter().collect();

        let actual = actual(&body, &locator);
        assert_eq!(
            actual,
            r#"import sys
import os
"#
        );

        let expected = expected(body, &locator);
        assert_eq!(
            expected,
            r#"import os
import sys
"#
        );

        Ok(())
    }
}
