use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pydocstyle::rules::regexes::COMMENT_REGEX;
use crate::violations;

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
                    violations::NoBlankLineBeforeClass(blank_lines_before),
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
                    violations::OneBlankLineBeforeClass(blank_lines_before),
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
            .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
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
                violations::OneBlankLineAfterClass(blank_lines_after),
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
