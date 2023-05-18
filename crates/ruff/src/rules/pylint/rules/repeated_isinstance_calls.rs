use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{self, Boolop, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::hashable::HashableExpr;
use ruff_python_ast::helpers::unparse_expr;

use crate::checkers::ast::Checker;

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
    if !matches!(op, Boolop::Or) || !checker.ctx.is_builtin("isinstance") {
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
                    obj: unparse_expr(obj.as_expr(), checker.generator()),
                    types: types
                        .iter()
                        .map(HashableExpr::as_expr)
                        .map(|expr| unparse_expr(expr, checker.generator()))
                        .sorted()
                        .collect(),
                },
                expr.range(),
            ));
        }
    }
}
