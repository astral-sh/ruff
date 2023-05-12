use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{self, Excepthandler, ExcepthandlerKind, MatchCase, Stmt, StmtKind};

use ruff_python_ast::source_code::Locator;
use ruff_python_ast::statement_visitor::StatementVisitor;

use crate::directives::IsortDirectives;
use crate::rules::isort::helpers;

#[derive(Debug, Copy, Clone)]
pub enum Trailer {
    Sibling,
    ClassDef,
    FunctionDef,
}

#[derive(Debug, Default)]
pub struct Block<'a> {
    pub nested: bool,
    pub imports: Vec<&'a Stmt>,
    pub trailer: Option<Trailer>,
}

pub(crate) struct ImportTracker<'a> {
    locator: &'a Locator<'a>,
    is_stub: bool,
    blocks: Vec<Block<'a>>,
    splits: &'a [TextSize],
    exclusions: &'a [TextRange],
    nested: bool,
}

impl<'a> ImportTracker<'a> {
    pub(crate) fn new(
        locator: &'a Locator<'a>,
        directives: &'a IsortDirectives,
        is_stub: bool,
    ) -> Self {
        Self {
            locator,
            is_stub,
            blocks: vec![Block::default()],
            splits: &directives.splits,
            exclusions: &directives.exclusions,
            nested: false,
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
            match &stmt.node {
                StmtKind::FunctionDef(_) | StmtKind::AsyncFunctionDef(_) => {
                    if helpers::has_comment_break(stmt, self.locator) {
                        Trailer::Sibling
                    } else {
                        Trailer::FunctionDef
                    }
                }
                StmtKind::ClassDef(_) => {
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

impl<'a, 'b> StatementVisitor<'b> for ImportTracker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        // Track manual splits.
        for (index, split) in self.splits.iter().enumerate() {
            if stmt.end() >= *split {
                self.finalize(self.trailer_for(stmt));
                self.splits = &self.splits[index + 1..];
            } else {
                break;
            }
        }

        // Test if the statement is in an excluded range
        let mut is_excluded = false;
        for (index, exclusion) in self.exclusions.iter().enumerate() {
            if exclusion.end() < stmt.start() {
                self.exclusions = &self.exclusions[index + 1..];
            } else {
                is_excluded = exclusion.contains(stmt.start());
                break;
            }
        }

        // Track imports.
        if matches!(stmt.node, StmtKind::Import(_) | StmtKind::ImportFrom(_)) && !is_excluded {
            self.track_import(stmt);
        } else {
            self.finalize(self.trailer_for(stmt));
        }

        // Track scope.
        let prev_nested = self.nested;
        self.nested = true;
        match &stmt.node {
            StmtKind::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::ClassDef(ast::StmtClassDef { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::For(ast::StmtFor { body, orelse, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::AsyncFor(ast::StmtAsyncFor { body, orelse, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::While(ast::StmtWhile { body, orelse, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::If(ast::StmtIf { body, orelse, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::With(ast::StmtWith { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::Match(ast::StmtMatch { cases, .. }) => {
                for match_case in cases {
                    self.visit_match_case(match_case);
                }
            }
            StmtKind::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
            })
            | StmtKind::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
            }) => {
                for excepthandler in handlers {
                    self.visit_excepthandler(excepthandler);
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

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        let prev_nested = self.nested;
        self.nested = true;

        let ExcepthandlerKind::ExceptHandler(ast::ExcepthandlerExceptHandler { body, .. }) =
            &excepthandler.node;
        for stmt in body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);

        self.nested = prev_nested;
    }

    fn visit_match_case(&mut self, match_case: &'b MatchCase) {
        for stmt in &match_case.body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);
    }
}
