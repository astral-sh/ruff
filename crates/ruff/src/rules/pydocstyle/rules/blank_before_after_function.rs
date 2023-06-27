use once_cell::sync::Lazy;
use regex::Regex;
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Definition, Member, MemberKind};
use ruff_python_whitespace::{PythonWhitespace, UniversalNewlineIterator, UniversalNewlines};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct NoBlankLineBeforeFunction {
    num_lines: usize,
}

impl AlwaysAutofixableViolation for NoBlankLineBeforeFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineBeforeFunction { num_lines } = self;
        format!("No blank lines allowed before function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before function docstring".to_string()
    }
}

#[violation]
pub struct NoBlankLineAfterFunction {
    num_lines: usize,
}

impl AlwaysAutofixableViolation for NoBlankLineAfterFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineAfterFunction { num_lines } = self;
        format!("No blank lines allowed after function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) after function docstring".to_string()
    }
}

static INNER_FUNCTION_OR_CLASS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s+(?:(?:class|def|async def)\s|@)").unwrap());

/// D201, D202
pub(crate) fn blank_before_after_function(checker: &mut Checker, docstring: &Docstring) {
    let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = docstring.definition else {
        return;
    };

    if checker.enabled(Rule::NoBlankLineBeforeFunction) {
        let before = checker
            .locator
            .slice(TextRange::new(stmt.start(), docstring.start()));

        let mut lines = UniversalNewlineIterator::with_offset(before, stmt.start()).rev();
        let mut blank_lines_before = 0usize;
        let mut blank_lines_start = lines.next().map(|l| l.end()).unwrap_or_default();

        for line in lines {
            if line.trim().is_empty() {
                blank_lines_before += 1;
                blank_lines_start = line.start();
            } else {
                break;
            }
        }

        if blank_lines_before != 0 {
            let mut diagnostic = Diagnostic::new(
                NoBlankLineBeforeFunction {
                    num_lines: blank_lines_before,
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the blank line before the docstring.
                diagnostic.set_fix(Fix::automatic(Edit::deletion(
                    blank_lines_start,
                    docstring.start() - docstring.indentation.text_len(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.enabled(Rule::NoBlankLineAfterFunction) {
        let after = checker
            .locator
            .slice(TextRange::new(docstring.end(), stmt.end()));

        // If the docstring is only followed by blank and commented lines, abort.
        let all_blank_after = after.universal_newlines().skip(1).all(|line| {
            line.trim_whitespace().is_empty() || line.trim_whitespace_start().starts_with('#')
        });
        if all_blank_after {
            return;
        }

        // Count the number of blank lines after the docstring.
        let mut blank_lines_after = 0usize;
        let mut lines = UniversalNewlineIterator::with_offset(after, docstring.end()).peekable();
        let first_line_end = lines.next().map(|l| l.end()).unwrap_or_default();
        let mut blank_lines_end = first_line_end;

        while let Some(line) = lines.peek() {
            if line.trim().is_empty() {
                blank_lines_after += 1;
                blank_lines_end = line.end();
                lines.next();
            } else {
                break;
            }
        }

        // Avoid violations for blank lines followed by inner functions or classes.
        if blank_lines_after == 1
            && lines
                .find(|line| !line.trim_whitespace_start().starts_with('#'))
                .map_or(false, |line| INNER_FUNCTION_OR_CLASS_REGEX.is_match(&line))
        {
            return;
        }

        if blank_lines_after != 0 {
            let mut diagnostic = Diagnostic::new(
                NoBlankLineAfterFunction {
                    num_lines: blank_lines_after,
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the blank line after the docstring.
                diagnostic.set_fix(Fix::automatic(Edit::deletion(
                    first_line_end,
                    blank_lines_end,
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
