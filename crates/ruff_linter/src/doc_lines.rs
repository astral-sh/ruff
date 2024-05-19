//! Doc line extraction. In this context, a doc line is a line consisting of a
//! standalone comment or a constant string statement.

use std::iter::FusedIterator;

use ruff_python_ast::{self as ast, Stmt, Suite};
use ruff_python_parser::{TokenKind, TokenKindIter};
use ruff_text_size::{Ranged, TextSize};

use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_source_file::{Locator, UniversalNewlineIterator};

/// Extract doc lines (standalone comments) from a token sequence.
pub(crate) fn doc_lines_from_tokens(tokens: TokenKindIter) -> DocLines {
    DocLines::new(tokens)
}

pub(crate) struct DocLines<'a> {
    inner: TokenKindIter<'a>,
    prev: TextSize,
}

impl<'a> DocLines<'a> {
    fn new(tokens: TokenKindIter<'a>) -> Self {
        Self {
            inner: tokens,
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
                TokenKind::Comment => {
                    if at_start_of_line {
                        break Some(range.start());
                    }
                }
                TokenKind::Newline | TokenKind::NonLogicalNewline => {
                    at_start_of_line = true;
                }
                TokenKind::Indent | TokenKind::Dedent => {
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

struct StringLinesVisitor<'a> {
    string_lines: Vec<TextSize>,
    locator: &'a Locator<'a>,
}

impl StatementVisitor<'_> for StringLinesVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let Stmt::Expr(ast::StmtExpr {
            value: expr,
            range: _,
        }) = stmt
        {
            if expr.is_string_literal_expr() {
                for line in UniversalNewlineIterator::with_offset(
                    self.locator.slice(expr.as_ref()),
                    expr.start(),
                ) {
                    self.string_lines.push(line.start());
                }
            }
        }
        walk_stmt(self, stmt);
    }
}

impl<'a> StringLinesVisitor<'a> {
    fn new(locator: &'a Locator<'a>) -> Self {
        Self {
            string_lines: Vec::new(),
            locator,
        }
    }
}

/// Extract doc lines (standalone strings) start positions from an AST.
pub(crate) fn doc_lines_from_ast(python_ast: &Suite, locator: &Locator) -> Vec<TextSize> {
    let mut visitor = StringLinesVisitor::new(locator);
    visitor.visit_body(python_ast);
    visitor.string_lines
}
