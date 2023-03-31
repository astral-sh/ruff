use once_cell::sync::Lazy;
use regex::Regex;
use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::{StrExt, UniversalNewlineIterator};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::{AsRule, Rule};

#[violation]
pub struct NoBlankLineBeforeFunction {
    pub num_lines: usize,
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
    pub num_lines: usize,
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
pub fn blank_before_after_function(checker: &mut Checker, docstring: &Docstring) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = &docstring.kind else {
        return;
    };

    if checker
        .settings
        .rules
        .enabled(Rule::NoBlankLineBeforeFunction)
    {
        let before = checker
            .locator
            .slice(TextRange::new(parent.start(), docstring.start()));

        let mut lines = UniversalNewlineIterator::with_offset(before, parent.start()).rev();
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
                diagnostic.set_fix(Edit::deletion(
                    blank_lines_start,
                    docstring.start() - docstring.indentation.text_len(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    if checker
        .settings
        .rules
        .enabled(Rule::NoBlankLineAfterFunction)
    {
        let after = checker
            .locator
            .slice(TextRange::new(docstring.end(), parent.end()));

        // If the docstring is only followed by blank and commented lines, abort.
        let all_blank_after = after
            .universal_newlines()
            .skip(1)
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'));
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
                .find(|line| !line.trim_start().starts_with('#'))
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
                diagnostic.set_fix(Edit::deletion(first_line_end, blank_lines_end));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
