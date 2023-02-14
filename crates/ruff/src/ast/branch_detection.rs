use std::cmp::Ordering;

use rustc_hash::FxHashMap;
use rustpython_parser::ast::ExcepthandlerKind::ExceptHandler;
use rustpython_parser::ast::{Stmt, StmtKind};

use crate::ast::types::RefEquality;

/// Return the common ancestor of `left` and `right` below `stop`, or `None`.
fn common_ancestor<'a>(
    left: &'a RefEquality<'a, Stmt>,
    right: &'a RefEquality<'a, Stmt>,
    stop: Option<&'a RefEquality<'a, Stmt>>,
    depths: &'a FxHashMap<RefEquality<'a, Stmt>, usize>,
    child_to_parent: &'a FxHashMap<RefEquality<'a, Stmt>, RefEquality<'a, Stmt>>,
) -> Option<&'a RefEquality<'a, Stmt>> {
    if let Some(stop) = stop {
        if left == stop || right == stop {
            return None;
        }
    }
    if left == right {
        return Some(left);
    }

    let left_depth = depths.get(left)?;
    let right_depth = depths.get(right)?;
    match left_depth.cmp(right_depth) {
        Ordering::Less => common_ancestor(
            left,
            child_to_parent.get(right)?,
            stop,
            depths,
            child_to_parent,
        ),
        Ordering::Equal => common_ancestor(
            child_to_parent.get(left)?,
            child_to_parent.get(right)?,
            stop,
            depths,
            child_to_parent,
        ),
        Ordering::Greater => common_ancestor(
            child_to_parent.get(left)?,
            right,
            stop,
            depths,
            child_to_parent,
        ),
    }
}

/// Return the alternative branches for a given node.
fn alternatives<'a>(stmt: &'a RefEquality<'a, Stmt>) -> Vec<Vec<RefEquality<'a, Stmt>>> {
    match &stmt.node {
        StmtKind::If { body, .. } => vec![body.iter().map(RefEquality).collect()],
        StmtKind::Try {
            body,
            handlers,
            orelse,
            ..
        } => vec![body.iter().chain(orelse.iter()).map(RefEquality).collect()]
            .into_iter()
            .chain(handlers.iter().map(|handler| {
                let ExceptHandler { body, .. } = &handler.node;
                body.iter().map(RefEquality).collect()
            }))
            .collect(),
        _ => vec![],
    }
}

/// Return `true` if `stmt` is a descendent of any of the nodes in `ancestors`.
fn descendant_of<'a>(
    stmt: &RefEquality<'a, Stmt>,
    ancestors: &[RefEquality<'a, Stmt>],
    stop: &RefEquality<'a, Stmt>,
    depths: &FxHashMap<RefEquality<'a, Stmt>, usize>,
    child_to_parent: &FxHashMap<RefEquality<'a, Stmt>, RefEquality<'a, Stmt>>,
) -> bool {
    ancestors.iter().any(|ancestor| {
        common_ancestor(stmt, ancestor, Some(stop), depths, child_to_parent).is_some()
    })
}

/// Return `true` if `left` and `right` are on different branches of an `if` or
/// `try` statement.
pub fn different_forks<'a>(
    left: &RefEquality<'a, Stmt>,
    right: &RefEquality<'a, Stmt>,
    depths: &FxHashMap<RefEquality<'a, Stmt>, usize>,
    child_to_parent: &FxHashMap<RefEquality<'a, Stmt>, RefEquality<'a, Stmt>>,
) -> bool {
    if let Some(ancestor) = common_ancestor(left, right, None, depths, child_to_parent) {
        for items in alternatives(ancestor) {
            let l = descendant_of(left, &items, ancestor, depths, child_to_parent);
            let r = descendant_of(right, &items, ancestor, depths, child_to_parent);
            if l ^ r {
                return true;
            }
        }
    }
    false
}
