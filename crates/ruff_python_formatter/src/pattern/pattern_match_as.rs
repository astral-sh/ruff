use ruff_formatter::write;
use ruff_python_ast::PatternMatchAs;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchAs;

impl FormatNodeRule<PatternMatchAs> for FormatPatternMatchAs {
    fn fmt_fields(
        &self,
        item: flat::View<'_, PatternMatchAs>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let pattern = item.pattern();
        let name = item.name();

        let comments = f.context().comments().clone();

        if let Some(name) = name {
            if let Some(pattern) = pattern {
                pattern.format().fmt(f)?;

                if comments.has_trailing(pattern) {
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

impl NeedsParentheses for flat::View<'_, PatternMatchAs> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.name().is_some() {
            OptionalParentheses::Multiline
        } else {
            OptionalParentheses::BestFit
        }
    }
}
