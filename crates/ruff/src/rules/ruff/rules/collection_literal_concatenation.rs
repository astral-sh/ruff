use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, ExprContext, Operator, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::has_comments;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct CollectionLiteralConcatenation {
    expr: String,
}

impl Violation for CollectionLiteralConcatenation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let CollectionLiteralConcatenation { expr } = self;
        format!("Consider `{expr}` instead of concatenation")
    }

    fn autofix_title(&self) -> Option<String> {
        let CollectionLiteralConcatenation { expr } = self;
        Some(format!("Replace with `{expr}`"))
    }
}

fn make_splat_elts(
    splat_element: &Expr,
    other_elements: &[Expr],
    splat_at_left: bool,
) -> Vec<Expr> {
    let mut new_elts = other_elements.to_owned();
    let node = ast::ExprStarred {
        value: Box::from(splat_element.clone()),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let splat = node.into();
    if splat_at_left {
        new_elts.insert(0, splat);
    } else {
        new_elts.push(splat);
    }
    new_elts
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Kind {
    List,
    Tuple,
}

/// Recursively merge all the tuples/lists of the expression.
fn build_new_expr(expr: &Expr) -> Option<Expr> {
    let Expr::BinOp(ast::ExprBinOp { left, op: Operator::Add, right, range: _ }) = expr else {
        return None;
    };

    let new_left = match left.as_ref() {
        Expr::BinOp(ast::ExprBinOp { .. }) => match build_new_expr(left) {
            Some(new_left) => new_left,
            None => *left.clone(),
        },
        _ => *left.clone(),
    };

    let new_right = match right.as_ref() {
        Expr::BinOp(ast::ExprBinOp { .. }) => match build_new_expr(right) {
            Some(new_right) => new_right,
            None => *right.clone(),
        },
        _ => *right.clone(),
    };

    // Figure out which way the splat is, and what the kind of the collection is.
    let (kind, splat_element, other_elements, splat_at_left, ctx) = match (&new_left, &new_right) {
        (
            Expr::List(ast::ExprList {
                elts: l_elts,
                ctx,
                range: _,
            }),
            _,
        ) => (Kind::List, &new_right, l_elts, false, ctx),
        (
            Expr::Tuple(ast::ExprTuple {
                elts: l_elts,
                ctx,
                range: _,
            }),
            _,
        ) => (Kind::Tuple, &new_right, l_elts, false, ctx),
        (
            _,
            Expr::List(ast::ExprList {
                elts: r_elts,
                ctx,
                range: _,
            }),
        ) => (Kind::List, &new_left, r_elts, true, ctx),
        (
            _,
            Expr::Tuple(ast::ExprTuple {
                elts: r_elts,
                ctx,
                range: _,
            }),
        ) => (Kind::Tuple, &new_left, r_elts, true, ctx),
        _ => return None,
    };

    // We'll be a bit conservative here; only calls, names and attribute accesses
    // will be considered as splat elements.
    let mut new_elts;
    if splat_element.is_call_expr()
        || splat_element.is_name_expr()
        || splat_element.is_attribute_expr()
    {
        new_elts = make_splat_elts(splat_element, other_elements, splat_at_left);
    }
    // If the splat element is itself a list/tuple, insert them in the other list/tuple.
    else if (kind == Kind::List && splat_element.is_list_expr())
        || (kind == Kind::Tuple && splat_element.is_tuple_expr())
    {
        new_elts = other_elements.clone();

        let elts = match splat_element {
            Expr::List(ast::ExprList { elts, .. }) => elts.clone(),
            Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.clone(),
            _ => return None,
        };

        new_elts.extend_from_slice(&elts);
    } else {
        return None;
    }

    let new_expr = match kind {
        Kind::List => {
            let node = ast::ExprList {
                elts: new_elts,
                ctx: *ctx,
                range: TextRange::default(),
            };
            node.into()
        }
        Kind::Tuple => {
            let node = ast::ExprTuple {
                elts: new_elts,
                ctx: *ctx,
                range: TextRange::default(),
            };
            node.into()
        }
    };

    Some(new_expr)
}

/// RUF005
/// This suggestion could be unsafe if the non-literal expression in the
/// expression has overridden the `__add__` (or `__radd__`) magic methods.
pub(crate) fn collection_literal_concatenation(checker: &mut Checker, expr: &Expr) {
    let Some(new_expr) = build_new_expr(expr) else {
        return
    };

    let Expr::BinOp(ast::ExprBinOp { left, op: Operator::Add, right, range: _ }) = expr else {
        return;
    };

    let kind = match (left.as_ref(), right.as_ref()) {
        (Expr::List(ast::ExprList { .. }), _) => Kind::List,
        (Expr::Tuple(ast::ExprTuple { .. }), _) => Kind::Tuple,
        (_, Expr::List(ast::ExprList { .. })) => Kind::List,
        (_, Expr::Tuple(ast::ExprTuple { .. })) => Kind::Tuple,
        _ => return,
    };

    let contents = match kind {
        // Wrap the new expression in parentheses if it was a tuple
        Kind::Tuple => format!("({})", checker.generator().expr(&new_expr)),
        Kind::List => checker.generator().expr(&new_expr),
    };
    let fixable = !has_comments(expr, checker.locator);

    let mut diagnostic = Diagnostic::new(
        CollectionLiteralConcatenation {
            expr: contents.clone(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if fixable {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                contents,
                expr.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}
