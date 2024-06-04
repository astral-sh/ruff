use ruff_python_ast::{Expr, ExprStringLiteral};
use ruff_python_semantic::{ScopeId, Snapshot};

/// A collection of AST nodes that are deferred for later visitation. Used to, e.g., store
/// functions, whose bodies shouldn't be visited until all module-level definitions have been
/// visited.
#[derive(Debug, Default)]
pub(crate) struct Visit<'a> {
    pub(crate) string_type_definitions: Vec<(&'a ExprStringLiteral, Snapshot)>,
    pub(crate) future_type_definitions: Vec<(&'a Expr, Snapshot)>,
    pub(crate) type_param_definitions: Vec<(&'a Expr, Snapshot)>,
    pub(crate) functions: Vec<Snapshot>,
    pub(crate) lambdas: Vec<Snapshot>,
    /// N.B. This field should always be empty unless it's a stub file
    pub(crate) class_bases: Vec<(&'a Expr, Snapshot)>,
}

impl Visit<'_> {
    /// Returns `true` if there are no deferred nodes.
    pub(crate) fn is_empty(&self) -> bool {
        self.string_type_definitions.is_empty()
            && self.future_type_definitions.is_empty()
            && self.type_param_definitions.is_empty()
            && self.functions.is_empty()
            && self.lambdas.is_empty()
            && self.class_bases.is_empty()
    }
}

/// A collection of AST nodes to be analyzed after the AST traversal. Used to, e.g., store
/// all `for` loops, so that they can be analyzed after the entire AST has been visited.
#[derive(Debug, Default)]
pub(crate) struct Analyze {
    pub(crate) scopes: Vec<ScopeId>,
    pub(crate) lambdas: Vec<Snapshot>,
    pub(crate) for_loops: Vec<Snapshot>,
}
