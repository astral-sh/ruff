use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchStar;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar<'_>> for FormatPatternMatchStar {
    fn fmt_fields(&self, item: &PatternMatchStar<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchStar { name, .. } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("*"), dangling_comments(dangling)])?;

        if let Some(name) = name {
            write!(f, [name.format()])
        } else {
            write!(f, [token("_")])
        }
    }
}

impl NeedsParentheses for PatternMatchStar<'_> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        // Doesn't matter what we return here because starred patterns can never be used
        // outside a sequence pattern.
        OptionalParentheses::Never
    }
}
