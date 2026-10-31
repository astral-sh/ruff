use ruff_python_ast::ExprIpyEscapeCommand;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprIpyEscapeCommand;

impl FormatNodeRule<ExprIpyEscapeCommand> for FormatExprIpyEscapeCommand {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprIpyEscapeCommand>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        source_text_slice(item.range()).fmt(f)
    }
}

impl NeedsParentheses for flat::View<'_, ExprIpyEscapeCommand> {
    fn needs_parentheses(
        &self,
        _parent: ruff_python_ast::flat::AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
