use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::NewlineWithTrailingNewline;
use ruff_python_ast::whitespace;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct IndentWithSpaces;

impl Violation for IndentWithSpaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring should be indented with spaces, not tabs")
    }
}

#[violation]
pub struct UnderIndentation;

impl AlwaysAutofixableViolation for UnderIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is under-indented")
    }

    fn autofix_title(&self) -> String {
        "Increase indentation".to_string()
    }
}

#[violation]
pub struct OverIndentation;

impl AlwaysAutofixableViolation for OverIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is over-indented")
    }

    fn autofix_title(&self) -> String {
        "Remove over-indentation".to_string()
    }
}

/// D206, D207, D208
pub fn indent(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    // Split the docstring into lines.
    let lines: Vec<_> = NewlineWithTrailingNewline::with_offset(&body, body.start()).collect();
    if lines.len() <= 1 {
        return;
    }

    let mut has_seen_tab = docstring.indentation.contains('\t');
    let mut is_over_indented = true;
    let mut over_indented_lines = vec![];

    for i in 0..lines.len() {
        // First lines and continuations doesn't need any indentation.
        if i == 0 || lines[i - 1].ends_with('\\') {
            continue;
        }

        let line = &lines[i];
        // Omit empty lines, except for the last line, which is non-empty by way of
        // containing the closing quotation marks.
        let is_blank = line.trim().is_empty();
        if i < lines.len() - 1 && is_blank {
            continue;
        }

        let line_indent = whitespace::leading_space(line);

        // We only report tab indentation once, so only check if we haven't seen a tab
        // yet.
        has_seen_tab = has_seen_tab || line_indent.contains('\t');

        if checker.settings.rules.enabled(Rule::UnderIndentation) {
            // We report under-indentation on every line. This isn't great, but enables
            // autofix.
            if (i == lines.len() - 1 || !is_blank)
                && line_indent.len() < docstring.indentation.len()
            {
                let mut diagnostic =
                    Diagnostic::new(UnderIndentation, TextRange::empty(line.start()));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Edit::range_replacement(
                        whitespace::clean(docstring.indentation),
                        TextRange::at(line.start(), line_indent.text_len()),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        // Like pydocstyle, we only report over-indentation if either: (1) every line
        // (except, optionally, the last line) is over-indented, or (2) the last line
        // (which contains the closing quotation marks) is
        // over-indented. We can't know if we've achieved that condition
        // until we've viewed all the lines, so for now, just track
        // the over-indentation status of every line.
        if i < lines.len() - 1 {
            if line_indent.len() > docstring.indentation.len() {
                over_indented_lines.push(TextRange::at(line.start(), line_indent.text_len()));
            } else {
                is_over_indented = false;
            }
        }
    }

    if checker.settings.rules.enabled(Rule::IndentWithSpaces) {
        if has_seen_tab {
            checker
                .diagnostics
                .push(Diagnostic::new(IndentWithSpaces, docstring.range()));
        }
    }

    if checker.settings.rules.enabled(Rule::OverIndentation) {
        // If every line (except the last) is over-indented...
        if is_over_indented {
            for over_indented in over_indented_lines {
                // We report over-indentation on every line. This isn't great, but
                // enables autofix.
                let mut diagnostic =
                    Diagnostic::new(OverIndentation, TextRange::empty(over_indented.start()));
                if checker.patch(diagnostic.kind.rule()) {
                    let new_indent = whitespace::clean(docstring.indentation);

                    let edit = if new_indent.is_empty() {
                        Edit::range_deletion(over_indented)
                    } else {
                        Edit::range_replacement(new_indent, over_indented)
                    };
                    diagnostic.set_fix(edit);
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        // If the last line is over-indented...
        if let Some(last) = lines.last() {
            let line_indent = whitespace::leading_space(last);
            if line_indent.len() > docstring.indentation.len() {
                let mut diagnostic =
                    Diagnostic::new(OverIndentation, TextRange::empty(last.start()));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Edit::range_replacement(
                        whitespace::clean(docstring.indentation),
                        TextRange::at(last.start(), line_indent.text_len()),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
