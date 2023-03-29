use rustpython_parser::ast::{Expr, ExprContext, ExprKind, Operator};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, has_comments, unparse_expr};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct CollectionLiteralConcatenation {
    pub expr: String,
    pub fixable: bool,
}

impl Violation for CollectionLiteralConcatenation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let CollectionLiteralConcatenation { expr, .. } = self;
        format!("Consider `{expr}` instead of concatenation")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|CollectionLiteralConcatenation { expr, .. }| {
                format!("Replace with `{expr}`")
            })
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

#[derive(Debug, Copy, Clone)]
enum Kind {
    List,
    Tuple,
}

/// RUF005
/// This suggestion could be unsafe if the non-literal expression in the
/// expression has overridden the `__add__` (or `__radd__`) magic methods.
pub fn collection_literal_concatenation(checker: &mut Checker, expr: &Expr) {
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

    let contents = match kind {
        // Wrap the new expression in parentheses if it was a tuple
        Kind::Tuple => format!("({})", unparse_expr(&new_expr, checker.stylist)),
        Kind::List => unparse_expr(&new_expr, checker.stylist),
    };
    let fixable = !has_comments(expr, checker.locator);

    let mut diagnostic = Diagnostic::new(
        CollectionLiteralConcatenation {
            expr: contents.clone(),
            fixable,
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if fixable {
            diagnostic.set_fix(Edit::replacement(
                contents,
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.diagnostics.push(diagnostic);
}
