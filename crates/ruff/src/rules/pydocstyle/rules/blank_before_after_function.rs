use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::message::Location;
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
            .slice(Range::new(parent.location, docstring.expr.location));

        let blank_lines_before = before
            .universal_newlines()
            .rev()
            .skip(1)
            .take_while(|line| line.trim().is_empty())
            .count();
        if blank_lines_before != 0 {
            let mut diagnostic = Diagnostic::new(
                NoBlankLineBeforeFunction {
                    num_lines: blank_lines_before,
                },
                Range::from(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the blank line before the docstring.
                diagnostic.set_fix(Edit::deletion(
                    Location::new(docstring.expr.location.row() - blank_lines_before, 0),
                    Location::new(docstring.expr.location.row(), 0),
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
        let after = checker.locator.slice(Range::new(
            docstring.expr.end_location.unwrap(),
            parent.end_location.unwrap(),
        ));

        // If the docstring is only followed by blank and commented lines, abort.
        let all_blank_after = after
            .universal_newlines()
            .skip(1)
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'));
        if all_blank_after {
            return;
        }

        // Count the number of blank lines after the docstring.
        let blank_lines_after = after
            .universal_newlines()
            .skip(1)
            .take_while(|line| line.trim().is_empty())
            .count();

        // Avoid violations for blank lines followed by inner functions or classes.
        if blank_lines_after == 1
            && after
                .universal_newlines()
                .skip(1 + blank_lines_after)
                .find(|line| !line.trim_start().starts_with('#'))
                .map_or(false, |line| INNER_FUNCTION_OR_CLASS_REGEX.is_match(line))
        {
            return;
        }

        if blank_lines_after != 0 {
            let mut diagnostic = Diagnostic::new(
                NoBlankLineAfterFunction {
                    num_lines: blank_lines_after,
                },
                Range::from(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the blank line after the docstring.
                diagnostic.set_fix(Edit::deletion(
                    Location::new(docstring.expr.end_location.unwrap().row() + 1, 0),
                    Location::new(
                        docstring.expr.end_location.unwrap().row() + 1 + blank_lines_after,
                        0,
                    ),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
