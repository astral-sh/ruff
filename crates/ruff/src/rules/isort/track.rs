use std::path::Path;

use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Excepthandler,
    ExcepthandlerKind, Expr, ExprContext, Keyword, MatchCase, Operator, Pattern, Stmt, StmtKind,
    Unaryop, Withitem,
};

use super::helpers;
use crate::ast::visitor::Visitor;
use crate::directives::IsortDirectives;
use crate::source_code::Locator;

#[derive(Debug)]
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

pub struct ImportTracker<'a> {
    locator: &'a Locator<'a>,
    directives: &'a IsortDirectives,
    pyi: bool,
    blocks: Vec<Block<'a>>,
    split_index: usize,
    nested: bool,
}

impl<'a> ImportTracker<'a> {
    pub fn new(locator: &'a Locator<'a>, directives: &'a IsortDirectives, path: &'a Path) -> Self {
        Self {
            locator,
            directives,
            pyi: path.extension().map_or(false, |ext| ext == "pyi"),
            blocks: vec![Block::default()],
            split_index: 0,
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

        Some(if self.pyi {
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
                StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                    if helpers::has_comment_break(stmt, self.locator) {
                        Trailer::Sibling
                    } else {
                        Trailer::FunctionDef
                    }
                }
                StmtKind::ClassDef { .. } => {
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

    pub fn iter<'b>(&'a self) -> impl Iterator<Item = &'b Block<'a>>
    where
        'a: 'b,
    {
        self.blocks.iter()
    }
}

impl<'a, 'b> Visitor<'b> for ImportTracker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        // Track manual splits.
        while self.split_index < self.directives.splits.len() {
            if stmt.location.row() >= self.directives.splits[self.split_index] {
                self.finalize(self.trailer_for(stmt));
                self.split_index += 1;
            } else {
                break;
            }
        }

        // Track imports.
        if matches!(
            stmt.node,
            StmtKind::Import { .. } | StmtKind::ImportFrom { .. }
        ) && !self.directives.exclusions.contains(&stmt.location.row())
        {
            self.track_import(stmt);
        } else {
            self.finalize(self.trailer_for(stmt));
        }

        // Track scope.
        let prev_nested = self.nested;
        self.nested = true;
        match &stmt.node {
            StmtKind::FunctionDef { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::AsyncFunctionDef { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::ClassDef { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::For { body, orelse, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::AsyncFor { body, orelse, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::While { body, orelse, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::If { body, orelse, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);

                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::With { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::AsyncWith { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.finalize(None);
            }
            StmtKind::Match { cases, .. } => {
                for match_case in cases {
                    self.visit_match_case(match_case);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
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

    fn visit_annotation(&mut self, _: &'b Expr) {}

    fn visit_expr(&mut self, _: &'b Expr) {}

    fn visit_constant(&mut self, _: &'b Constant) {}

    fn visit_expr_context(&mut self, _: &'b ExprContext) {}

    fn visit_boolop(&mut self, _: &'b Boolop) {}

    fn visit_operator(&mut self, _: &'b Operator) {}

    fn visit_unaryop(&mut self, _: &'b Unaryop) {}

    fn visit_cmpop(&mut self, _: &'b Cmpop) {}

    fn visit_comprehension(&mut self, _: &'b Comprehension) {}

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        let prev_nested = self.nested;
        self.nested = true;

        let ExcepthandlerKind::ExceptHandler { body, .. } = &excepthandler.node;
        for stmt in body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);

        self.nested = prev_nested;
    }

    fn visit_arguments(&mut self, _: &'b Arguments) {}

    fn visit_arg(&mut self, _: &'b Arg) {}

    fn visit_keyword(&mut self, _: &'b Keyword) {}

    fn visit_alias(&mut self, _: &'b Alias) {}

    fn visit_withitem(&mut self, _: &'b Withitem) {}

    fn visit_match_case(&mut self, match_case: &'b MatchCase) {
        for stmt in &match_case.body {
            self.visit_stmt(stmt);
        }
        self.finalize(None);
    }

    fn visit_pattern(&mut self, _: &'b Pattern) {}
}
