use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprContext, ExprKind, Operator};

use crate::ast::helpers::{create_expr, has_comments, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UnpackInsteadOfConcatenatingToCollectionLiteral {
        pub expr: String,
    }
);
impl Violation for UnpackInsteadOfConcatenatingToCollectionLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnpackInsteadOfConcatenatingToCollectionLiteral { expr } = self;
        format!("Consider `{expr}` instead of concatenation")
    }
}

fn make_splat_elts(
    splat_element: &Expr,
    other_elements: &[Expr],
    splat_at_left: bool,
) -> Vec<Expr> {
    let mut new_elts = other_elements.to_owned();
    let splat = create_expr(ExprKind::Starred {
        value: Box::from(splat_element.clone()),
        ctx: ExprContext::Load,
    });
    if splat_at_left {
        new_elts.insert(0, splat);
    } else {
        new_elts.push(splat);
    }
    new_elts
}

#[derive(Debug)]
enum Kind {
    List,
    Tuple,
}

/// RUF005
/// This suggestion could be unsafe if the non-literal expression in the
/// expression has overridden the `__add__` (or `__radd__`) magic methods.
pub fn unpack_instead_of_concatenating_to_collection_literal(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BinOp { op, left, right } = &expr.node else {
        return;
    };
    if !matches!(op, Operator::Add) {
        return;
    }

    // Figure out which way the splat is, and what the kind of the collection is.
    let (kind, splat_element, other_elements, splat_at_left, ctx) = match (&left.node, &right.node)
    {
        (ExprKind::List { elts: l_elts, ctx }, _) => (Kind::List, right, l_elts, false, ctx),
        (ExprKind::Tuple { elts: l_elts, ctx }, _) => (Kind::Tuple, right, l_elts, false, ctx),
        (_, ExprKind::List { elts: r_elts, ctx }) => (Kind::List, left, r_elts, true, ctx),
        (_, ExprKind::Tuple { elts: r_elts, ctx }) => (Kind::Tuple, left, r_elts, true, ctx),
        _ => return,
    };

    // We'll be a bit conservative here; only calls, names and attribute accesses
    // will be considered as splat elements.
    if !matches!(
        splat_element.node,
        ExprKind::Call { .. } | ExprKind::Name { .. } | ExprKind::Attribute { .. }
    ) {
        return;
    }

    let new_expr = match kind {
        Kind::List => create_expr(ExprKind::List {
            elts: make_splat_elts(splat_element, other_elements, splat_at_left),
            ctx: ctx.clone(),
        }),
        Kind::Tuple => create_expr(ExprKind::Tuple {
            elts: make_splat_elts(splat_element, other_elements, splat_at_left),
            ctx: ctx.clone(),
        }),
    };

    let mut new_expr_string = unparse_expr(&new_expr, checker.stylist);

    new_expr_string = match kind {
        // Wrap the new expression in parentheses if it was a tuple
        Kind::Tuple => format!("({new_expr_string})"),
        Kind::List => new_expr_string,
    };

    let mut diagnostic = Diagnostic::new(
        UnpackInsteadOfConcatenatingToCollectionLiteral {
            expr: new_expr_string.clone(),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if !has_comments(expr, checker.locator) {
            diagnostic.amend(Fix::replacement(
                new_expr_string,
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.diagnostics.push(diagnostic);
}
