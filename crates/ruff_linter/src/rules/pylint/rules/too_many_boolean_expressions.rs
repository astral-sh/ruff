use ast::{Expr, StmtIf};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for too many boolean expressions in an `if` statement.
///
/// ## Why is this bad?
/// Too many boolean expressions in an `if` statement can make the code
/// harder to understand.
///
/// ## Example
/// ```python
/// if a and b and c and d and e and f and g and h:
///    ...
/// ```
///
/// ## Options
/// - `pylint.max-bools`
#[violation]
pub struct TooManyBooleanExpressions {
    expressions: usize,
    max_expressions: usize,
}

impl Violation for TooManyBooleanExpressions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBooleanExpressions {
            expressions,
            max_expressions,
        } = self;
        format!("Too many boolean expressions ({expressions} > {max_expressions})")
    }
}

fn count_bools(expr: &Expr) -> usize {
    match expr {
        Expr::BoolOp(ast::ExprBoolOp { op, values, .. }) => match op {
            ast::BoolOp::And | ast::BoolOp::Or => {
                (values.len() - 1) + values.iter().map(count_bools).sum::<usize>()
            }
        },
        Expr::Compare(ast::ExprCompare {
            left, comparators, ..
        }) => count_bools(left) + comparators.iter().map(count_bools).sum::<usize>(),
        _ => 0,
    }
}

/// PLR0916
pub(crate) fn too_many_boolean_expressions(checker: &mut Checker, stmt: &StmtIf) {
    let test_bool_count = count_bools(stmt.test.as_ref());

    if test_bool_count > checker.settings.pylint.max_bools {
        checker.diagnostics.push(Diagnostic::new(
            TooManyBooleanExpressions {
                expressions: test_bool_count,
                max_expressions: checker.settings.pylint.max_bools,
            },
            stmt.test.as_ref().range(),
        ));
    }

    for elif in &stmt.elif_else_clauses {
        if let Some(test) = elif.test.as_ref() {
            let elif_bool_count = count_bools(test);

            if elif_bool_count > checker.settings.pylint.max_bools {
                checker.diagnostics.push(Diagnostic::new(
                    TooManyBooleanExpressions {
                        expressions: elif_bool_count,
                        max_expressions: checker.settings.pylint.max_bools,
                    },
                    test.range(),
                ));
            }
        }
    }
}
