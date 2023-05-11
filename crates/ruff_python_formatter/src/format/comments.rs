use ruff_formatter::prelude::*;
use ruff_formatter::{write, Format};

use crate::context::ASTFormatContext;
use crate::cst::Attributed;
use crate::format::builders::literal;
use crate::trivia::TriviaKind;

#[derive(Debug)]
pub struct LeadingComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext> for LeadingComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for trivia in &self.item.trivia {
            if trivia.relationship.is_leading() {
                match trivia.kind {
                    TriviaKind::EmptyLine => {
                        write!(f, [empty_line()])?;
                    }
                    TriviaKind::OwnLineComment(range) => {
                        write!(f, [literal(range), hard_line_break()])?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub const fn leading_comments<T>(item: &Attributed<T>) -> LeadingComments<'_, T> {
    LeadingComments { item }
}

#[derive(Debug)]
pub struct TrailingComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext> for TrailingComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for trivia in &self.item.trivia {
            if trivia.relationship.is_trailing() {
                match trivia.kind {
                    TriviaKind::EmptyLine => {
                        write!(f, [empty_line()])?;
                    }
                    TriviaKind::OwnLineComment(range) => {
                        write!(f, [literal(range), hard_line_break()])?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub const fn trailing_comments<T>(item: &Attributed<T>) -> TrailingComments<'_, T> {
    TrailingComments { item }
}

#[derive(Debug)]
pub struct EndOfLineComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext> for EndOfLineComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let mut first = true;
        for range in self
            .item
            .trivia
            .iter()
            .filter_map(|trivia| trivia.kind.end_of_line_comment())
        {
            if std::mem::take(&mut first) {
                write!(f, [line_suffix(&text("  "))])?;
            }
            write!(f, [line_suffix(&literal(range))])?;
        }
        Ok(())
    }
}

#[inline]
pub const fn end_of_line_comments<T>(item: &Attributed<T>) -> EndOfLineComments<'_, T> {
    EndOfLineComments { item }
}

#[derive(Debug)]
pub struct DanglingComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext> for DanglingComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for trivia in &self.item.trivia {
            if trivia.relationship.is_dangling() {
                if let TriviaKind::OwnLineComment(range) = trivia.kind {
                    write!(f, [hard_line_break()])?;
                    write!(f, [literal(range)])?;
                    write!(f, [hard_line_break()])?;
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub const fn dangling_comments<T>(item: &Attributed<T>) -> DanglingComments<'_, T> {
    DanglingComments { item }
}
