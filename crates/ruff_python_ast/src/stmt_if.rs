use std::iter;

use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::{ElifElseClause, Expr, Stmt, StmtIf};

/// Return the `Range` of the first `Elif` or `Else` token in an `If` statement.
pub fn elif_else_range(clause: &ElifElseClause, contents: &str) -> Option<TextRange> {
    let token = SimpleTokenizer::new(contents, clause.range)
        .skip_trivia()
        .next()?;
    matches!(token.kind, SimpleTokenKind::Elif | SimpleTokenKind::Else).then_some(token.range())
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BranchKind {
    If,
    Elif,
}

#[derive(Debug)]
pub struct IfElifBranch<'a, 'ast> {
    pub kind: BranchKind,
    pub test: &'a Expr<'ast>,
    pub body: &'a [Stmt<'ast>],
    range: TextRange,
}

impl Ranged for IfElifBranch<'_, '_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

pub fn if_elif_branches<'a, 'ast>(
    stmt_if: &'a StmtIf<'ast>,
) -> impl Iterator<Item = IfElifBranch<'a, 'ast>> {
    iter::once(IfElifBranch {
        kind: BranchKind::If,
        test: stmt_if.test.as_ref(),
        body: stmt_if.body.as_slice(),
        range: TextRange::new(stmt_if.start(), stmt_if.body.last().unwrap().end()),
    })
    .chain(stmt_if.elif_else_clauses.iter().filter_map(|clause| {
        Some(IfElifBranch {
            kind: BranchKind::Elif,
            test: clause.test.as_ref()?,
            body: clause.body.as_slice(),
            range: clause.range,
        })
    }))
}
