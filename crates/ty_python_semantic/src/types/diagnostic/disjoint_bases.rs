//! Prune redundant layout constraints without changing the original bases during comparison.

use std::convert::Infallible;

use super::{IncompatibleBaseInfo, IncompatibleBases};
use crate::Db;
use crate::types::class::DisjointBase;

#[cfg(test)]
mod tests;

pub(super) struct OrdinaryDisjointBaseEffects<'db> {
    pub(super) db: &'db dyn Db,
}

pub(in crate::types) struct DisjointBaseFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDisjointBaseEffects)]
    pub(in crate::types) trait DisjointBaseEffects<'db> {
        type Error;

        #[operation(local)]
        async fn empty_retained_bases(&self) -> Result<IncompatibleBases<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_disjoint_base(&self, bases: &IncompatibleBases<'db>, cursor: &mut usize) -> Result<Option<(DisjointBase<'db>, IncompatibleBaseInfo<'db>)>, Self::Error>;
        #[operation(child)]
        async fn is_layout_subtype(&self, base: DisjointBase<'db>, other: DisjointBase<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn retain_disjoint_base(&self, retained: &mut IncompatibleBases<'db>, base: DisjointBase<'db>, info: IncompatibleBaseInfo<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn replace_disjoint_bases(&self, bases: &mut IncompatibleBases<'db>, retained: &mut IncompatibleBases<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl DisjointBaseFacts {
        fn same_base<'db>(&self, base: DisjointBase<'db>, other: DisjointBase<'db>) -> bool {
            base == other
        }
    }

    #[synchronous(prune_disjoint_bases_sync)]
    #[capabilities(effects = DisjointBaseEffects, facts = DisjointBaseFacts)]
    #[passive_values()]
    pub(in crate::types) async fn prune_disjoint_bases_with<'db, E: DisjointBaseEffects<'db>>(
        bases: &mut IncompatibleBases<'db>,
        facts: DisjointBaseFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let mut retained = effects.empty_retained_bases().await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(entry) = effects.next_disjoint_base(bases, &mut cursor).await? {
            let (base, info) = entry;
            #[passive_state]
            let mut redundant = false;
            let mut other_cursor = 0;
            #[cursor_loop]
            while let Some(other_entry) = effects.next_disjoint_base(bases, &mut other_cursor).await? {
                let (other, _) = other_entry;
                if !facts.same_base(base, other) && effects.is_layout_subtype(base, other).await? {
                    redundant = true;
                    break;
                }
            }
            if !redundant {
                effects.retain_disjoint_base(&mut retained, base, info).await?;
            }
        }
        effects.replace_disjoint_bases(bases, &mut retained).await
    }
}

pub(in crate::types) fn next_disjoint_base_entry<'db>(
    bases: &IncompatibleBases<'db>,
    cursor: &mut usize,
) -> Option<(DisjointBase<'db>, IncompatibleBaseInfo<'db>)> {
    let (base, info) = bases.0.get_index(*cursor)?;
    *cursor += 1;
    Some((*base, *info))
}

impl<'db> SynchronousDisjointBaseEffects<'db> for OrdinaryDisjointBaseEffects<'db> {
    type Error = Infallible;

    fn empty_retained_bases(&self) -> Result<IncompatibleBases<'db>, Infallible> {
        Ok(IncompatibleBases::default())
    }

    fn next_disjoint_base(
        &self,
        bases: &IncompatibleBases<'db>,
        cursor: &mut usize,
    ) -> Result<Option<(DisjointBase<'db>, IncompatibleBaseInfo<'db>)>, Infallible> {
        Ok(next_disjoint_base_entry(bases, cursor))
    }

    fn is_layout_subtype(
        &self,
        base: DisjointBase<'db>,
        other: DisjointBase<'db>,
    ) -> Result<bool, Infallible> {
        // CPython's layout check operates on runtime classes. Type arguments are irrelevant
        // here: a generic disjoint base and any specialization share the same layout.
        Ok(base
            .class
            .default_specialization(self.db)
            .is_subtype_of_class_literal(self.db, other.class))
    }

    fn retain_disjoint_base(
        &self,
        retained: &mut IncompatibleBases<'db>,
        base: DisjointBase<'db>,
        info: IncompatibleBaseInfo<'db>,
    ) -> Result<(), Infallible> {
        retained.0.insert(base, info);
        Ok(())
    }

    fn replace_disjoint_bases(
        &self,
        bases: &mut IncompatibleBases<'db>,
        retained: &mut IncompatibleBases<'db>,
    ) -> Result<(), Infallible> {
        std::mem::swap(bases, retained);
        Ok(())
    }
}
