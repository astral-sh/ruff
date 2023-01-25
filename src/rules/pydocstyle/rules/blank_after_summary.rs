use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::violations;

/// D205
pub fn blank_after_summary(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body;

    let mut lines_count = 1;
    let mut blanks_count = 0;
    for line in body.trim().lines().skip(1) {
        lines_count += 1;
        if line.trim().is_empty() {
            blanks_count += 1;
        } else {
            break;
        }
    }
    if lines_count > 1 && blanks_count != 1 {
        let mut diagnostic = Diagnostic::new(
            violations::BlankLineAfterSummary(blanks_count),
            Range::from_located(docstring.expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if blanks_count > 1 {
                // Find the "summary" line (defined as the first non-blank line).
                let mut summary_line = 0;
                for line in body.lines() {
                    if line.trim().is_empty() {
                        summary_line += 1;
                    } else {
                        break;
                    }
                }

                // Insert one blank line after the summary (replacing any existing lines).
                diagnostic.amend(Fix::replacement(
                    checker.stylist.line_ending().to_string(),
                    Location::new(docstring.expr.location.row() + summary_line + 1, 0),
                    Location::new(
                        docstring.expr.location.row() + summary_line + 1 + blanks_count,
                        0,
                    ),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
