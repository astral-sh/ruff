use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::MatchCase;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::not_yet_implemented_custom_text;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            pattern: _,
            guard,
            body,
        } = item;

        write!(
            f,
            [
                text("case"),
                space(),
                not_yet_implemented_custom_text("NOT_YET_IMPLEMENTED_Pattern"),
            ]
        )?;

        if let Some(guard) = guard {
            write!(
                f,
                [
                    space(),
                    text("if"),
                    space(),
                    maybe_parenthesize_expression(guard, item, Parenthesize::IfBreaks)
                ]
            )?;
        }

        write!(f, [text(":"), block_indent(&body.format())])
    }
}
