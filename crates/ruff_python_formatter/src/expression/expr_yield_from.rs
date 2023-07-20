use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses, Parenthesize};
use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult, Format, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::{ExprYield, ExprYieldFrom};
use ruff_formatter::prelude::{space, text};
use crate::expression::maybe_parenthesize_expression;

#[derive(Default)]
pub struct FormatExprYieldFrom;

impl FormatNodeRule<ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, item: &ExprYieldFrom, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprYieldFrom {
            range: _,
            value
        } = item;

        write!(
            f,
            [&text("yield from"), space(), maybe_parenthesize_expression(value, item, Parenthesize::IfRequired)]
        )?;

        Ok(())

    }
}

impl NeedsParentheses for ExprYieldFrom {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if _parent.is_stmt_return() || _parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}
