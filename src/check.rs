use std::path::Path;

use rustpython_parser::ast::{ExprKind, Stmt, StmtKind, Suite};

use crate::message::Message;

fn check_statement(path: &Path, stmt: &Stmt) -> Vec<Message> {
    let mut messages: Vec<Message> = vec![];
    match &stmt.node {
        StmtKind::FunctionDef { body, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::AsyncFunctionDef { body, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::ClassDef { body, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::Return { .. } => {}
        StmtKind::Delete { .. } => {}
        StmtKind::Assign { .. } => {}
        StmtKind::AugAssign { .. } => {}
        StmtKind::AnnAssign { .. } => {}
        StmtKind::For { body, orelse, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
            messages.extend(orelse.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::AsyncFor { body, orelse, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
            messages.extend(orelse.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::While { body, orelse, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
            messages.extend(orelse.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::If { test, body, orelse } => {
            if let ExprKind::Tuple { .. } = test.node {
                messages.push(Message::IfTuple {
                    filename: path.to_path_buf(),
                    location: stmt.location,
                });
            }
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
            messages.extend(orelse.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::With { body, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::AsyncWith { body, .. } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
        }
        StmtKind::Raise { .. } => {}
        StmtKind::Try {
            body,
            orelse,
            finalbody,
            ..
        } => {
            messages.extend(body.iter().flat_map(|stmt| check_statement(path, stmt)));
            messages.extend(orelse.iter().flat_map(|stmt| check_statement(path, stmt)));
            messages.extend(
                finalbody
                    .iter()
                    .flat_map(|stmt| check_statement(path, stmt)),
            );
        }
        StmtKind::Assert { .. } => {}
        StmtKind::Import { .. } => {}
        StmtKind::ImportFrom { names, .. } => {
            for alias in names {
                if alias.name == "*" {
                    messages.push(Message::ImportStarUsage {
                        filename: path.to_path_buf(),
                        location: stmt.location,
                    });
                }
            }
        }
        StmtKind::Global { .. } => {}
        StmtKind::Nonlocal { .. } => {}
        StmtKind::Expr { .. } => {}
        StmtKind::Pass => {}
        StmtKind::Break => {}
        StmtKind::Continue => {}
    }
    messages
}

pub fn check_ast(path: &Path, python_ast: &Suite) -> Vec<Message> {
    python_ast
        .iter()
        .flat_map(|stmt| check_statement(path, stmt))
        .collect()
}
