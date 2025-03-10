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
pub(super) fn invalid_pre_py39_decorator_description_and_range(
    expr: &Expr,
) -> Option<(&'static str, TextRange)> {
    let description = match expr {
        Expr::Attribute(attr) => {
            return invalid_pre_py39_decorator_description_and_range(&attr.value)
        }

        Expr::Name(_) => return None,

        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(_) => "int literals",
            Number::Float(_) => "float literals",
            Number::Complex { .. } => "complex literals",
        },

        Expr::BoolOp(_) => "boolean expressions",
        Expr::BinOp(_) => "binary-operation expressions",
        Expr::UnaryOp(_) => "unary-operation expressions",
        Expr::Await(_) => "`await` expressions",
        Expr::Lambda(_) => "lambda expressions",
        Expr::If(_) => "conditional expressions",
        Expr::Dict(_) => "dict literals",
        Expr::Set(_) => "set literals",
        Expr::List(_) => "list literals",
        Expr::Tuple(_) => "tuple literals",
        Expr::Starred(_) => "starred expressions",
        Expr::Slice(_) => "slice expressions",
        Expr::BytesLiteral(_) => "bytes literals",
        Expr::StringLiteral(_) => "string literals",
        Expr::EllipsisLiteral(_) => "ellipsis literals",
        Expr::NoneLiteral(_) => "`None` literals",
        Expr::BooleanLiteral(_) => "boolean literals",
        Expr::ListComp(_) => "list comprehensions",
        Expr::SetComp(_) => "set comprehensions",
        Expr::DictComp(_) => "dict comprehensions",
        Expr::Generator(_) => "generator expressions",
        Expr::Yield(_) => "`yield` expressions",
        Expr::YieldFrom(_) => "`yield from` expressions",
        Expr::Compare(_) => "comparison expressions",
        Expr::Call(_) => "function calls",
        Expr::FString(_) => "f-strings",
        Expr::Named(_) => "assignment expressions",
        Expr::Subscript(_) => "subscript expressions",
        Expr::IpyEscapeCommand(_) => "IPython escape commands",
    };

    Some((description, expr.range()))
}
