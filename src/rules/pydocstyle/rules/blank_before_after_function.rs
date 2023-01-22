use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pydocstyle::rules::regexes::{COMMENT_REGEX, INNER_FUNCTION_OR_CLASS_REGEX};
use crate::violations;

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
        .enabled(&Rule::NoBlankLineBeforeFunction)
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
        if blank_lines_before != 0 {
            let mut diagnostic = Diagnostic::new(
                violations::NoBlankLineBeforeFunction(blank_lines_before),
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the blank line before the docstring.
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
        .enabled(&Rule::NoBlankLineAfterFunction)
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

        // Avoid D202 violations for blank lines followed by inner functions or classes.
        if blank_lines_after == 1 && INNER_FUNCTION_OR_CLASS_REGEX.is_match(after) {
            return;
        }

        if blank_lines_after != 0 {
            let mut diagnostic = Diagnostic::new(
                violations::NoBlankLineAfterFunction(blank_lines_after),
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the blank line after the docstring.
                diagnostic.amend(Fix::deletion(
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
