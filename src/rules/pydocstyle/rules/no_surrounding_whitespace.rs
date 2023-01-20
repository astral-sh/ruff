use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::leading_quote;
use crate::violations;

/// D210
pub fn no_surrounding_whitespace(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let mut lines = LinesWithTrailingNewline::from(body);
    let Some(line) = lines.next() else {
        return;
    };
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if line == trimmed {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::NoSurroundingWhitespace,
        Range::from_located(docstring.expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if let Some(pattern) = leading_quote(contents) {
            // If removing whitespace would lead to an invalid string of quote
            // characters, avoid applying the fix.
            if !trimmed.ends_with(pattern.chars().last().unwrap())
                && !trimmed.starts_with(pattern.chars().last().unwrap())
            {
                diagnostic.amend(Fix::replacement(
                    trimmed.to_string(),
                    Location::new(
                        docstring.expr.location.row(),
                        docstring.expr.location.column() + pattern.len(),
                    ),
                    Location::new(
                        docstring.expr.location.row(),
                        docstring.expr.location.column() + pattern.len() + line.chars().count(),
                    ),
                ));
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}
