use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{self, Boolop, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::hashable::HashableExpr;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for repeated `isinstance` calls on the same object.
///
/// ## Why is this bad?
/// Repeated `isinstance` calls on the same object can be merged into a
/// single call.
///
/// ## Example
/// ```python
/// def is_number(x):
///     return isinstance(x, int) or isinstance(x, float) or isinstance(x, complex)
/// ```
///
/// Use instead:
/// ```python
/// def is_number(x):
///     return isinstance(x, (int, float, complex))
/// ```
///
/// Or, for Python 3.10 and later:
///
/// ```python
/// def is_number(x):
///     return isinstance(x, int | float | complex)
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/functions.html#isinstance)
#[violation]
pub struct RepeatedIsinstanceCalls {
    obj: String,
    types: Vec<String>,
}

impl Violation for RepeatedIsinstanceCalls {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RepeatedIsinstanceCalls { obj, types } = self;
        let types = types.join(", ");
        format!("Merge these isinstance calls: `isinstance({obj}, ({types}))`")
    }
}

/// PLR1701
pub(crate) fn repeated_isinstance_calls(
    checker: &mut Checker,
    expr: &Expr,
    op: Boolop,
    values: &[Expr],
) {
    if !matches!(op, Boolop::Or) || !checker.semantic_model().is_builtin("isinstance") {
        return;
    }

    let mut obj_to_types: FxHashMap<HashableExpr, (usize, FxHashSet<HashableExpr>)> =
        FxHashMap::default();
    for value in values {
        let Expr::Call(ast::ExprCall { func, args, .. }) = value else {
            continue;
        };
        if !matches!(func.as_ref(), Expr::Name(ast::ExprName { id, .. }) if id == "isinstance") {
            continue;
        }
        let [obj, types] = &args[..] else {
            continue;
        };
        let (num_calls, matches) = obj_to_types
            .entry(obj.into())
            .or_insert_with(|| (0, FxHashSet::default()));

        *num_calls += 1;
        matches.extend(match types {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                elts.iter().map(HashableExpr::from_expr).collect()
            }
            _ => {
                vec![types.into()]
            }
        });
    }

    for (obj, (num_calls, types)) in obj_to_types {
        if num_calls > 1 && types.len() > 1 {
            checker.diagnostics.push(Diagnostic::new(
                RepeatedIsinstanceCalls {
                    obj: checker.generator().expr(obj.as_expr()),
                    types: types
                        .iter()
                        .map(HashableExpr::as_expr)
                        .map(|expr| checker.generator().expr(expr))
                        .sorted()
                        .collect(),
                },
                expr.range(),
            ));
        }
    }
}
