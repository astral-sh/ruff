//! Stable owners for constraints created by invocations during a shared execution.

use std::cell::OnceCell;

use typed_arena::Arena;

use super::TaskEndpoint;
use crate::ProgramEnvironment;
use crate::types::constraints::ConstraintSetBuilder;
pub(in crate::types) use crate::types::relation::resources::RelationOwners;

#[derive(Default)]
pub(in crate::types) struct CallBuilders<'db> {
    builders: OnceCell<Arena<ConstraintSetBuilder<'db>>>,
}

impl<'db> CallBuilders<'db> {
    #[cfg(test)]
    pub(super) fn is_initialized(&self) -> bool {
        self.builders.get().is_some()
    }

    pub(in crate::types) fn allocate<E>(
        &self,
        endpoint: &TaskEndpoint<'_, E>,
    ) -> Result<&ConstraintSetBuilder<'db>, E> {
        endpoint.admit_resource::<ConstraintSetBuilder<'db>>()?;
        Ok(self
            .builders
            .get_or_init(|| Arena::with_capacity(1))
            .alloc(ConstraintSetBuilder::new()))
    }
}

#[derive(Default)]
pub(in crate::types) struct CallRelationOwners<'env, 'c, 'db> {
    owners: OnceCell<Arena<RelationOwners<'env, 'c, 'db>>>,
}

impl<'env, 'c, 'db> CallRelationOwners<'env, 'c, 'db> {
    pub(in crate::types) fn allocate<E>(
        &self,
        endpoint: &TaskEndpoint<'_, E>,
        env: &'env ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> Result<&RelationOwners<'env, 'c, 'db>, E> {
        endpoint.admit_resource::<RelationOwners<'env, 'c, 'db>>()?;
        Ok(self
            .owners
            .get_or_init(|| Arena::with_capacity(1))
            .alloc(RelationOwners::new(env, constraints)))
    }
}
