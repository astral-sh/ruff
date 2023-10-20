use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_python_semantic::SemanticModel;

/// Traverse a "union" type annotation, applying `func` to each union member.
/// Supports traversal of `Union` and `|` union expressions.
/// The function is called with each expression in the union (excluding declarations of nested unions)
/// and the parent expression (if any).
pub(super) fn traverse_union<'a, F>(
    func: &mut F,
    semantic: &SemanticModel,
    expr: &'a Expr,
    parent: Option<&'a Expr>,
) where
    F: FnMut(&'a Expr, Option<&'a Expr>),
{
    // Ex) x | y
    if let Expr::BinOp(ast::ExprBinOp {
        op: Operator::BitOr,
        left,
        right,
        range: _,
    }) = expr
    {
        // The union data structure usually looks like this:
        //  a | b | c -> (a | b) | c
        //
        // However, parenthesized expressions can coerce it into any structure:
        //  a | (b | c)
        //
        // So we have to traverse both branches in order (left, then right), to report members
        // in the order they appear in the source code.

        // Traverse the left then right arms
        traverse_union(func, semantic, left, Some(expr));
        traverse_union(func, semantic, right, Some(expr));
        return;
    }

    // Ex) Union[x, y]
    if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
        if semantic.match_typing_expr(value, "Union") {
            if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                // Traverse each element of the tuple within the union recursively to handle cases
                // such as `Union[..., Union[...]]
                elts.iter()
                    .for_each(|elt| traverse_union(func, semantic, elt, Some(expr)));
                return;
            }
        }
    }

    // Otherwise, call the function on expression
    func(expr, parent);
}
