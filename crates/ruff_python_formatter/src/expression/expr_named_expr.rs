use ruff_formatter::{format_args, write};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprNamedExpr;

use crate::comments::{dangling_comments, SourceComment};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprNamedExpr;

impl FormatNodeRule<ExprNamedExpr> for FormatExprNamedExpr {
    fn fmt_fields(&self, item: &ExprNamedExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprNamedExpr {
            target,
            value,
            range: _,
        } = item;

        // This context, a dangling comment is an end-of-line comment on the same line as the `:=`.
        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(
            f,
            [
                group(&format_args!(target.format(), soft_line_break_or_space())),
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

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled by `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprNamedExpr {
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
