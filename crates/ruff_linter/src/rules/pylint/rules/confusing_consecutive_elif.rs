use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ElifElseClause, Stmt, StmtIf};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for an `elif` statement which follows right after an indented block which itself ends with if or elif."
///
/// ## Why is this bad?
/// It may not be ovious if the elif statement was willingly or mistakenly unindented.
/// Adding an explicit else, or extracting the indented if statement into a separate
/// function might avoid confusion and prevent errors.
///
/// ## Example
/// ```python
/// if old_conf:
///     if not new_conf:
///         machine.disable()
///     elif old_conf.value != new_conf.value:
///         machine.disable()
///         machine.enable(new_conf.value)
/// elif new_conf:  # [confusing-consecutive-elif]
///     machine.enable(new_conf.value)
/// ```
///
/// Use instead:
/// ```python
/// # Option 1: add explicit else
/// if old_conf:
///     if not new_conf:
///         machine.disable()
///     elif old_conf.value != new_conf.value:
///         machine.disable()
///         machine.enable(new_conf.value)
///     else:
///         pass
/// elif new_conf:  # [confusing-consecutive-elif]
///     machine.enable(new_conf.value)
///
///
/// # Option 2: extract function
/// def extracted(old_conf, new_conf, machine):
///     if not new_conf:
///         machine.disable()
///     elif old_conf.value != new_conf.value:
///         machine.disable()
///         machine.enable(new_conf.value)
///
///
/// if old_conf:
///     extracted(old_conf, new_conf, machine)
/// elif new_conf:
///     machine.enable(new_conf.value)
/// ```
///
/// ## References
/// - [Python documentation: `if` Statements](https://docs.python.org/3/tutorial/controlflow.html#if-statements)
#[violation]
pub struct ConfusingConsecutiveElif;

impl Violation for ConfusingConsecutiveElif {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consecutive elif with differing indentation level, consider creating a function to separate the inner elif")
    }
}

/// PLR5601
pub(crate) fn confusing_consecutive_elif(checker: &mut Checker, stmt_if: &StmtIf) {
    let ast::StmtIf {
        body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // The last clause must be an elif
    let Some(ElifElseClause { test: Some(_), .. }) = elif_else_clauses.last() else {
        return;
    };

    // Take the second last elif, or if that does not exist, take the if
    let orelse = match elif_else_clauses.len() {
        0 => return,
        1 => {
            let Some(Stmt::If(ast::StmtIf {
                elif_else_clauses: orelse,
                ..
            })) = body.last()
            else {
                return;
            };
            orelse
        }
        _ => {
            let [.., ElifElseClause {
                body: body_stmt,
                test: Some(_),
                ..
            }, _] = elif_else_clauses.as_slice()
            else {
                return;
            };
            let Some(Stmt::If(ast::StmtIf {
                elif_else_clauses: orelse,
                ..
            })) = body_stmt.last()
            else {
                return;
            };
            orelse
        }
    };
    if !has_no_else_clause(orelse) {
        return;
    }

    let diagnostic = Diagnostic::new(
        ConfusingConsecutiveElif,
        TextRange::new(elif_else_clauses.last().unwrap().start(), stmt_if.end()),
    );

    checker.diagnostics.push(diagnostic);
}

fn has_no_else_clause(orelse: &[ElifElseClause]) -> bool {
    if orelse.is_empty() {
        return true;
    }
    let Some(ElifElseClause {
        body: body_stmt,
        test: Some(_),
        ..
    }) = orelse.last()
    else {
        return false;
    };
    let Some(Stmt::If(ast::StmtIf {
        elif_else_clauses: orelse,
        ..
    })) = body_stmt.last()
    else {
        return true;
    };
    has_no_else_clause(orelse)
}
