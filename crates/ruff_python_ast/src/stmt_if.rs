use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;
use rustpython_ast::{ElifElseClause, Expr, Ranged, Stmt, StmtIf};
use std::iter;

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

pub struct IfElifBranch<'a> {
    pub kind: BranchKind,
    pub test: &'a Expr,
    pub body: &'a [Stmt],
    pub range: TextRange,
}

pub fn if_elif_branches(stmt_if: &StmtIf) -> impl Iterator<Item = IfElifBranch> {
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

#[cfg(test)]
mod test {
    use crate::stmt_if::elif_else_range;
    use ruff_text_size::TextSize;
    use rustpython_ast::Stmt;
    use rustpython_parser::{Parse, ParseError};

    #[test]
    fn extract_elif_else_range() -> Result<(), ParseError> {
        let contents = "if a:
    ...
elif b:
    ...
";
        let stmt = Stmt::parse(contents, "<filename>")?;
        let stmt = Stmt::as_if_stmt(&stmt).unwrap();
        let range = elif_else_range(&stmt.elif_else_clauses[0], contents).unwrap();
        assert_eq!(range.start(), TextSize::from(14));
        assert_eq!(range.end(), TextSize::from(18));

        let contents = "if a:
    ...
else:
    ...
";
        let stmt = Stmt::parse(contents, "<filename>")?;
        let stmt = Stmt::as_if_stmt(&stmt).unwrap();
        let range = elif_else_range(&stmt.elif_else_clauses[0], contents).unwrap();
        assert_eq!(range.start(), TextSize::from(14));
        assert_eq!(range.end(), TextSize::from(18));

        Ok(())
    }
}
