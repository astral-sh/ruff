use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::ast::whitespace;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct NewLineAfterLastParagraph;
);
impl AlwaysAutofixableViolation for NewLineAfterLastParagraph {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring closing quotes should be on a separate line")
    }

    fn autofix_title(&self) -> String {
        "Move closing quotes to new line".to_string()
    }
}

/// D209
pub fn newline_after_last_paragraph(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let mut line_count = 0;
    for line in LinesWithTrailingNewline::from(body) {
        if !line.trim().is_empty() {
            line_count += 1;
        }
        if line_count > 1 {
            if let Some(last_line) = contents.lines().last().map(str::trim) {
                if last_line != "\"\"\"" && last_line != "'''" {
                    let mut diagnostic = Diagnostic::new(
                        NewLineAfterLastParagraph,
                        Range::from_located(docstring.expr),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        // Insert a newline just before the end-quote(s).
                        let num_trailing_quotes = "'''".len();
                        let num_trailing_spaces = last_line
                            .chars()
                            .rev()
                            .skip(num_trailing_quotes)
                            .take_while(|c| c.is_whitespace())
                            .count();
                        let content = format!(
                            "{}{}",
                            checker.stylist.line_ending().as_str(),
                            whitespace::clean(docstring.indentation)
                        );
                        diagnostic.amend(Fix::replacement(
                            content,
                            Location::new(
                                docstring.expr.end_location.unwrap().row(),
                                docstring.expr.end_location.unwrap().column()
                                    - num_trailing_spaces
                                    - num_trailing_quotes,
                            ),
                            Location::new(
                                docstring.expr.end_location.unwrap().row(),
                                docstring.expr.end_location.unwrap().column() - num_trailing_quotes,
                            ),
                        ));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
            return;
        }
    }
}
