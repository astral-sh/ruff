use std::hash::Hash;

use rustpython_parser::ast::Expr;

use crate::ast::comparable::ComparableExpr;

/// Wrapper around `Expr` that implements `Hash` and `PartialEq`.
pub struct HashableExpr<'a>(&'a Expr);

impl Hash for HashableExpr<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let comparable = ComparableExpr::from(self.0);
        comparable.hash(state);
    }
}

impl PartialEq<Self> for HashableExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        let comparable = ComparableExpr::from(self.0);
        comparable == ComparableExpr::from(other.0)
    }
}

impl Eq for HashableExpr<'_> {}

impl<'a> From<&'a Expr> for HashableExpr<'a> {
    fn from(expr: &'a Expr) -> Self {
        Self(expr)
    }
}

impl<'a> HashableExpr<'a> {
    pub(crate) const fn from_expr(expr: &'a Expr) -> Self {
        Self(expr)
    }

    pub(crate) const fn as_expr(&self) -> &'a Expr {
        self.0
    }
}
