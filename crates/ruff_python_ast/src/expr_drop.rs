//! Iterative `Drop` for the expression nodes the parser builds with *iterative*
//! loops: left-associative binary chains (`a + b + c + ...`) and postfix chains
//! (`f()()...`, `a[0][0]...`, `a.b.c...`). Unlike recursive-descent constructs,
//! these bypass the parser's recursion-depth guard, so their depth is bounded
//! only by the input length. Such a tree parses fine, but the derived drop glue
//! recurses once per level (~150 bytes of stack each) and overflows when it is
//! dropped — a denial-of-service vector for consumers parsing untrusted source
//! on a normally-sized stack.
//!
//! The fix gives the four node types forming these spines a manual `Drop` that
//! walks the recursive edge (`left`/`func`/`value`), detaching each node and
//! replacing the edge with a trivial placeholder *before* the node drops, so no
//! drop ever recurses and a spine of any depth is torn down in O(1) stack. A
//! single [`iter_expr_drop`] helper handles all four edges in one `match`, so
//! a chain that mixes kinds (e.g. `f()[0].a()...`) is still torn down in one
//! flat loop.

use crate::{Expr, ExprAttribute, ExprBinOp, ExprCall, ExprNoneLiteral, ExprSubscript};

/// Iteratively drop the chain of expressions reachable by following the spine
/// edge starting at `head` — the field along which the parser builds unbounded
/// left-associative and postfix chains.
///
/// `head` is left holding a [`placeholder`]; the caller's own field drop then
/// disposes of that placeholder.
fn iter_expr_drop(head: &mut Box<Expr>) {
    // Fast path: if the node along the spine edge isn't itself one of the
    // chainable kinds, there is no deep spine here, so the ordinary recursive
    // drop is shallow. This keeps the common case (e.g. dropping a plain
    // `a + b` or `f()`) free of any extra work.
    if !matches!(
        **head,
        Expr::BinOp(_) | Expr::Call(_) | Expr::Subscript(_) | Expr::Attribute(_)
    ) {
        return;
    }
    let mut node = std::mem::replace(&mut **head, placeholder());
    // Each iteration detaches the next node down the spine and then drops the
    // current one. The current node's own spine edge has already been replaced
    // with a placeholder, so its `Drop` runs without recursing.
    loop {
        let next = match &mut node {
            Expr::BinOp(inner) => std::mem::replace(&mut *inner.left, placeholder()),
            Expr::Call(inner) => std::mem::replace(&mut *inner.func, placeholder()),
            Expr::Subscript(inner) => std::mem::replace(&mut *inner.value, placeholder()),
            Expr::Attribute(inner) => std::mem::replace(&mut *inner.value, placeholder()),
            _ => break,
        };
        node = next;
    }
}

/// A trivial expression used to sever a recursive edge before the owning node
/// is dropped. It owns no children, so dropping it is a no-op.
fn placeholder() -> Expr {
    Expr::NoneLiteral(ExprNoneLiteral::default())
}

impl Drop for ExprBinOp {
    fn drop(&mut self) {
        iter_expr_drop(&mut self.left);
    }
}

impl Drop for ExprCall {
    fn drop(&mut self) {
        iter_expr_drop(&mut self.func);
    }
}

impl Drop for ExprSubscript {
    fn drop(&mut self) {
        iter_expr_drop(&mut self.value);
    }
}

impl Drop for ExprAttribute {
    fn drop(&mut self) {
        iter_expr_drop(&mut self.value);
    }
}
