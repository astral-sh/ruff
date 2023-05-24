use crate::prelude::*;
use ruff_formatter::{write, Format};

use crate::cst::Attributed;
use crate::format::builders::literal;
use crate::trivia::TriviaKind;

#[derive(Debug)]
pub(crate) struct LeadingComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext<'_>> for LeadingComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for trivia in &self.item.trivia {
            if trivia.relationship.is_leading() {
                match trivia.kind {
                    TriviaKind::EmptyLine => {
                        write!(f, [empty_line()])?;
                    }
                    TriviaKind::OwnLineComment(range) => {
                        write!(f, [literal(range, ContainsNewlines::No), hard_line_break()])?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub(crate) const fn leading_comments<T>(item: &Attributed<T>) -> LeadingComments<'_, T> {
    LeadingComments { item }
}

#[derive(Debug)]
pub(crate) struct TrailingComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext<'_>> for TrailingComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for trivia in &self.item.trivia {
            if trivia.relationship.is_trailing() {
                match trivia.kind {
                    TriviaKind::EmptyLine => {
                        write!(f, [empty_line()])?;
                    }
                    TriviaKind::OwnLineComment(range) => {
                        write!(f, [literal(range, ContainsNewlines::No), hard_line_break()])?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub(crate) const fn trailing_comments<T>(item: &Attributed<T>) -> TrailingComments<'_, T> {
    TrailingComments { item }
}

#[derive(Debug)]
pub(crate) struct EndOfLineComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext<'_>> for EndOfLineComments<'_, T> {
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
            write!(f, [line_suffix(&literal(range, ContainsNewlines::No))])?;
        }
        Ok(())
    }
}

#[inline]
pub(crate) const fn end_of_line_comments<T>(item: &Attributed<T>) -> EndOfLineComments<'_, T> {
    EndOfLineComments { item }
}

#[derive(Debug)]
pub(crate) struct DanglingComments<'a, T> {
    item: &'a Attributed<T>,
}

impl<T> Format<ASTFormatContext<'_>> for DanglingComments<'_, T> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for trivia in &self.item.trivia {
            if trivia.relationship.is_dangling() {
                if let TriviaKind::OwnLineComment(range) = trivia.kind {
                    write!(f, [hard_line_break()])?;
                    write!(f, [literal(range, ContainsNewlines::No)])?;
                    write!(f, [hard_line_break()])?;
                }
            }
        }
        Ok(())
    }
}

#[inline]
pub(crate) const fn dangling_comments<T>(item: &Attributed<T>) -> DanglingComments<'_, T> {
    DanglingComments { item }
}
