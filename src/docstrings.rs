use crate::check_ast::Checker;
use rustpython_ast::{Expr, Stmt, StmtKind};

#[derive(Debug)]
pub enum DocstringKind {
    Module,
    Function,
    Class,
}

#[derive(Debug)]
pub struct Docstring<'a> {
    pub kind: DocstringKind,
    pub parent: Option<&'a Stmt>,
    pub node: &'a Expr,
}

/// Extract a docstring from an expression.
pub fn extract<'a, 'b>(
    checker: &'a Checker,
    stmt: &'b Stmt,
    expr: &'b Expr,
) -> Option<Docstring<'b>> {
    let defined_in = checker
        .binding_context()
        .defined_in
        .map(|index| checker.parents[index]);

    match defined_in {
        None => {
            if checker.initial {
                return Some(Docstring {
                    kind: DocstringKind::Module,
                    parent: None,
                    node: expr,
                });
            }
        }
        Some(parent) => {
            if let StmtKind::FunctionDef { body, .. }
            | StmtKind::AsyncFunctionDef { body, .. }
            | StmtKind::ClassDef { body, .. } = &parent.node
            {
                if body.first().map(|node| node == stmt).unwrap_or_default() {
                    return Some(Docstring {
                        kind: if matches!(&parent.node, StmtKind::ClassDef { .. }) {
                            DocstringKind::Class
                        } else {
                            DocstringKind::Function
                        },
                        parent: None,
                        node: expr,
                    });
                }
            }
        }
    }

    None
}
