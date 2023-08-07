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
    // Fast path: if the nodes are the same, they are their own common ancestor.
    if left == right {
        return Some(left);
    }

    // Grab all the ancestors of `right`.
    let candidates = node_tree.ancestor_ids(right).collect::<Vec<_>>();

    // Find the first ancestor of `left` that is also an ancestor of `right`.
    node_tree
        .ancestor_ids(left)
        .take_while(|id| stop != Some(*id))
        .find(|id| candidates.contains(id))
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
        })
        | Stmt::TryStar(ast::StmtTryStar {
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
