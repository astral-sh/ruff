use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, CmpOp, Expr, Number};
use ruff_text_size::{Ranged, TextRange};

use crate::error::RelaxedDecoratorError;

/// Converts a [`TokenKind`] into its corresponding [`CmpOp`].
///
/// `next` is only needed to distinguish `is not` and `not in`.
pub(super) const fn token_kind_to_cmp_op(
    current: TokenKind,
    next: Option<TokenKind>,
) -> Option<CmpOp> {
    Some(match (current, next) {
        (TokenKind::Is, Some(TokenKind::Not)) => CmpOp::IsNot,
        (TokenKind::Is, _) => CmpOp::Is,
        (TokenKind::Not, Some(TokenKind::In)) => CmpOp::NotIn,
        (TokenKind::In, _) => CmpOp::In,
        (TokenKind::EqEqual, _) => CmpOp::Eq,
        (TokenKind::NotEqual, _) => CmpOp::NotEq,
        (TokenKind::Less, _) => CmpOp::Lt,
        (TokenKind::LessEqual, _) => CmpOp::LtE,
        (TokenKind::Greater, _) => CmpOp::Gt,
        (TokenKind::GreaterEqual, _) => CmpOp::GtE,
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
    expr: Expr<'_>,
) -> Option<(RelaxedDecoratorError, TextRange)> {
    let description = match expr.classify() {
        ast::ExprKind::Name(_) => return None,

        ast::ExprKind::Attribute(attribute) => {
            return detect_invalid_pre_py39_decorator_node(attribute.value());
        }

        ast::ExprKind::Call(_) => {
            return Some((RelaxedDecoratorError::CallExpression, expr.range()));
        }

        ast::ExprKind::NumberLiteral(number) => match number.value() {
            Number::Int(_) => "an int literal",
            Number::Float(_) => "a float literal",
            Number::Complex { .. } => "a complex literal",
        },

        ast::ExprKind::BoolOp(_) => "boolean expression",
        ast::ExprKind::BinOp(_) => "binary-operation expression",
        ast::ExprKind::UnaryOp(_) => "unary-operation expression",
        ast::ExprKind::Await(_) => "`await` expression",
        ast::ExprKind::Lambda(_) => "lambda expression",
        ast::ExprKind::If(_) => "conditional expression",
        ast::ExprKind::Dict(_) => "a dict literal",
        ast::ExprKind::Set(_) => "a set literal",
        ast::ExprKind::List(_) => "a list literal",
        ast::ExprKind::Tuple(_) => "a tuple literal",
        ast::ExprKind::Starred(_) => "starred expression",
        ast::ExprKind::Slice(_) => "slice expression",
        ast::ExprKind::BytesLiteral(_) => "a bytes literal",
        ast::ExprKind::StringLiteral(_) => "a string literal",
        ast::ExprKind::EllipsisLiteral(_) => "an ellipsis literal",
        ast::ExprKind::NoneLiteral(_) => "a `None` literal",
        ast::ExprKind::BooleanLiteral(_) => "a boolean literal",
        ast::ExprKind::ListComp(_) => "a list comprehension",
        ast::ExprKind::SetComp(_) => "a set comprehension",
        ast::ExprKind::DictComp(_) => "a dict comprehension",
        ast::ExprKind::Generator(_) => "generator expression",
        ast::ExprKind::Yield(_) => "`yield` expression",
        ast::ExprKind::YieldFrom(_) => "`yield from` expression",
        ast::ExprKind::Compare(_) => "comparison expression",
        ast::ExprKind::FString(_) => "f-string",
        ast::ExprKind::TString(_) => "t-string",
        ast::ExprKind::Named(_) => "assignment expression",
        ast::ExprKind::Subscript(_) => "subscript expression",
        ast::ExprKind::IpyEscapeCommand(_) => "IPython escape command",
    };

    Some((RelaxedDecoratorError::Other(description), expr.range()))
}
