use ruff_formatter::{format_args, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprNamed;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{
    in_parentheses_only_soft_line_break_or_space, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprNamed;

impl FormatNodeRule<ExprNamed> for FormatExprNamed {
    fn fmt_fields(&self, item: &ExprNamed, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprNamed {
            target,
            value,
            range: _,
        } = item;

        // This context, a dangling comment is a comment between the `:=` and the value.
        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(
            f,
            [
                group(&format_args![
                    target.format(),
                    in_parentheses_only_soft_line_break_or_space()
                ]),
                token(":=")
            ]
        )?;

        if dangling.is_empty() {
            write!(f, [space()])?;
        } else {
            write!(f, [dangling_comments(dangling), hard_line_break()])?;
        }

        write!(f, [value.format()])
    }
}

impl NeedsParentheses for ExprNamed {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        // Unlike tuples, named expression parentheses are not part of the range even when
        // mandatory. See [PEP 572](https://peps.python.org/pep-0572/) for details.
        if parent.is_stmt_ann_assign()
            || parent.is_stmt_assign()
            || parent.is_stmt_aug_assign()
            || parent.is_stmt_assert()
            || parent.is_stmt_return()
            || parent.is_except_handler_except_handler()
            || parent.is_with_item()
            || parent.is_expr_yield()
            || parent.is_expr_yield_from()
            || parent.is_expr_await()
            || parent.is_stmt_delete()
            || parent.is_stmt_for()
            || parent.is_stmt_function_def()
        {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}
