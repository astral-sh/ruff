//! Doc line extraction. In this context, a doc line is a line consisting of a
//! standalone comment or a constant string statement.

use std::iter::FusedIterator;

use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind, Suite};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

/// Extract doc lines (standalone comments) from a token sequence.
pub fn doc_lines_from_tokens(lxr: &[LexResult]) -> DocLines {
    DocLines::new(lxr)
}

pub struct DocLines<'a> {
    inner: std::iter::Flatten<core::slice::Iter<'a, LexResult>>,
    prev: Option<usize>,
}

impl<'a> DocLines<'a> {
    fn new(lxr: &'a [LexResult]) -> Self {
        Self {
            inner: lxr.iter().flatten(),
            prev: None,
        }
    }
}

impl Iterator for DocLines<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (start, tok, end) = self.inner.next()?;

            match tok {
                Tok::Indent | Tok::Dedent | Tok::Newline => continue,
                Tok::Comment(..) => {
                    if let Some(prev) = self.prev {
                        if start.row() > prev {
                            break Some(start.row());
                        }
                    } else {
                        break Some(start.row());
                    }
                }
                _ => {}
            }

            self.prev = Some(end.row());
        }
    }
}

impl FusedIterator for DocLines<'_> {}

#[derive(Default)]
struct StringLinesVisitor {
    string_lines: Vec<usize>,
}

impl Visitor<'_> for StringLinesVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let StmtKind::Expr { value } = &stmt.node {
            if let ExprKind::Constant {
                value: Constant::Str(..),
                ..
            } = &value.node
            {
                self.string_lines
                    .extend(value.location.row()..=value.end_location.unwrap().row());
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// Extract doc lines (standalone strings) from an AST.
pub fn doc_lines_from_ast(python_ast: &Suite) -> Vec<usize> {
    let mut visitor = StringLinesVisitor::default();
    visitor.visit_body(python_ast);
    visitor.string_lines
}
