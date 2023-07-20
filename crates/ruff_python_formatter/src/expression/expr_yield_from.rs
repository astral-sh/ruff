use crate::context::PyFormatContext;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parenthesize};
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprYieldFrom;

#[derive(Default)]
pub struct FormatExprYieldFrom;

impl FormatNodeRule<ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, item: &ExprYieldFrom, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprYieldFrom { range: _, value } = item;

        write!(
            f,
            [
                &text("yield from"),
                space(),
                maybe_parenthesize_expression(value, item, Parenthesize::IfRequired)
            ]
        )?;

        Ok(())
    }
}

impl NeedsParentheses for ExprYieldFrom {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_stmt_assign() || parent.is_stmt_ann_assign() || parent.is_stmt_aug_assign() {
            OptionalParentheses::Multiline
        } else {
            OptionalParentheses::Always
        }
    }
}
