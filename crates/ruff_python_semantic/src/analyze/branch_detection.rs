use std::cmp::Ordering;

use ruff_python_ast::types::RefEquality;
use rustpython_parser::ast::ExcepthandlerKind::ExceptHandler;
use rustpython_parser::ast::{Stmt, StmtKind};

use crate::node::Nodes;

/// Return the common ancestor of `left` and `right` below `stop`, or `None`.
fn common_ancestor<'a>(
    left: &'a Stmt,
    right: &'a Stmt,
    stop: Option<&'a Stmt>,
    node_tree: &Nodes<'a>,
) -> Option<&'a Stmt> {
    if stop.map_or(false, |stop| {
        RefEquality(left) == RefEquality(stop) || RefEquality(right) == RefEquality(stop)
    }) {
        return None;
    }

    if RefEquality(left) == RefEquality(right) {
        return Some(left);
    }

    let left_id = node_tree.node_id(left)?;
    let right_id = node_tree.node_id(right)?;

    let left_depth = node_tree.depth(left_id);
    let right_depth = node_tree.depth(right_id);

    match left_depth.cmp(&right_depth) {
        Ordering::Less => {
            let right_id = node_tree.parent_id(right_id)?;
            common_ancestor(left, node_tree[right_id], stop, node_tree)
        }
        Ordering::Equal => {
            let left_id = node_tree.parent_id(left_id)?;
            let right_id = node_tree.parent_id(right_id)?;
            common_ancestor(node_tree[left_id], node_tree[right_id], stop, node_tree)
        }
        Ordering::Greater => {
            let left_id = node_tree.parent_id(left_id)?;
            common_ancestor(node_tree[left_id], right, stop, node_tree)
        }
    }
}

/// Return the alternative branches for a given node.
fn alternatives(stmt: &Stmt) -> Vec<Vec<&Stmt>> {
    match &stmt.node {
        StmtKind::If { body, .. } => vec![body.iter().collect()],
        StmtKind::Try {
            body,
            handlers,
            orelse,
            ..
        }
        | StmtKind::TryStar {
            body,
            handlers,
            orelse,
            ..
        } => vec![body.iter().chain(orelse.iter()).collect()]
            .into_iter()
            .chain(handlers.iter().map(|handler| {
                let ExceptHandler { body, .. } = &handler.node;
                body.iter().collect()
            }))
            .collect(),
        StmtKind::Match { cases, .. } => cases
            .iter()
            .map(|case| case.body.iter().collect())
            .collect(),
        _ => vec![],
    }
}

/// Return `true` if `stmt` is a descendent of any of the nodes in `ancestors`.
fn descendant_of<'a>(
    stmt: &'a Stmt,
    ancestors: &[&'a Stmt],
    stop: &'a Stmt,
    node_tree: &Nodes<'a>,
) -> bool {
    ancestors
        .iter()
        .any(|ancestor| common_ancestor(stmt, ancestor, Some(stop), node_tree).is_some())
}

/// Return `true` if `left` and `right` are on different branches of an `if` or
/// `try` statement.
pub fn different_forks<'a>(left: &'a Stmt, right: &'a Stmt, node_tree: &Nodes<'a>) -> bool {
    if let Some(ancestor) = common_ancestor(left, right, None, node_tree) {
        for items in alternatives(ancestor) {
            let l = descendant_of(left, &items, ancestor, node_tree);
            let r = descendant_of(right, &items, ancestor, node_tree);
            if l ^ r {
                return true;
            }
        }
    }
    false
}
