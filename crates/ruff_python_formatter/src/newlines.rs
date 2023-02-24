use rustpython_parser::ast::Constant;

use crate::core::visitor;
use crate::core::visitor::Visitor;
use crate::cst::{ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind};
use crate::trivia::{Relationship, Trivia, TriviaKind};

#[derive(Debug, Copy, Clone)]
enum Depth {
    TopLevel,
    Nested,
}

impl Depth {
    fn max_newlines(self) -> usize {
        match self {
            Self::TopLevel => 2,
            Self::Nested => 1,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Scope {
    Module,
    Class,
    Function,
}

#[derive(Debug, Copy, Clone)]
enum Trailer {
    None,
    ClassDef,
    FunctionDef,
    Import,
    Docstring,
    Generic,
    CompoundStatement,
}

struct NewlineNormalizer {
    depth: Depth,
    trailer: Trailer,
    scope: Scope,
}

impl<'a> Visitor<'a> for NewlineNormalizer {
    fn visit_stmt(&mut self, stmt: &'a mut Stmt) {
        // Remove any runs of empty lines greater than two in a row.
        let mut count = 0;
        stmt.trivia.retain(|c| {
            if c.kind.is_empty_line() && c.relationship.is_leading() {
                count += 1;
                count <= self.depth.max_newlines()
            } else {
                count = 0;
                true
            }
        });

        if matches!(self.trailer, Trailer::None)
            || (matches!(self.trailer, Trailer::CompoundStatement)
                && !matches!(
                    stmt.node,
                    StmtKind::FunctionDef { .. }
                        | StmtKind::AsyncFunctionDef { .. }
                        | StmtKind::ClassDef { .. }
                ))
        {
            // If this is the first statement in the block, remove any leading empty lines, with the
            // exception being functions and classes defined within compound statements (e.g., as
            // the first statement in an `if` body).
            let mut seen_non_empty = false;
            stmt.trivia.retain(|c| {
                if seen_non_empty {
                    true
                } else {
                    if c.kind.is_empty_line() && c.relationship.is_leading() {
                        false
                    } else {
                        seen_non_empty = true;
                        true
                    }
                }
            });
        } else {
            // If the previous statement was a function or similar, ensure we have the
            // appropriate number of lines to start.
            let required_newlines = match self.trailer {
                Trailer::FunctionDef | Trailer::ClassDef => self.depth.max_newlines(),
                Trailer::Docstring if matches!(self.scope, Scope::Class) => 1,
                Trailer::Import => usize::from(!matches!(
                    stmt.node,
                    StmtKind::Import { .. } | StmtKind::ImportFrom { .. }
                )),
                _ => 0,
            };
            let present_newlines = stmt
                .trivia
                .iter()
                .take_while(|c| c.kind.is_empty_line() && c.relationship.is_leading())
                .count();
            if present_newlines < required_newlines {
                for _ in 0..(required_newlines - present_newlines) {
                    stmt.trivia.insert(
                        0,
                        Trivia {
                            kind: TriviaKind::EmptyLine,
                            relationship: Relationship::Leading,
                        },
                    );
                }
            }

            // If the current statement is a function or similar, Ensure we have an
            // appropriate number of lines above.
            if matches!(
                stmt.node,
                StmtKind::FunctionDef { .. }
                    | StmtKind::AsyncFunctionDef { .. }
                    | StmtKind::ClassDef { .. }
            ) {
                let num_to_insert = self.depth.max_newlines()
                    - stmt
                        .trivia
                        .iter()
                        .take_while(|c| c.kind.is_empty_line() && c.relationship.is_leading())
                        .count();
                for _ in 0..num_to_insert {
                    stmt.trivia.insert(
                        0,
                        Trivia {
                            kind: TriviaKind::EmptyLine,
                            relationship: Relationship::Leading,
                        },
                    );
                }
            }
        }

        let prev_scope = self.scope;
        let prev_depth = self.depth;

        match &mut stmt.node {
            StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } => {
                self.depth = Depth::Nested;
                self.scope = Scope::Function;
                self.trailer = Trailer::None;
                self.visit_body(body);
                self.trailer = Trailer::FunctionDef;
            }
            StmtKind::ClassDef { body, .. } => {
                self.depth = Depth::Nested;
                self.scope = Scope::Class;
                self.trailer = Trailer::None;
                self.visit_body(body);
                self.trailer = Trailer::ClassDef;
            }
            StmtKind::While { body, orelse, .. }
            | StmtKind::For { body, orelse, .. }
            | StmtKind::AsyncFor { body, orelse, .. } => {
                self.depth = Depth::Nested;
                self.trailer = Trailer::CompoundStatement;
                self.visit_body(body);

                if !orelse.is_empty() {
                    // If the previous body ended with a function or class definition, we need to
                    // insert an empty line before the else block. Since the `else` itself isn't
                    // a statement, we need to insert it into the last statement of the body.
                    if matches!(self.trailer, Trailer::ClassDef | Trailer::FunctionDef) {
                        let stmt = body.last_mut().unwrap();
                        stmt.trivia.push(Trivia {
                            kind: TriviaKind::EmptyLine,
                            relationship: Relationship::Trailing,
                        });
                    }

                    self.depth = Depth::Nested;
                    self.trailer = Trailer::CompoundStatement;
                    self.visit_body(orelse);
                }
            }
            StmtKind::If { body, orelse, .. } => {
                self.depth = Depth::Nested;
                self.trailer = Trailer::CompoundStatement;
                self.visit_body(body);

                if !orelse.is_empty() {
                    if matches!(self.trailer, Trailer::ClassDef | Trailer::FunctionDef) {
                        let stmt = body.last_mut().unwrap();
                        stmt.trivia.push(Trivia {
                            kind: TriviaKind::EmptyLine,
                            relationship: Relationship::Trailing,
                        });
                    }

                    self.depth = Depth::Nested;
                    self.trailer = Trailer::CompoundStatement;
                    self.visit_body(orelse);
                }
            }
            StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => {
                self.depth = Depth::Nested;
                self.trailer = Trailer::CompoundStatement;
                self.visit_body(body);
            }
            // StmtKind::Match { .. } => {}
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            }
            | StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                self.depth = Depth::Nested;
                self.trailer = Trailer::CompoundStatement;
                self.visit_body(body);
                let mut last = body.last_mut();

                for handler in handlers {
                    if matches!(self.trailer, Trailer::ClassDef | Trailer::FunctionDef) {
                        if let Some(stmt) = last.as_mut() {
                            stmt.trivia.push(Trivia {
                                kind: TriviaKind::EmptyLine,
                                relationship: Relationship::Trailing,
                            });
                        }
                    }

                    self.depth = Depth::Nested;
                    self.trailer = Trailer::CompoundStatement;
                    let ExcepthandlerKind::ExceptHandler { body, .. } = &mut handler.node;
                    self.visit_body(body);
                    last = body.last_mut();
                }

                if !orelse.is_empty() {
                    if matches!(self.trailer, Trailer::ClassDef | Trailer::FunctionDef) {
                        if let Some(stmt) = last.as_mut() {
                            stmt.trivia.push(Trivia {
                                kind: TriviaKind::EmptyLine,
                                relationship: Relationship::Trailing,
                            });
                        }
                    }

                    self.depth = Depth::Nested;
                    self.trailer = Trailer::CompoundStatement;
                    self.visit_body(orelse);
                    last = body.last_mut();
                }

                if !finalbody.is_empty() {
                    if matches!(self.trailer, Trailer::ClassDef | Trailer::FunctionDef) {
                        if let Some(stmt) = last.as_mut() {
                            stmt.trivia.push(Trivia {
                                kind: TriviaKind::EmptyLine,
                                relationship: Relationship::Trailing,
                            });
                        }
                    }

                    self.depth = Depth::Nested;
                    self.trailer = Trailer::CompoundStatement;
                    self.visit_body(finalbody);
                }
            }
            _ => {
                self.trailer = match &stmt.node {
                    StmtKind::Expr { value, .. }
                        if matches!(self.scope, Scope::Class | Scope::Function)
                            && matches!(self.trailer, Trailer::None) =>
                    {
                        if let ExprKind::Constant {
                            value: Constant::Str(..),
                            ..
                        } = &value.node
                        {
                            Trailer::Docstring
                        } else {
                            Trailer::Generic
                        }
                    }
                    StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => Trailer::Import,
                    _ => Trailer::Generic,
                };
                visitor::walk_stmt(self, stmt);
            }
        }

        self.depth = prev_depth;
        self.scope = prev_scope;
    }

    fn visit_expr(&mut self, _expr: &'a mut Expr) {}
}

pub fn normalize_newlines(python_cst: &mut [Stmt]) {
    let mut normalizer = NewlineNormalizer {
        depth: Depth::TopLevel,
        trailer: Trailer::None,
        scope: Scope::Module,
    };
    for stmt in python_cst.iter_mut() {
        normalizer.visit_stmt(stmt);
    }
}
