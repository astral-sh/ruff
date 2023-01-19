use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::violations;

/// D200
pub fn one_liner(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body;

    let mut line_count = 0;
    let mut non_empty_line_count = 0;
    for line in LinesWithTrailingNewline::from(body) {
        line_count += 1;
        if !line.trim().is_empty() {
            non_empty_line_count += 1;
        }
        if non_empty_line_count > 1 {
            break;
        }
    }

    if non_empty_line_count == 1 && line_count > 1 {
        checker.diagnostics.push(Diagnostic::new(
            violations::FitsOnOneLine,
            Range::from_located(docstring.expr),
        ));
    }
}
