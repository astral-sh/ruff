use ruff_formatter::prelude::*;
use ruff_formatter::{write, Format};
use ruff_text_size::TextRange;

use crate::context::ASTFormatContext;
use crate::cst::{Body, Stmt};
use crate::shared_traits::AsFormat;
use crate::trivia::{Relationship, TriviaKind};

#[derive(Copy, Clone)]
pub(crate) struct Block<'a> {
    body: &'a Body,
}

impl Format<ASTFormatContext> for Block<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for (i, stmt) in self.body.iter().enumerate() {
            if i > 0 {
                write!(f, [hard_line_break()])?;
            }
            write!(f, [stmt.format()])?;
        }

        for trivia in &self.body.trivia {
            if matches!(trivia.relationship, Relationship::Dangling) {
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
pub(crate) fn block(body: &Body) -> Block {
    Block { body }
}

#[derive(Copy, Clone)]
pub(crate) struct Statements<'a> {
    suite: &'a [Stmt],
}

impl Format<ASTFormatContext> for Statements<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        for (i, stmt) in self.suite.iter().enumerate() {
            if i > 0 {
                write!(f, [hard_line_break()])?;
            }
            write!(f, [stmt.format()])?;
        }
        Ok(())
    }
}

pub(crate) fn statements(suite: &[Stmt]) -> Statements {
    Statements { suite }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct Literal {
    range: TextRange,
}

impl Format<ASTFormatContext> for Literal {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let text = f.context().contents();

        f.write_element(FormatElement::StaticTextSlice {
            text,
            range: self.range,
        })
    }
}

#[inline]
pub(crate) const fn literal(range: TextRange) -> Literal {
    Literal { range }
}

pub(crate) const fn join_names(names: &[String]) -> JoinNames {
    JoinNames { names }
}

pub(crate) struct JoinNames<'a> {
    names: &'a [String],
}

impl<Context> Format<Context> for JoinNames<'_> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let mut join = f.join_with(text(", "));
        for name in self.names {
            join.entry(&dynamic_text(name, None));
        }
        join.finish()
    }
}
