use path_absolutize::path_dedot;
use rustpython_ast::{Location, Stmt};
use textwrap::{dedent, indent};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::docstrings::helpers::leading_space;
use crate::isort::sort_imports;
use crate::{Check, SourceCodeLocator};

// STOPSHIP(charlie): If an import isn't the first or last statement on a line,
// this will remove other valid code.
fn extract_range(body: &[&Stmt]) -> Range {
    // Extract the range of the existing import block. We extend to include the
    // entire first and last line.
    let location = body.iter().map(|stmt| stmt.location).min().unwrap();
    let end_location = body
        .iter()
        .map(|stmt| stmt.end_location)
        .max()
        .unwrap()
        .unwrap();
    Range {
        location: Location::new(location.row(), 0),
        end_location: Location::new(end_location.row() + 1, 0),
    }
}

/// I001
pub fn check_imports(checker: &mut Checker, body: Vec<&Stmt>) {
    // Extract the existing import block.
    let range = extract_range(&body);
    let existing = checker.locator.slice_source_code_range(&range);

    // Infer existing indentation.
    let indentation = leading_space(&existing);

    // Dedent the existing import block.
    let actual = dedent(&existing);

    // Generate the sorted import block.
    let expected = sort_imports(
        body,
        &checker.settings.line_length,
        &checker.settings.src_paths,
        &checker.settings.isort.known_first_party,
        &checker.settings.isort.known_third_party,
        &checker.settings.isort.extra_standard_library,
    )
    .unwrap();

    // Compare the two?
    if actual != expected {
        let mut check = Check::new(CheckKind::UnsortedImports, range);
        if checker.patch() {
            check.amend(Fix::replacement(
                indent(&expected, &indentation),
                range.location,
                range.end_location,
            ));
        }
        checker.add_check(check);
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
        &vec![path_dedot::CWD.clone()],
        &Default::default(),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
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
