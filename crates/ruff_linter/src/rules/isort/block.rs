use std::iter::Peekable;
use std::slice;

use ruff_notebook::CellOffsets;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{self as ast, ElifElseClause, ExceptHandler, MatchCase, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::directives::IsortDirectives;
use crate::rules::isort::helpers;
use crate::Locator;

/// A block of imports within a Python module.
#[derive(Debug, Default)]
pub(crate) struct Block<'a> {
    pub(crate) nested: bool,
    pub(crate) imports: Vec<&'a Stmt>,
    pub(crate) trailer: Option<Trailer>,
}

/// The type of trailer that should follow an import block.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Trailer {
    Sibling,
    ClassDef,
    FunctionDef,
}

/// A builder for identifying and constructing import blocks within a Python module.
pub(crate) struct BlockBuilder<'a> {
    locator: &'a Locator<'a>,
    is_stub: bool,
    blocks: Vec<Block<'a>>,
    splits: Peekable<slice::Iter<'a, TextSize>>,
    cell_offsets: Option<Peekable<slice::Iter<'a, TextSize>>>,
    exclusions: &'a [TextRange],
    nested: bool,
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn new(
        locator: &'a Locator<'a>,
        directives: &'a IsortDirectives,
        is_stub: bool,
        cell_offsets: Option<&'a CellOffsets>,
    ) -> Self {
        Self {
            locator,
            is_stub,
            blocks: vec![Block::default()],
            splits: directives.splits.iter().peekable(),
            exclusions: &directives.exclusions,
            nested: false,
            cell_offsets: cell_offsets.map(|offsets| offsets.iter().peekable()),
        }
    }

    fn track_import(&mut self, stmt: &'a Stmt) {
        let index = self.blocks.len() - 1;
        self.blocks[index].imports.push(stmt);
        self.blocks[index].nested = self.nested;
    }

    fn trailer_for(&self, stmt: &'a Stmt) -> Option<Trailer> {
        // No need to compute trailers if we won't be finalizing anything.
        let index = self.blocks.len() - 1;
        if self.blocks[index].imports.is_empty() {
            return None;
        }

        // Similar to isort, avoid enforcing any newline behaviors in nested blocks.
        if self.nested {
            return None;
        }

        Some(if self.is_stub {
            // Black treats interface files differently, limiting to one newline
            // (`Trailing::Sibling`).
            Trailer::Sibling
        } else {
            // If the import block is followed by a class or function, we want to enforce
            // two blank lines. The exception: if, between the import and the class or
            // function, we have at least one commented line, followed by at
            // least one blank line. In that case, we treat it as a regular
            // sibling (i.e., as if the comment is the next statement, as
            // opposed to the class or function).
            match stmt {
                Stmt::FunctionDef(_) => {
                    if helpers::has_comment_break(stmt, self.locator) {
                        Trailer::Sibling
                    } else {
                        Trailer::FunctionDef
                    }
                }
                Stmt::ClassDef(_) => {
                    if helpers::has_comment_break(stmt, self.locator) {
                        Trailer::Sibling
                    } else {
                        Trailer::ClassDef
                    }
                }
                _ => Trailer::Sibling,
            }
        })
    }

    fn finalize(&mut self, trailer: Option<Trailer>) {
        let index = self.blocks.len() - 1;
        if !self.blocks[index].imports.is_empty() {
            self.blocks[index].trailer = trailer;
            self.blocks.push(Block::default());
        }
    }

    pub(crate) fn iter<'b>(&'a self) -> impl Iterator<Item = &'b Block<'a>>
    where
        'a: 'b,
    {
        self.blocks.iter()
    }
}

impl<'a> StatementVisitor<'a> for BlockBuilder<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        // Track manual splits (e.g., `# isort: split`).
        if self
            .splits
            .next_if(|split| stmt.start() >= **split)
            .is_some()
        {
            // Skip any other splits that occur before the current statement, to support, e.g.:
            // ```python
            // # isort: split
            // # isort: split
            // import foo
            // ```
            while self
                .splits
                .peek()
                .is_some_and(|split| stmt.start() >= **split)
            {
                self.splits.next();
            }

            self.finalize(self.trailer_for(stmt));
        }

        // Track Jupyter notebook cell offsets as splits. This will make sure
        // that each cell is considered as an individual block to organize the
        // imports in. Thus, not creating an edit which spans across multiple
        // cells.
        if let Some(cell_offsets) = self.cell_offsets.as_mut() {
            if cell_offsets
                .next_if(|cell_offset| stmt.start() >= **cell_offset)
                .is_some()
            {
                // Skip any other cell offsets that occur before the current statement (e.g., in
                // the case of multiple empty cells).
                while cell_offsets
                    .peek()
                    .is_some_and(|split| stmt.start() >= **split)
                {
                    cell_offsets.next();
                }

                self.finalize(None);
            }
        }

        // Track imports.
        if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_))
            && !self
                .exclusions
                .iter()
                .any(|exclusion| exclusion.contains(stmt.start()))
        {
            self.track_import(stmt);
        } else {
            self.finalize(self.trailer_for(stmt));
        }

        // Track scope.
        let prev_nested = self.nested;
        self.nested = true;
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            Stmt::ClassDef(ast::StmtClassDef { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            Stmt::For(ast::StmtFor { body, orelse, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for clause in elif_else_clauses {
                    self.visit_elif_else_clause(clause);
                }
            }
            Stmt::With(ast::StmtWith { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                for match_case in cases {
                    self.visit_match_case(match_case);
                }
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                for except_handler in handlers {
                    self.visit_except_handler(except_handler);
                }

                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in finalbody {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            _ => {}
        }
        self.nested = prev_nested;
    }

    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        let prev_nested = self.nested;
        self.nested = true;

        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) =
            except_handler;
        for stmt in body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);

        self.nested = prev_nested;
    }

    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        for stmt in &match_case.body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);
    }

    fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause) {
        for stmt in &elif_else_clause.body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);
    }
}
