use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::{StrExt, UniversalNewlineIterator};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::AsRule;

#[violation]
pub struct BlankLineAfterSummary {
    num_lines: usize,
}

impl Violation for BlankLineAfterSummary {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSummary { num_lines } = self;
        if *num_lines == 0 {
            format!("1 blank line required between summary line and description")
        } else {
            format!(
                "1 blank line required between summary line and description (found {num_lines})"
            )
        }
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Insert single blank line".to_string())
    }
}

/// D205
pub(crate) fn blank_after_summary(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    let mut lines_count: usize = 1;
    let mut blanks_count = 0;
    for line in body.trim().universal_newlines().skip(1) {
        lines_count += 1;
        if line.trim().is_empty() {
            blanks_count += 1;
        } else {
            break;
        }
    }
    if lines_count > 1 && blanks_count != 1 {
        let mut diagnostic = Diagnostic::new(
            BlankLineAfterSummary {
                num_lines: blanks_count,
            },
            docstring.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if blanks_count > 1 {
                let mut lines = UniversalNewlineIterator::with_offset(&body, body.start());
                let mut summary_end = body.start();

                // Find the "summary" line (defined as the first non-blank line).
                for line in lines.by_ref() {
                    if !line.trim().is_empty() {
                        summary_end = line.full_end();
                        break;
                    }
                }

                // Find the last blank line
                let mut blank_end = summary_end;
                for line in lines {
                    if !line.trim().is_empty() {
                        blank_end = line.start();
                        break;
                    }
                }

                // Insert one blank line after the summary (replacing any existing lines).
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::replacement(
                    checker.stylist.line_ending().to_string(),
                    summary_end,
                    blank_end,
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
