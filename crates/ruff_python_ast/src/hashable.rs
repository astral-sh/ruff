use std::hash::Hash;

use crate::Expr;

use crate::comparable::ComparableExpr;

/// Wrapper around `Expr` that implements `Hash` and `PartialEq`.
pub struct HashableExpr<'a, 'ast>(&'a Expr<'ast>);

impl Hash for HashableExpr<'_, '_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let comparable = ComparableExpr::from(self.0);
        comparable.hash(state);
    }
}

impl PartialEq<Self> for HashableExpr<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        let comparable = ComparableExpr::from(self.0);
        comparable == ComparableExpr::from(other.0)
    }
}

impl Eq for HashableExpr<'_, '_> {}

impl<'a, 'ast> From<&'a Expr<'ast>> for HashableExpr<'a, 'ast> {
    fn from(expr: &'a Expr<'ast>) -> Self {
        Self(expr)
    }
}

impl<'a, 'ast> HashableExpr<'a, 'ast> {
    pub const fn from_expr(expr: &'a Expr<'ast>) -> Self {
        Self(expr)
    }

    pub const fn as_expr(&self) -> &'a Expr<'ast> {
        self.0
    }
}
