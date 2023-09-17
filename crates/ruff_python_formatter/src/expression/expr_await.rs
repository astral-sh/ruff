use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, ExprAwait};

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses, Parenthesize,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprAwait;

impl FormatNodeRule<ExprAwait> for FormatExprAwait {
    fn fmt_fields(&self, item: &ExprAwait, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAwait { range: _, value } = item;

        // Drop parentheses around low-precedence operators (constants, names, attributes, and
        // subscripts). It would also be syntactically valid to drop parentheses around lists,
        // sets, and dictionaries, but Black doesn't do that.
        // See: https://docs.python.org/3/reference/expressions.html#operator-precedence
        let parenthesize = match value.as_ref() {
            Expr::FString(_)
            | Expr::Constant(_)
            | Expr::Attribute(_)
            | Expr::Subscript(_)
            | Expr::Call(_)
            | Expr::Name(_) => Parenthesize::IfBreaks,

            Expr::Dict(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::BoolOp(_)
            | Expr::NamedExpr(_)
            | Expr::BinOp(_)
            | Expr::UnaryOp(_)
            | Expr::Lambda(_)
            | Expr::IfExp(_)
            | Expr::GeneratorExp(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::Compare(_)
            | Expr::FormattedValue(_)
            | Expr::Starred(_)
            | Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Slice(_)
            | Expr::IpyEscapeCommand(_) => Parenthesize::Optional,
        };

        write!(
            f,
            [
                token("await"),
                space(),
                maybe_parenthesize_expression(value, item, parenthesize)
            ]
        )
    }
}

impl NeedsParentheses for ExprAwait {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else if is_expression_parenthesized(
            self.value.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            OptionalParentheses::Never
        } else {
            self.value.needs_parentheses(self.into(), context)
        }
    }
}
