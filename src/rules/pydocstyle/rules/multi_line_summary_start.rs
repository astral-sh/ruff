use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::constants;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// D212
pub fn multi_line_summary_start(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if LinesWithTrailingNewline::from(body).nth(1).is_none() {
        return;
    };
    let mut content_lines = contents.lines();
    let Some(first_line) = content_lines
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
            let mut diagnostic = Diagnostic::new(
                violations::MultiLineSummaryFirstLine,
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let location = docstring.expr.location;
                let mut end_row = location.row() + 1;
                // Delete until first non-whitespace char.
                for line in content_lines {
                    if let Some(end_column) = line.find(|c: char| !c.is_whitespace()) {
                        let start =
                            Location::new(location.row(), location.column() + first_line.len());
                        let end = Location::new(end_row, end_column);
                        diagnostic.amend(Fix::deletion(start, end));
                        break;
                    }
                    end_row += 1;
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
