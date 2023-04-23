//! Doc line extraction. In this context, a doc line is a line consisting of a
//! standalone comment or a constant string statement.

use ruff_text_size::{TextRange, TextSize};
use std::iter::FusedIterator;

use ruff_python_ast::source_code::Locator;
use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind, Suite};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

/// Extract doc lines (standalone comments) from a token sequence.
pub fn doc_lines_from_tokens<'a>(lxr: &'a [LexResult], locator: &'a Locator<'a>) -> DocLines<'a> {
    DocLines::new(lxr, locator)
}

pub struct DocLines<'a> {
    inner: std::iter::Flatten<core::slice::Iter<'a, LexResult>>,
    locator: &'a Locator<'a>,
    prev: TextSize,
}

impl<'a> DocLines<'a> {
    fn new(lxr: &'a [LexResult], locator: &'a Locator) -> Self {
        Self {
            inner: lxr.iter().flatten(),
            locator,
            prev: TextSize::default(),
        }
    }
}

impl Iterator for DocLines<'_> {
    type Item = TextSize;

    fn next(&mut self) -> Option<Self::Item> {
        let mut at_start_of_line = true;
        loop {
            let (tok, range) = self.inner.next()?;

            match tok {
                Tok::Comment(..) => {
                    if at_start_of_line
                        || self
                            .locator
                            .contains_line_break(TextRange::new(self.prev, range.start()))
                    {
                        break Some(range.start());
                    }
                }
                Tok::Newline => {
                    at_start_of_line = true;
                }
                Tok::Indent | Tok::Dedent => {
                    // ignore
                }
                _ => {
                    at_start_of_line = false;
                }
            }

            self.prev = range.end();
        }
    }
}

impl FusedIterator for DocLines<'_> {}

#[derive(Default)]
struct StringLinesVisitor {
    string_lines: Vec<TextSize>,
}

impl Visitor<'_> for StringLinesVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let StmtKind::Expr { value } = &stmt.node {
            if let ExprKind::Constant {
                value: Constant::Str(..),
                ..
            } = &value.node
            {
                self.string_lines.push(value.start());
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// Extract doc lines (standalone strings) start positions from an AST.
pub fn doc_lines_from_ast(python_ast: &Suite) -> Vec<TextSize> {
    let mut visitor = StringLinesVisitor::default();
    visitor.visit_body(python_ast);
    visitor.string_lines
}
