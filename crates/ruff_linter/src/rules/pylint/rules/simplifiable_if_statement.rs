use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for if-statements that can be simplified into a single assignment.
///
/// ## Why is this bad?
/// If-statements that can be simplified to `bool(test)` are redundant.
///
/// ## Example
/// ```python
/// if x == 1:
///     is_one = True
/// else:
///     is_one = False
/// ```
///
/// Use instead:
/// ```python
/// is_one = x == 1
/// ```
#[violation]
pub struct SimplifiableIfStatement;

impl AlwaysFixableViolation for SimplifiableIfStatement {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Simplifiable if-statement")
    }

    fn fix_title(&self) -> String {
        format!("Simplify if-statement")
    }
}

/// PLR1703
pub(crate) fn simplifiable_if_statement(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // early return if the body is more than a single boolean assignment
    if body.len() != 1 {
        return;
    }

    let Some((first_assign_name, first_assign_value)) = get_assignment_details(&body[0]) else {
        return;
    };

    // early return if we have more than an else branch
    if elif_else_clauses.len() != 1 {
        return;
    }
    let ast::ElifElseClause {
        test: elif_test,
        body: elif_body,
        ..
    } = &elif_else_clauses[0];
    if elif_test.is_some() {
        // we need just an "else", which has no test
        return;
    }

    let Some((second_assign_name, second_assign_value)) = get_assignment_details(&elif_body[0])
    else {
        return;
    };

    if first_assign_name != second_assign_name {
        return;
    }

    let fixed = match (first_assign_value, second_assign_value) {
        (true, false) => {
            format!("{first_assign_name} = {}", checker.generator().expr(test))
        }
        (false, true) => {
            format!(
                "{first_assign_name} = not {}",
                checker.generator().expr(test)
            )
        }
        _ => {
            return;
        }
    };

    let mut diagnostic = Diagnostic::new(SimplifiableIfStatement, stmt_if.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        fixed,
        stmt_if.range(),
    )));

    checker.diagnostics.push(diagnostic);
}

fn get_assignment_details(stmt: &Stmt) -> Option<(String, bool)> {
    // extract the name and value of the assignment
    // e.g. `blah = True` -> ("blah", true)
    if let Stmt::Assign(assign) = stmt {
        if assign.targets.len() != 1 {
            return None;
        }
        if let Expr::Name(ast::ExprName {
            id: assign_name, ..
        }) = &assign.targets[0]
        {
            if let Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) =
                &assign.value.as_ref()
            {
                return Some((assign_name.to_string(), *value));
            }
        }
    }
    None
}
