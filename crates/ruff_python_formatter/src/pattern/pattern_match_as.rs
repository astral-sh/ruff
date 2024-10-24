use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchAs;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchAs;

impl FormatNodeRule<PatternMatchAs> for FormatPatternMatchAs {
    fn fmt_fields(&self, item: &PatternMatchAs, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchAs {
            range: _,
            pattern,
            name,
        } = item;

        let comments = f.context().comments().clone();

        if let Some(name) = name {
            if let Some(pattern) = pattern {
                pattern.format().fmt(f)?;

                if comments.has_trailing(pattern.as_ref()) {
                    write!(f, [hard_line_break()])?;
                } else {
                    write!(f, [space()])?;
                }

                write!(f, [token("as")])?;

                let trailing_as_comments = comments.dangling(item);
                if trailing_as_comments.is_empty() {
                    write!(f, [space()])?;
                } else if trailing_as_comments
                    .iter()
                    .all(|comment| comment.line_position().is_own_line())
                {
                    write!(f, [hard_line_break()])?;
                }
                write!(f, [dangling_comments(trailing_as_comments)])?;
            }
            name.format().fmt(f)
        } else {
            debug_assert!(pattern.is_none());
            token("_").fmt(f)
        }
    }
}

impl NeedsParentheses for PatternMatchAs {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.name.is_some() {
            OptionalParentheses::Multiline
        } else {
            OptionalParentheses::BestFit
        }
    }
}
