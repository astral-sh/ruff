use ruff_python_semantic::SemanticModel;
use rustpython_parser::ast::{self, Expr, Operator, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of multiple literal types in a union.
///
/// ## Why is this bad?
/// Literal types accept multiple arguments and it is clearer to specify them as a single literal.
///
/// ## Example
/// ```python
/// field: Literal[1] | Literal[2]
/// ```
///
/// Use instead:
/// ```python
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
pub(crate) fn unnecessary_literal_union(checker: &mut Checker, expr: &Expr) {
    let mut literal_members = Vec::new();
    collect_literal_members(&mut literal_members, checker.semantic(), expr);

    // Raise a violation if more than one
    if literal_members.len() > 1 {
        let diagnostic = Diagnostic::new(
            UnnecessaryLiteralUnion {
                members: literal_members
                    .into_iter()
                    .map(|m| checker.locator.slice(m.range()).to_string())
                    .collect(),
            },
            expr.range(),
        );

        checker.diagnostics.push(diagnostic);
    }
}

/// Collect literal expressions from a union.
fn collect_literal_members<'a>(
    literal_members: &mut Vec<&'a Expr>,
    model: &SemanticModel,
    expr: &'a Expr,
) {
    // The union data structure usually looks like this:
    //  a | b | c -> (a | b) | c
    //
    // However, parenthesized expressions can coerce it into any structure:
    //  a | (b | c)
    //
    // So we have to traverse both branches in order (left, then right), to report members
    // in the order they appear in the source code.
    if let Expr::BinOp(ast::ExprBinOp {
        op: Operator::BitOr,
        left,
        right,
        range: _,
    }) = expr
    {
        // Traverse left subtree, then the right subtree, propagating the previous node.
        collect_literal_members(literal_members, model, left);
        collect_literal_members(literal_members, model, right);
    }

    // If it's a literal expression add it to the members
    if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
        if model.match_typing_expr(value, "Literal") {
            literal_members.push(slice);
        }
    }
}
