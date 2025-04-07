use ruff_python_ast::{self as ast, CmpOp, Expr, ExprContext, Number};
use ruff_text_size::{Ranged, TextRange};

use crate::{error::RelaxedDecoratorError, TokenKind};

/// Set the `ctx` for `Expr::Id`, `Expr::Attribute`, `Expr::Subscript`, `Expr::Starred`,
/// `Expr::Tuple` and `Expr::List`. If `expr` is either `Expr::Tuple` or `Expr::List`,
/// recursively sets the `ctx` for their elements.
pub(super) fn set_expr_ctx(expr: &mut Expr, new_ctx: ExprContext) {
    match expr {
        Expr::Name(ast::ExprName { ctx, .. })
        | Expr::Attribute(ast::ExprAttribute { ctx, .. })
        | Expr::Subscript(ast::ExprSubscript { ctx, .. }) => *ctx = new_ctx,
        Expr::Starred(ast::ExprStarred { value, ctx, .. }) => {
            *ctx = new_ctx;
            set_expr_ctx(value, new_ctx);
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
            set_expr_ctx(operand, new_ctx);
        }
        Expr::List(ast::ExprList { elts, ctx, .. })
        | Expr::Tuple(ast::ExprTuple { elts, ctx, .. }) => {
            *ctx = new_ctx;
            elts.iter_mut()
                .for_each(|element| set_expr_ctx(element, new_ctx));
        }
        _ => {}
    }
}

/// Converts a [`TokenKind`] array of size 2 to its correspondent [`CmpOp`].
pub(super) const fn token_kind_to_cmp_op(tokens: [TokenKind; 2]) -> Option<CmpOp> {
    Some(match tokens {
        [TokenKind::Is, TokenKind::Not] => CmpOp::IsNot,
        [TokenKind::Is, _] => CmpOp::Is,
        [TokenKind::Not, TokenKind::In] => CmpOp::NotIn,
        [TokenKind::In, _] => CmpOp::In,
        [TokenKind::EqEqual, _] => CmpOp::Eq,
        [TokenKind::NotEqual, _] => CmpOp::NotEq,
        [TokenKind::Less, _] => CmpOp::Lt,
        [TokenKind::LessEqual, _] => CmpOp::LtE,
        [TokenKind::Greater, _] => CmpOp::Gt,
        [TokenKind::GreaterEqual, _] => CmpOp::GtE,
        _ => return None,
    })
}

/// Helper for `parse_decorators` to determine if `expr` is a [`dotted_name`] from the decorator
/// grammar before Python 3.9.
///
/// Returns `Some((error, range))` if `expr` is not a `dotted_name`, or `None` if it is a `dotted_name`.
///
/// [`dotted_name`]: https://docs.python.org/3.8/reference/compound_stmts.html#grammar-token-dotted-name
pub(super) fn detect_invalid_pre_py39_decorator_node(
    expr: &Expr,
) -> Option<(RelaxedDecoratorError, TextRange)> {
    let description = match expr {
        Expr::Name(_) => return None,

        Expr::Attribute(attribute) => {
            return detect_invalid_pre_py39_decorator_node(&attribute.value)
        }

        Expr::Call(_) => return Some((RelaxedDecoratorError::CallExpression, expr.range())),

        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(_) => "an int literal",
            Number::Float(_) => "a float literal",
            Number::Complex { .. } => "a complex literal",
        },

        Expr::BoolOp(_) => "boolean expression",
        Expr::BinOp(_) => "binary-operation expression",
        Expr::UnaryOp(_) => "unary-operation expression",
        Expr::Await(_) => "`await` expression",
        Expr::Lambda(_) => "lambda expression",
        Expr::If(_) => "conditional expression",
        Expr::Dict(_) => "a dict literal",
        Expr::Set(_) => "a set literal",
        Expr::List(_) => "a list literal",
        Expr::Tuple(_) => "a tuple literal",
        Expr::Starred(_) => "starred expression",
        Expr::Slice(_) => "slice expression",
        Expr::BytesLiteral(_) => "a bytes literal",
        Expr::StringLiteral(_) => "a string literal",
        Expr::EllipsisLiteral(_) => "an ellipsis literal",
        Expr::NoneLiteral(_) => "a `None` literal",
        Expr::BooleanLiteral(_) => "a boolean literal",
        Expr::ListComp(_) => "a list comprehension",
        Expr::SetComp(_) => "a set comprehension",
        Expr::DictComp(_) => "a dict comprehension",
        Expr::Generator(_) => "generator expression",
        Expr::Yield(_) => "`yield` expression",
        Expr::YieldFrom(_) => "`yield from` expression",
        Expr::Compare(_) => "comparison expression",
        Expr::FString(_) => "f-string",
        Expr::Named(_) => "assignment expression",
        Expr::Subscript(_) => "subscript expression",
        Expr::IpyEscapeCommand(_) => "IPython escape command",
    };

    Some((RelaxedDecoratorError::Other(description), expr.range()))
}
