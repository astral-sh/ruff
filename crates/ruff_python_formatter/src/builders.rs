use rome_formatter::prelude::*;
use rome_formatter::{write, Format};

use crate::context::ASTFormatContext;
use crate::core::types::Range;
use crate::trivia::{Relationship, Trivia, TriviaKind};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Literal {
    range: Range,
}

impl Format<ASTFormatContext<'_>> for Literal {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let (text, start, end) = f.context().locator().slice(self.range);
        f.write_element(FormatElement::Text { text, start, end })
    }
}

// TODO(charlie): We still can't use this everywhere we'd like. We need the AST
// to include ranges for all `Ident` nodes.
#[inline]
pub const fn literal(range: Range) -> Literal {
    Literal { range }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct LeadingComments<'a> {
    comments: &'a [Trivia],
}

impl Format<ASTFormatContext<'_>> for LeadingComments<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        for comment in self.comments {
            if matches!(comment.relationship, Relationship::Leading) {
                if let TriviaKind::StandaloneComment(range) = comment.kind {
                    write!(f, [hard_line_break()])?;
                    write!(f, [literal(range)])?;
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub const fn leading_comments(comments: &[Trivia]) -> LeadingComments {
    LeadingComments { comments }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TrailingComments<'a> {
    comments: &'a [Trivia],
}

impl Format<ASTFormatContext<'_>> for TrailingComments<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        for comment in self.comments {
            if matches!(comment.relationship, Relationship::Trailing) {
                if let TriviaKind::StandaloneComment(range) = comment.kind {
                    write!(f, [hard_line_break()])?;
                    write!(f, [literal(range)])?;
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub const fn trailing_comments(comments: &[Trivia]) -> TrailingComments {
    TrailingComments { comments }
}
