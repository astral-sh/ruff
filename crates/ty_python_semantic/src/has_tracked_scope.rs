use ruff_python_ast::{self as ast, HasNodeIndex};

/// Implemented by types for which the semantic index tracks their scope.
pub trait HasTrackedScope: HasNodeIndex {}

impl HasTrackedScope for ast::Expr {}

impl HasTrackedScope for ast::ExprRef<'_> {}
impl HasTrackedScope for &ast::ExprRef<'_> {}

// We never explicitly register the scope of an `Identifier`.
// However, `ExpressionsScopeMap` stores the text ranges of each scope.
// That allows us to look up the identifier's scope for as long as it's
// inside an expression (because the ranges overlap).
impl HasTrackedScope for ast::Identifier {}
