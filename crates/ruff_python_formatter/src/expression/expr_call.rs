use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, ExprCall};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatExprCall {
    fluent_style: bool,
}

impl FormatRuleWithOptions<ExprCall, PyFormatContext<'_>> for FormatExprCall {
    type Options = bool;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.fluent_style = options;
        self
    }
}

impl FormatNodeRule<ExprCall> for FormatExprCall {
    fn fmt_fields(&self, item: &ExprCall, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprCall {
            range: _,
            func,
            arguments,
        } = item;

        match func.as_ref() {
            Expr::Attribute(expr) => expr.format().with_options(self.fluent_style).fmt(f)?,
            Expr::Call(expr) => expr.format().with_options(self.fluent_style).fmt(f)?,
            Expr::Subscript(expr) => expr.format().with_options(self.fluent_style).fmt(f)?,
            _ => func.format().fmt(f)?,
        }

        write!(f, [arguments.format()])
    }
}

impl NeedsParentheses for ExprCall {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        self.func.needs_parentheses(self.into(), context)
    }
}
