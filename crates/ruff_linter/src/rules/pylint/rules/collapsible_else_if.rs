use anyhow::Result;

use ast::whitespace::indentation;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ElifElseClause, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::adjust_indentation;

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
        Some("Convert to `elif`".to_string())
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
            body, test: None, ..
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
        TextRange::new(else_clause.start(), first.start()),
    );
    diagnostic.try_set_fix(|| {
        convert_to_elif(
            first,
            else_clause,
            checker.locator(),
            checker.indexer(),
            checker.stylist(),
        )
    });
    checker.diagnostics.push(diagnostic);
}

/// Generate [`Fix`] to convert an `else` block to an `elif` block.
fn convert_to_elif(
    first: &Stmt,
    else_clause: &ElifElseClause,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<Fix> {
    let inner_if_line_start = locator.line_start(first.start());
    let inner_if_line_end = locator.line_end(first.end());

    // Identify the indentation of the loop itself (e.g., the `while` or `for`).
    let Some(indentation) = indentation(locator, else_clause) else {
        return Err(anyhow::anyhow!("`else` is expected to be on its own line"));
    };

    // Dedent the content from the end of the `else` to the end of the `if`.
    let indented = adjust_indentation(
        TextRange::new(inner_if_line_start, inner_if_line_end),
        indentation,
        locator,
        indexer,
        stylist,
    )?;

    // Strip the indent from the first line of the `if` statement, and add `el` to the start.
    let Some(unindented) = indented.strip_prefix(indentation) else {
        return Err(anyhow::anyhow!("indented block to start with indentation"));
    };
    let indented = format!("{indentation}el{unindented}");

    Ok(Fix::safe_edit(Edit::replacement(
        indented,
        locator.line_start(else_clause.start()),
        inner_if_line_end,
    )))
}
