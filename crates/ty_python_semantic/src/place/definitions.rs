use smallvec::SmallVec;
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::{Definition, DefinitionState};

use crate::Db;
use crate::reachability::ReachabilityConstraintsExtension;

/// A set of definitions found by name resolution.
pub(crate) struct DefinitionResolution<'db> {
    definitions: SmallVec<[Definition<'db>; 2]>,
}

impl<'db> DefinitionResolution<'db> {
    /// Returns the definitions found by name resolution.
    pub(crate) fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Resolves the reachable definitions supplied by the given bindings.
    pub(crate) fn from_bindings(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut definitions = SmallVec::new();

        while let Some(binding) = bindings.next() {
            let reachability = bindings.reachability_constraints().evaluate(
                db,
                bindings.predicates(),
                binding.reachability_constraint,
            );
            if reachability.is_always_false() {
                continue;
            }

            if let DefinitionState::Defined(definition) = binding.binding
                && !definitions.contains(&definition)
            {
                definitions.push(definition);
            }
        }

        Self { definitions }
    }
}
