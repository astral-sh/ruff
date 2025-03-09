use ruff_python_ast::{self as ast, CmpOp, Expr, ExprContext, Number};
use ruff_text_size::{Ranged, TextRange};

use crate::TokenKind;

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
/// Returns `None` if `expr` is a `dotted_name`. Returns `Some((description, range))` if it is not,
/// where `description` is a string describing the invalid node and `range` is the node's range.
///
/// [`dotted_name`]: https://docs.python.org/3.8/reference/compound_stmts.html#grammar-token-dotted-name
pub(super) fn invalid_pre_py39_decorator_node(expr: &Expr) -> Option<(&'static str, TextRange)> {
    let description = match expr {
        Expr::Attribute(attr) => return invalid_pre_py39_decorator_node(&attr.value),

        Expr::Name(_) => None,

        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(_) => Some("an int literal"),
            Number::Float(_) => Some("a float literal"),
            Number::Complex { .. } => Some("a complex literal"),
        },

        Expr::BoolOp(_) => Some("boolean expression"),
        Expr::BinOp(_) => Some("binary-operation expression"),
        Expr::UnaryOp(_) => Some("unary-operation expression"),
        Expr::Await(_) => Some("`await` expression"),
        Expr::Lambda(_) => Some("lambda expression"),
        Expr::If(_) => Some("conditional expression"),
        Expr::Dict(_) => Some("a dict literal"),
        Expr::Set(_) => Some("a set literal"),
        Expr::List(_) => Some("a list literal"),
        Expr::Tuple(_) => Some("a tuple literal"),
        Expr::Starred(_) => Some("starred expression"),
        Expr::Slice(_) => Some("slice expression"),
        Expr::BytesLiteral(_) => Some("bytes literal"),
        Expr::StringLiteral(_) => Some("string literal"),
        Expr::EllipsisLiteral(_) => Some("ellipsis literal"),
        Expr::NoneLiteral(_) => Some("`None` literal"),
        Expr::BooleanLiteral(_) => Some("boolean literal"),
        Expr::ListComp(_) => Some("list comprehension"),
        Expr::SetComp(_) => Some("set comprehension"),
        Expr::DictComp(_) => Some("dict comprehension"),
        Expr::Generator(_) => Some("generator expression"),
        Expr::Yield(_) => Some("`yield` expression"),
        Expr::YieldFrom(_) => Some("`yield from` expression"),
        Expr::Compare(_) => Some("comparison expression"),
        Expr::Call(_) => Some("function call"),
        Expr::FString(_) => Some("f-string"),
        Expr::Named(_) => Some("assignment expression"),
        Expr::Subscript(_) => Some("subscript expression"),
        Expr::IpyEscapeCommand(_) => Some("IPython escape command"),
    };

    description.map(|description| (description, expr.range()))
}
