use ruff_python_semantic::SemanticModel;
use rustpython_parser::ast::{self, Expr, Operator, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use smallvec::SmallVec;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of multiple literal types in a union.
///
/// ## Why is this bad?
/// Literal types accept multiple arguments and it is clearer to specify them
/// as a single literal.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// field: Literal[1] | Literal[2]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// field: Literal[1, 2]
/// ```
#[violation]
pub struct UnnecessaryLiteralUnion {
    members: Vec<String>,
}

impl Violation for UnnecessaryLiteralUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Multiple literal members in a union. Use a single literal, e.g. `Literal[{}]`",
            self.members.join(", ")
        )
    }
}

/// PYI030
pub(crate) fn unnecessary_literal_union<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut literal_exprs = SmallVec::<[&Box<Expr>; 1]>::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut collect_literal_expr = |expr: &'a Expr| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                literal_exprs.push(slice);
            }
        }
    };

    // Traverse the union, collect all literal members
    traverse_union(&mut collect_literal_expr, expr, checker.semantic());

    // Raise a violation if more than one
    if literal_exprs.len() > 1 {
        let diagnostic = Diagnostic::new(
            UnnecessaryLiteralUnion {
                members: literal_exprs
                    .into_iter()
                    .map(|literal_expr| checker.locator.slice(literal_expr.range()).to_string())
                    .collect(),
            },
            expr.range(),
        );

        checker.diagnostics.push(diagnostic);
    }
}

/// Traverse a "union" type annotation, calling `func` on each expression in the union.
fn traverse_union<'a, F>(func: &mut F, expr: &'a Expr, semantic: &SemanticModel)
where
    F: FnMut(&'a Expr),
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
        traverse_union(func, left, semantic);
        traverse_union(func, right, semantic);
        return;
    }

    // Ex) Union[x, y]
    if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
        if semantic.match_typing_expr(value, "Union") {
            if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                // Traverse each element of the tuple within the union recursively to handle cases
                // such as `Union[..., Union[...]]
                elts.iter()
                    .for_each(|elt| traverse_union(func, elt, semantic));
                return;
            }
        }
    }

    // Otherwise, call the function on expression
    func(expr);
}
