use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::PatternMatchStar;

use crate::comments::{dangling_comments, SourceComment};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar> for FormatPatternMatchStar {
    fn fmt_fields(&self, item: &PatternMatchStar, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchStar { name, .. } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("*"), dangling_comments(dangling)])?;

        match name {
            Some(name) => write!(f, [name.format()]),
            None => write!(f, [token("_")]),
        }
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

impl NeedsParentheses for PatternMatchStar {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
