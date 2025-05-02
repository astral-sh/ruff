//! When building a semantic model, we often need to know which names in a given scope are declared
//! as `global`. This module provides data structures for storing and querying the set of `global`
//! names in a given scope.

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

/// The set of global names for a given scope, represented as a map from the name of the global to
/// the range of the declaration in the source code.
#[derive(Debug, salsa::Update, Default)]
pub struct Globals(FxHashSet<Name>);

impl Globals {
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    pub(crate) fn insert(&mut self, name: Name) {
        self.0.insert(name);
    }
}
