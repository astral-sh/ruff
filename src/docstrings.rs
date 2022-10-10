use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

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
    pub expr: &'a Expr,
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
                    expr,
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
                        expr,
                    });
                }
            }
        }
    }

    None
}

pub fn docstring_empty(checker: &mut Checker, docstring: &Docstring) {
    // Extract the source.
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if string.trim().is_empty() {
            checker.add_check(Check::new(
                CheckKind::EmptyDocstring,
                Range::from_located(docstring.expr),
            ));
        }
    }
}
