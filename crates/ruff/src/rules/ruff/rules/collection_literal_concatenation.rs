use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, ExprContext, Operator, Ranged};
use thin_vec::ThinVec;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_comments, unparse_expr};

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
) -> ThinVec<Expr> {
    let mut new_elts = ThinVec::from(other_elements);
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

#[derive(Debug, Copy, Clone)]
enum Kind {
    List,
    Tuple,
}

/// RUF005
/// This suggestion could be unsafe if the non-literal expression in the
/// expression has overridden the `__add__` (or `__radd__`) magic methods.
pub(crate) fn collection_literal_concatenation(checker: &mut Checker, expr: &Expr) {
    let Expr::BinOp(ast::ExprBinOp { left, op: Operator::Add, right, range: _ }) = expr else {
        return;
    };

    // Figure out which way the splat is, and what the kind of the collection is.
    let (kind, splat_element, other_elements, splat_at_left, ctx) =
        match (left.as_ref(), right.as_ref()) {
            (
                Expr::List(ast::ExprList {
                    elts: l_elts,
                    ctx,
                    range: _,
                }),
                _,
            ) => (Kind::List, right, l_elts, false, ctx),
            (
                Expr::Tuple(ast::ExprTuple {
                    elts: l_elts,
                    ctx,
                    range: _,
                }),
                _,
            ) => (Kind::Tuple, right, l_elts, false, ctx),
            (
                _,
                Expr::List(ast::ExprList {
                    elts: r_elts,
                    ctx,
                    range: _,
                }),
            ) => (Kind::List, left, r_elts, true, ctx),
            (
                _,
                Expr::Tuple(ast::ExprTuple {
                    elts: r_elts,
                    ctx,
                    range: _,
                }),
            ) => (Kind::Tuple, left, r_elts, true, ctx),
            _ => return,
        };

    // We'll be a bit conservative here; only calls, names and attribute accesses
    // will be considered as splat elements.
    if !(splat_element.is_call_expr()
        || splat_element.is_name_expr()
        || splat_element.is_attribute_expr())
    {
        return;
    }

    let new_expr = match kind {
        Kind::List => {
            let node = ast::ExprList {
                elts: make_splat_elts(splat_element, other_elements, splat_at_left),
                ctx: *ctx,
                range: TextRange::default(),
            };
            node.into()
        }
        Kind::Tuple => {
            let node = ast::ExprTuple {
                elts: make_splat_elts(splat_element, other_elements, splat_at_left),
                ctx: *ctx,
                range: TextRange::default(),
            };
            node.into()
        }
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
