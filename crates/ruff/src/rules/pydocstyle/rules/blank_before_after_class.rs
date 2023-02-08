use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct OneBlankLineBeforeClass {
        pub lines: usize,
    }
);
impl AlwaysAutofixableViolation for OneBlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("1 blank line required before class docstring")
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line before class docstring".to_string()
    }
}

define_violation!(
    pub struct OneBlankLineAfterClass {
        pub lines: usize,
    }
);
impl AlwaysAutofixableViolation for OneBlankLineAfterClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("1 blank line required after class docstring")
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line after class docstring".to_string()
    }
}

define_violation!(
    pub struct NoBlankLineBeforeClass {
        pub lines: usize,
    }
);
impl AlwaysAutofixableViolation for NoBlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No blank lines allowed before class docstring")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before class docstring".to_string()
    }
}

/// D203, D204, D211
pub fn blank_before_after_class(checker: &mut Checker, docstring: &Docstring) {
    let (DefinitionKind::Class(parent) | DefinitionKind::NestedClass(parent)) = &docstring.kind else {
        return;
    };

    if checker
        .settings
        .rules
        .enabled(&Rule::OneBlankLineBeforeClass)
        || checker
            .settings
            .rules
            .enabled(&Rule::NoBlankLineBeforeClass)
    {
        let (before, ..) = checker.locator.partition_source_code_at(
            &Range::from_located(parent),
            &Range::from_located(docstring.expr),
        );

        let blank_lines_before = before
            .lines()
            .rev()
            .skip(1)
            .take_while(|line| line.trim().is_empty())
            .count();
        if checker
            .settings
            .rules
            .enabled(&Rule::NoBlankLineBeforeClass)
        {
            if blank_lines_before != 0 {
                let mut diagnostic = Diagnostic::new(
                    NoBlankLineBeforeClass {
                        lines: blank_lines_before,
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Delete the blank line before the class.
                    diagnostic.amend(Fix::deletion(
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
            .enabled(&Rule::OneBlankLineBeforeClass)
        {
            if blank_lines_before != 1 {
                let mut diagnostic = Diagnostic::new(
                    OneBlankLineBeforeClass {
                        lines: blank_lines_before,
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Insert one blank line before the class.
                    diagnostic.amend(Fix::replacement(
                        checker.stylist.line_ending().to_string(),
                        Location::new(docstring.expr.location.row() - blank_lines_before, 0),
                        Location::new(docstring.expr.location.row(), 0),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker
        .settings
        .rules
        .enabled(&Rule::OneBlankLineAfterClass)
    {
        let (_, _, after) = checker.locator.partition_source_code_at(
            &Range::from_located(parent),
            &Range::from_located(docstring.expr),
        );

        let all_blank_after = after
            .lines()
            .skip(1)
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'));
        if all_blank_after {
            return;
        }

        let blank_lines_after = after
            .lines()
            .skip(1)
            .take_while(|line| line.trim().is_empty())
            .count();
        if blank_lines_after != 1 {
            let mut diagnostic = Diagnostic::new(
                OneBlankLineAfterClass {
                    lines: blank_lines_after,
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Insert a blank line before the class (replacing any existing lines).
                diagnostic.amend(Fix::replacement(
                    checker.stylist.line_ending().to_string(),
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
