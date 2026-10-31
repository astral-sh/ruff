use ruff_formatter::write;
use ruff_python_ast::PatternMatchStar;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar> for FormatPatternMatchStar {
    fn fmt_fields(
        &self,
        item: flat::View<'_, PatternMatchStar>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let name = item.name();

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

impl NeedsParentheses for flat::View<'_, PatternMatchStar> {
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
