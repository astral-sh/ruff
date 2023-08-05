use ruff_python_ast::{Expr, TypeParam};
use ruff_python_semantic::{ScopeId, Snapshot};
use ruff_text_size::TextRange;

/// A collection of AST nodes that are deferred for later analysis.
/// Used to, e.g., store functions, whose bodies shouldn't be analyzed until all
/// module-level definitions have been analyzed.
#[derive(Debug, Default)]
pub(crate) struct Deferred<'a> {
    pub(crate) scopes: Vec<ScopeId>,
    pub(crate) string_type_definitions: Vec<(TextRange, &'a str, Snapshot)>,
    pub(crate) future_type_definitions: Vec<(&'a Expr, Snapshot)>,
    pub(crate) type_param_definitions: Vec<(&'a TypeParam, Snapshot)>,
    pub(crate) functions: Vec<Snapshot>,
    pub(crate) lambdas: Vec<(&'a Expr, Snapshot)>,
    pub(crate) for_loops: Vec<Snapshot>,
}
