use crate::SemanticModel;
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

/// Returns `true` if the given expression is either an unused value or a tuple of unused values.
pub fn is_unused(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            elts.iter().all(|expr| is_unused(expr, semantic))
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            // Treat a variable as used if it has any usages, _or_ it's shadowed by another variable
            // with usages.
            //
            // If we don't respect shadowing, we'll incorrectly flag `bar` as unused in:
            // ```python
            // from random import random
            //
            // for bar in range(10):
            //     if random() > 0.5:
            //         break
            // else:
            //     bar = 1
            //
            // print(bar)
            // ```
            let scope = semantic.current_scope();
            scope
                .get_all(id)
                .map(|binding_id| semantic.binding(binding_id))
                .filter(|binding| binding.start() >= expr.start())
                .all(|binding| !binding.is_used())
        }
        _ => false,
    }
}
