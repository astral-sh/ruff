//! Extract docstrings from an AST.

use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::docstrings::definition::{Definition, DefinitionKind, Documentable};
use crate::visibility::{Modifier, VisibleScope};

/// Extract a docstring from a function or class body.
pub fn docstring_from(suite: &[Stmt]) -> Option<&Expr> {
    let stmt = suite.first()?;
    let StmtKind::Expr { value } = &stmt.node else {
        return None;
    };
    if !matches!(
        &value.node,
        ExprKind::Constant {
            value: Constant::Str(_),
            ..
        }
    ) {
        return None;
    }
    Some(value)
}

/// Extract a `Definition` from the AST node defined by a `Stmt`.
pub fn extract<'a>(
    scope: &VisibleScope,
    stmt: &'a Stmt,
    body: &'a [Stmt],
    kind: &Documentable,
) -> Definition<'a> {
    let expr = docstring_from(body);
    match kind {
        Documentable::Function => match scope {
            VisibleScope {
                modifier: Modifier::Module,
                ..
            } => Definition {
                kind: DefinitionKind::Function(stmt),
                docstring: expr,
            },
            VisibleScope {
                modifier: Modifier::Class,
                ..
            } => Definition {
                kind: DefinitionKind::Method(stmt),
                docstring: expr,
            },
            VisibleScope {
                modifier: Modifier::Function,
                ..
            } => Definition {
                kind: DefinitionKind::NestedFunction(stmt),
                docstring: expr,
            },
        },
        Documentable::Class => match scope {
            VisibleScope {
                modifier: Modifier::Module,
                ..
            } => Definition {
                kind: DefinitionKind::Class(stmt),
                docstring: expr,
            },
            VisibleScope {
                modifier: Modifier::Class,
                ..
            } => Definition {
                kind: DefinitionKind::NestedClass(stmt),
                docstring: expr,
            },
            VisibleScope {
                modifier: Modifier::Function,
                ..
            } => Definition {
                kind: DefinitionKind::NestedClass(stmt),
                docstring: expr,
            },
        },
    }
}
