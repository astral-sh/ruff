use ruff_formatter::prelude::{space, token};
use ruff_formatter::write;
use ruff_python_ast::StmtAssert;

use crate::comments::SourceComment;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::preview::is_join_implicit_concatenated_string_enabled;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssert {
            range: _,
            test,
            msg,
        } = item;

        write!(
            f,
            [
                token("assert"),
                space(),
                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks)
            ]
        )?;

        if let Some(msg) = msg {
            let parenthesize = if is_join_implicit_concatenated_string_enabled(f.context()) {
                Parenthesize::IfBreaksParenthesized
            } else {
                Parenthesize::IfBreaks
            };

            write!(
                f,
                [
                    token(","),
                    space(),
                    maybe_parenthesize_expression(msg, item, parenthesize),
                ]
            )?;
        }

        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
