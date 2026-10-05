//! Reconcile metaclasses using known subclass relationships and explicit gradual bases.

use std::convert::Infallible;

use crate::types::class_base::ClassBase;
use crate::types::mro::MroIterator;
use crate::types::relation::TypeRelation;
use crate::types::{ClassLiteral, ClassType, SubclassOfType, Type};
use crate::{Db, FxIndexSet, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies class relations and ordered ancestry traversal for metaclass reconciliation.
    #[synchronous(SynchronousReconciliationEffects)]
    pub(in crate::types) trait ReconciliationEffects<'db> {
        type Error;
        type MroCursor<'state> where Self: 'state;
        type Ancestors<'state> where Self: 'state;
        type Bases<'state> where Self: 'state;

        /// Accounts for the work and bytes needed to copy fixed-size operands and results.
        /// Implementations with resource limits return an error before reconciliation starts
        /// when either budget is insufficient.
        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;

        /// Tests whether `source` is a subclass of `target`, using a fresh relation checker.
        #[operation(child)]
        async fn is_subclass(&self, env: &ProgramEnvironment<'db>, source: ClassType<'db>, target: ClassType<'db>) -> Result<bool, Self::Error>;

        /// Tests whether gradual ancestry can supply an otherwise unproven subclass relationship.
        #[operation(child)]
        async fn could_inherit(&self, env: &ProgramEnvironment<'db>, source: ClassType<'db>, target: ClassType<'db>) -> Result<bool, Self::Error>;

        /// Tests whether `source` is assignable to `target`, using a fresh relation checker.
        #[operation(child)]
        async fn is_assignable(&self, env: &ProgramEnvironment<'db>, source: ClassType<'db>, target: ClassType<'db>) -> Result<bool, Self::Error>;

        /// Tests whether a class forbids subclasses.
        #[operation(child)]
        async fn is_final(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;

        /// Tests whether a class's MRO uses inheritance-cycle recovery.
        #[operation(child)]
        async fn has_cyclic_mro(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;

        /// Starts lazy traversal of a class's MRO.
        #[operation(child)]
        async fn mro_start(&self, class: ClassType<'db>) -> Result<Self::MroCursor<'_>, Self::Error>;

        /// Advances an MRO cursor by one base.
        #[operation(child)]
        #[progress]
        async fn mro_next<'state>(&'state self, cursor: &mut Self::MroCursor<'state>) -> Result<Option<ClassBase<'db>>, Self::Error>;

        /// Returns the underlying class literal, ignoring generic specialization.
        #[operation(source)]
        async fn literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;

        /// Creates an empty set of target-ancestor class literals.
        #[operation(local)]
        async fn ancestors_start(&self) -> Result<Self::Ancestors<'_>, Self::Error>;

        /// Inserts one target ancestor after checking any work and byte limits.
        /// A failed budget check leaves the caller's set unchanged.
        #[operation(local)]
        async fn ancestors_insert<'state>(&'state self, ancestors: &mut Self::Ancestors<'state>, class: ClassLiteral<'db>) -> Result<(), Self::Error>;

        /// Tests whether a source ancestor is shared with the target.
        #[operation(local)]
        async fn ancestors_contains<'state>(&'state self, ancestors: &Self::Ancestors<'state>, class: ClassLiteral<'db>) -> Result<bool, Self::Error>;

        /// Starts ordered traversal of a class literal's explicit base types.
        #[operation(child)]
        async fn bases_start(&self, class: ClassLiteral<'db>) -> Result<Self::Bases<'_>, Self::Error>;

        /// Advances an explicit-base cursor by one type.
        #[operation(local)]
        #[progress]
        async fn bases_next<'state>(&'state self, bases: &mut Self::Bases<'state>) -> Result<Option<Type<'db>>, Self::Error>;

        /// Converts an explicit base type in the context of the class that declared it.
        #[operation(child)]
        async fn resolve_base(&self, env: &ProgramEnvironment<'db>, class: ClassLiteral<'db>, base: Type<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
    }

    /// Selects the more derived metaclass, returning `type[Unknown]` for gradual ambiguity
    /// or `None` for a conflict.
    #[synchronous(most_derived_metaclass_sync)]
    #[capabilities(effects = ReconciliationEffects)]
    #[passive_values(Type::from, SubclassOfType::subclass_of_unknown)]
    pub(in crate::types) async fn most_derived_metaclass_with<'db, E: ReconciliationEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        if effects.is_subclass(env, source, target).await? {
            return Ok(Some(Type::from(source)));
        }
        if effects.is_subclass(env, target, source).await? {
            return Ok(Some(Type::from(target)));
        }

        let source_could_inherit = effects.could_inherit(env, source, target).await?;
        let target_could_inherit = effects.could_inherit(env, target, source).await?;
        Ok(match (source_could_inherit, target_could_inherit) {
            (true, false) => Some(Type::from(source)),
            (false, true) => Some(Type::from(target)),
            (true, true) => Some(SubclassOfType::subclass_of_unknown()),
            (false, false) => None,
        })
    }

    /// Tests whether unknown ancestry can supply an unproven subclass relationship to `target`.
    /// Call this after ruling out known subclass relationships in both directions.
    #[synchronous(could_inherit_from_sync)]
    #[capabilities(effects = ReconciliationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn could_inherit_from_with<'db, E: ReconciliationEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        if effects.is_final(target).await?
            || !effects.is_assignable(env, source, target).await?
        {
            return Ok(false);
        }

        // A cyclic MRO uses an unknown base for recovery. It need not correspond to an
        // explicit unknown base, and rejecting it here can make recursive inference oscillate.
        if effects.has_cyclic_mro(source).await? {
            return Ok(true);
        }

        let mut target_ancestors = effects.ancestors_start().await?;
        {
            let mut target_mro = effects.mro_start(target).await?;
            #[cursor_loop]
            while let Some(base) = effects.mro_next(&mut target_mro).await? {
                let ClassBase::Class(class) = base else {
                    continue;
                };
                let class = effects.literal(class).await?;
                effects.ancestors_insert(&mut target_ancestors, class).await?;
            }
        }

        let mut source_mro = effects.mro_start(source).await?;
        #[cursor_loop]
        while let Some(base) = effects.mro_next(&mut source_mro).await? {
            let ClassBase::Class(class) = base else {
                continue;
            };
            let class = effects.literal(class).await?;
            if effects.ancestors_contains(&target_ancestors, class).await? {
                continue;
            }

            let mut bases = effects.bases_start(class).await?;
            #[cursor_loop]
            while let Some(base) = effects.bases_next(&mut bases).await? {
                match effects.resolve_base(env, class, base).await? {
                    Some(ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_)) => {
                        return Ok(true);
                    }
                    Some(
                        ClassBase::Class(_)
                        | ClassBase::Protocol
                        | ClassBase::Generic
                        | ClassBase::TypedDict(_),
                    )
                    | None => {}
                }
            }
        }
        Ok(false)
    }
}

/// Runs reconciliation through ordinary class queries and owned ancestry storage.
pub(super) struct InlineReconciliationEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineReconciliationEffects<'db> {
    /// Binds ordinary reconciliation effects to their query database.
    pub(super) const fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousReconciliationEffects<'db> for InlineReconciliationEffects<'db> {
    type Error = Infallible;
    type MroCursor<'state>
        = MroIterator<'db>
    where
        Self: 'state;
    type Ancestors<'state>
        = FxIndexSet<ClassLiteral<'db>>
    where
        Self: 'state;
    type Bases<'state>
        = std::vec::IntoIter<Type<'db>>
    where
        Self: 'state;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn is_subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.is_subclass_of(self.db, env, target))
    }

    fn could_inherit(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.could_inherit_from(self.db, env, target))
    }

    fn is_assignable(
        &self,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.has_relation_to(self.db, env, target, TypeRelation::Assignability))
    }

    fn is_final(&self, class: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(class.is_final(self.db))
    }

    fn has_cyclic_mro(&self, class: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(ClassBase::Class(class).has_cyclic_mro(self.db))
    }

    fn mro_start(&self, class: ClassType<'db>) -> Result<Self::MroCursor<'_>, Infallible> {
        Ok(class.iter_mro(self.db))
    }

    fn mro_next<'state>(
        &'state self,
        cursor: &mut Self::MroCursor<'state>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Infallible> {
        Ok(class.class_literal(self.db))
    }

    fn ancestors_start(&self) -> Result<Self::Ancestors<'_>, Infallible> {
        Ok(FxIndexSet::default())
    }

    fn ancestors_insert<'state>(
        &'state self,
        ancestors: &mut Self::Ancestors<'state>,
        class: ClassLiteral<'db>,
    ) -> Result<(), Infallible> {
        ancestors.insert(class);
        Ok(())
    }

    fn ancestors_contains<'state>(
        &'state self,
        ancestors: &Self::Ancestors<'state>,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ancestors.contains(&class))
    }

    fn bases_start(&self, class: ClassLiteral<'db>) -> Result<Self::Bases<'_>, Infallible> {
        Ok(class.explicit_bases(self.db).into_vec().into_iter())
    }

    fn bases_next<'state>(
        &'state self,
        bases: &mut Self::Bases<'state>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(bases.next())
    }

    fn resolve_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
        base: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(ClassBase::try_from_explicit_base(self.db, env, base, Some(class)))
    }
}
