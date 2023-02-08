//! Doc line extraction. In this context, a doc line is a line consisting of a
//! standalone comment or a constant string statement.

use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind, Suite};
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::visitor;
use crate::ast::visitor::Visitor;

/// Extract doc lines (standalone comments) from a token sequence.
pub fn doc_lines_from_tokens(lxr: &[LexResult]) -> Vec<usize> {
    let mut doc_lines: Vec<usize> = Vec::default();
    let mut prev: Option<usize> = None;
    for (start, tok, end) in lxr.iter().flatten() {
        if matches!(tok, Tok::Indent | Tok::Dedent | Tok::Newline) {
            continue;
        }
        if matches!(tok, Tok::Comment(..)) {
            if let Some(prev) = prev {
                if start.row() > prev {
                    doc_lines.push(start.row());
                }
            } else {
                doc_lines.push(start.row());
            }
        }
        prev = Some(end.row());
    }
    doc_lines
}

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
