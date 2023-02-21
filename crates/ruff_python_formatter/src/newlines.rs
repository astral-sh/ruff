use rustpython_parser::ast::Constant;

use crate::core::visitor;
use crate::core::visitor::Visitor;
use crate::cst::{Expr, ExprKind, Stmt, StmtKind};
use crate::trivia::{Relationship, Trivia, TriviaKind};

#[derive(Copy, Clone)]
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

#[derive(Copy, Clone)]
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
            if matches!(
                (c.kind, c.relationship),
                (TriviaKind::EmptyLine, Relationship::Leading)
            ) {
                count += 1;
                count <= self.depth.max_newlines()
            } else {
                count = 0;
                true
            }
        });

        if matches!(self.trailer, Trailer::None) {
            // If this is the first statement in the block, remove any leading empty lines.
            // TODO(charlie): If we have a function or class definition within a non-scoped block,
            // like an if-statement, retain a line before and after.
            let mut seen_non_empty = false;
            stmt.trivia.retain(|c| {
                if seen_non_empty {
                    true
                } else {
                    if matches!(
                        (c.kind, c.relationship),
                        (TriviaKind::EmptyLine, Relationship::Leading)
                    ) {
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
                .take_while(|c| {
                    matches!(
                        (c.kind, c.relationship),
                        (TriviaKind::EmptyLine, Relationship::Leading)
                    )
                })
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
                        .take_while(|c| {
                            matches!(
                                (c.kind, c.relationship),
                                (TriviaKind::EmptyLine, Relationship::Leading)
                            )
                        })
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

        self.trailer = match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                Trailer::FunctionDef
            }
            // TODO(charlie): This needs to be the first statement in a class or function.
            StmtKind::Expr { value, .. } => {
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
            StmtKind::ClassDef { .. } => Trailer::ClassDef,
            StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => Trailer::Import,
            _ => Trailer::Generic,
        };

        let prev_scope = self.scope;
        self.scope = match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => Scope::Function,
            StmtKind::ClassDef { .. } => Scope::Class,
            _ => prev_scope,
        };

        visitor::walk_stmt(self, stmt);

        self.scope = prev_scope;
    }

    fn visit_expr(&mut self, expr: &'a mut Expr) {
        expr.trivia
            .retain(|c| !matches!(c.kind, TriviaKind::EmptyLine));
        visitor::walk_expr(self, expr);
    }

    fn visit_body(&mut self, body: &'a mut [Stmt]) {
        let prev_depth = self.depth;
        let prev_trailer = self.trailer;

        self.depth = Depth::Nested;
        self.trailer = Trailer::None;

        visitor::walk_body(self, body);

        self.trailer = prev_trailer;
        self.depth = prev_depth;
    }
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
