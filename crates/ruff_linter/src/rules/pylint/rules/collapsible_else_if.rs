use ast::whitespace::indentation;
use ruff_python_ast::{self as ast, ElifElseClause, Stmt};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::pyupgrade::fixes::adjust_indentation;

/// ## What it does
/// Checks for `else` blocks that consist of a single `if` statement.
///
/// ## Why is this bad?
/// If an `else` block contains a single `if` statement, it can be collapsed
/// into an `elif`, thus reducing the indentation level.
///
/// ## Example
/// ```python
/// def check_sign(value: int) -> None:
///     if value > 0:
///         print("Number is positive.")
///     else:
///         if value < 0:
///             print("Number is negative.")
///         else:
///             print("Number is zero.")
/// ```
///
/// Use instead:
/// ```python
/// def check_sign(value: int) -> None:
///     if value > 0:
///         print("Number is positive.")
///     elif value < 0:
///         print("Number is negative.")
///     else:
///         print("Number is zero.")
/// ```
///
/// ## References
/// - [Python documentation: `if` Statements](https://docs.python.org/3/tutorial/controlflow.html#if-statements)
#[violation]
pub struct CollapsibleElseIf;

impl Violation for CollapsibleElseIf {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `elif` instead of `else` then `if`, to reduce indentation")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `elif` instead of `else` then `if`, to reduce indentation".to_string())
    }
}

/// PLR5501
pub(crate) fn collapsible_else_if(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::If(ast::StmtIf {
        elif_else_clauses, ..
    }) = stmt
    else {
        return;
    };

    let Some(
        else_clause @ ElifElseClause {
            body,
            test: None,
            range,
        },
    ) = elif_else_clauses.last()
    else {
        return;
    };
    let [first @ Stmt::If(ast::StmtIf { .. })] = body.as_slice() else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        CollapsibleElseIf,
        TextRange::new(range.start(), first.start()),
    );

    if checker.settings.preview.is_enabled() {
        let inner_if_line_start = checker.locator().line_start(first.start());
        let inner_if_line_end = checker.locator().line_end(first.end());
        let inner_if_full_line_end = checker.locator().full_line_end(first.end());

        let desired_indentation = indentation(checker.locator(), else_clause).unwrap_or("");

        let indented = adjust_indentation(
            TextRange::new(inner_if_line_start, inner_if_line_end),
            desired_indentation,
            checker.locator(),
            checker.stylist(),
        )
        .unwrap();

        let fixed_indented = format!("el{}", indented.strip_prefix(desired_indentation).unwrap());

        let else_colon =
            SimpleTokenizer::starts_at(else_clause.start(), checker.locator().contents())
                .find(|token| token.kind() == SimpleTokenKind::Colon)
                .unwrap();

        diagnostic.set_fix(Fix::applicable_edits(
            Edit::deletion(inner_if_line_start, inner_if_full_line_end),
            [Edit::range_replacement(
                fixed_indented,
                TextRange::new(else_clause.start(), else_colon.end()),
            )],
            Applicability::Safe,
        ));
    }

    checker.diagnostics.push(diagnostic);
}
