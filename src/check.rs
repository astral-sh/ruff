use std::path::Path;

use rustpython_parser::ast::{Located, StmtKind, Suite};

use crate::message::Message;

pub fn check_ast(path: &Path, python_ast: &Suite) -> Vec<Message> {
    let mut messages: Vec<Message> = vec![];
    for statement in python_ast {
        let Located {
            location,
            custom: _,
            node,
        } = statement;
        match node {
            StmtKind::FunctionDef { .. } => {}
            StmtKind::AsyncFunctionDef { .. } => {}
            StmtKind::ClassDef { .. } => {}
            StmtKind::Return { .. } => {}
            StmtKind::Delete { .. } => {}
            StmtKind::Assign { .. } => {}
            StmtKind::AugAssign { .. } => {}
            StmtKind::AnnAssign { .. } => {}
            StmtKind::For { .. } => {}
            StmtKind::AsyncFor { .. } => {}
            StmtKind::While { .. } => {}
            StmtKind::If { .. } => {}
            StmtKind::With { .. } => {}
            StmtKind::AsyncWith { .. } => {}
            StmtKind::Raise { .. } => {}
            StmtKind::Try { .. } => {}
            StmtKind::Assert { .. } => {}
            StmtKind::Import { .. } => {}
            StmtKind::ImportFrom {
                level: _,
                module: _,
                names,
            } => {
                for alias in names {
                    if alias.name == "*" {
                        messages.push(Message::ImportStarUsage {
                            filename: path.to_path_buf(),
                            location: *location,
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
    }
    messages
}
