use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::constants;
use crate::docstrings::definition::Docstring;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// D212, D213
pub fn multi_line_summary_start(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if LinesWithTrailingNewline::from(body).nth(1).is_none() {
        return;
    };
    let Some(first_line) = contents
        .lines()
        .next()
         else
    {
        return;
    };
    if constants::TRIPLE_QUOTE_PREFIXES.contains(&first_line) {
        if checker
            .settings
            .rules
            .enabled(&Rule::MultiLineSummaryFirstLine)
        {
            checker.diagnostics.push(Diagnostic::new(
                violations::MultiLineSummaryFirstLine,
                Range::from_located(docstring.expr),
            ));
        }
    } else {
        if checker
            .settings
            .rules
            .enabled(&Rule::MultiLineSummarySecondLine)
        {
            checker.diagnostics.push(Diagnostic::new(
                violations::MultiLineSummarySecondLine,
                Range::from_located(docstring.expr),
            ));
        }
    }
}
