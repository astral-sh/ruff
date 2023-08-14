use std::cmp::Ordering;
use std::iter;

use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

use crate::statements::{StatementId, Statements};

/// Return the common ancestor of `left` and `right` below `stop`, or `None`.
fn common_ancestor(
    left: StatementId,
    right: StatementId,
    stop: Option<StatementId>,
    node_tree: &Statements,
) -> Option<StatementId> {
    if stop.is_some_and(|stop| left == stop || right == stop) {
        return None;
    }

    if left == right {
        return Some(left);
    }

    let left_depth = node_tree.depth(left);
    let right_depth = node_tree.depth(right);

    match left_depth.cmp(&right_depth) {
        Ordering::Less => {
            let right = node_tree.parent_id(right)?;
            common_ancestor(left, right, stop, node_tree)
        }
        Ordering::Equal => {
            let left = node_tree.parent_id(left)?;
            let right = node_tree.parent_id(right)?;
            common_ancestor(left, right, stop, node_tree)
        }
        Ordering::Greater => {
            let left = node_tree.parent_id(left)?;
            common_ancestor(left, right, stop, node_tree)
        }
    }
}

/// Return the alternative branches for a given node.
fn alternatives(stmt: &Stmt) -> Vec<Vec<&Stmt>> {
    match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => iter::once(body.iter().collect())
            .chain(
                elif_else_clauses
                    .iter()
                    .map(|clause| clause.body.iter().collect()),
            )
            .collect(),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            ..
        }) => vec![body.iter().chain(orelse.iter()).collect()]
            .into_iter()
            .chain(handlers.iter().map(|handler| {
                let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) =
                    handler;
                body.iter().collect()
            }))
            .collect(),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .map(|case| case.body.iter().collect())
            .collect(),
        _ => vec![],
    }
}

/// Return `true` if `stmt` is a descendent of any of the nodes in `ancestors`.
fn descendant_of<'a>(
    stmt: StatementId,
    ancestors: &[&'a Stmt],
    stop: StatementId,
    node_tree: &Statements<'a>,
) -> bool {
    ancestors.iter().any(|ancestor| {
        node_tree.statement_id(ancestor).is_some_and(|ancestor| {
            common_ancestor(stmt, ancestor, Some(stop), node_tree).is_some()
        })
    })
}

/// Return `true` if `left` and `right` are on different branches of an `if` or
/// `try` statement.
pub fn different_forks(left: StatementId, right: StatementId, node_tree: &Statements) -> bool {
    if let Some(ancestor) = common_ancestor(left, right, None, node_tree) {
        for items in alternatives(node_tree[ancestor]) {
            let l = descendant_of(left, &items, ancestor, node_tree);
            let r = descendant_of(right, &items, ancestor, node_tree);
            if l ^ r {
                return true;
            }
        }
    }
    false
}
