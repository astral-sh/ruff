//! Collect type variables referenced by explicit base expressions in encounter order.

use std::convert::Infallible;
use std::fmt;

use smallvec::SmallVec;

use crate::types::visitor::{
    NonAtomicType, OrdinaryTypeWalk, SyncTypeWalkEffects, TypeCollector, TypeKind, TypeWalkCursor,
    TypeWalkEvent, TypeWalkPolicy, Unrestricted, WalkAction,
};
use crate::types::{BoundTypeVarInstance, StaticClassLiteral, Type};
use crate::{Db, FxIndexSet, ProgramEnvironment};

/// Retains encounter order and completed visits across all explicit bases of one class.
pub(in crate::types) struct BaseTypeVarCollector<'db> {
    pub(in crate::types) env: ProgramEnvironment<'db>,
    pub(in crate::types) cursor: TypeWalkCursor<'db>,
    pub(in crate::types) typevars: FxIndexSet<BoundTypeVarInstance<'db>>,
    pub(in crate::types) recursion_guard: TypeCollector<'db>,
    #[cfg(test)]
    pub(in crate::types) class: StaticClassLiteral<'db>,
}

impl fmt::Debug for BaseTypeVarCollector<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BaseTypeVarCollector")
            .field("pending", &self.cursor.pending.len())
            .field("typevars", &self.typevars.len())
            .field("recursion_guard", &self.recursion_guard)
            .finish_non_exhaustive()
    }
}

impl<'db> BaseTypeVarCollector<'db> {
    /// Creates empty traversal and result state for the complete base list.
    pub(in crate::types) fn new(
        env: ProgramEnvironment<'db>,
        #[cfg(test)] class: StaticClassLiteral<'db>,
    ) -> Self {
        Self {
            env,
            cursor: TypeWalkCursor {
                pending: SmallVec::new(),
            },
            typevars: FxIndexSet::default(),
            recursion_guard: TypeCollector::default(),
            #[cfg(test)]
            class,
        }
    }

    pub(in crate::types) fn into_typevars(self) -> FxIndexSet<BoundTypeVarInstance<'db>> {
        self.typevars
    }
}

/// Classifies complete type values without reading semantic fields.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct BaseTypeVarFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousBaseTypeVarEffects)]
    pub(in crate::types) trait BaseTypeVarEffects<'db> {
        type Error;

        /// Creates the empty state shared by every base in one class's variable scan.
        /// Admission covers construction and disposal of the empty traversal and result state.
        #[operation(local)]
        async fn new_collector(&self, class: StaticClassLiteral<'db>) -> Result<BaseTypeVarCollector<'db>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_base(&self, bases: &[Type<'db>], cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        /// Adds one base's variable occurrences to the existing collector.
        /// Traversal must admit mutations and retained ownership before its collections grow.
        #[operation(child)]
        async fn visit_base(&self, collector: &mut BaseTypeVarCollector<'db>, base: Type<'db>) -> Result<(), Self::Error>;
        /// Transfers the actual result set and disposes of the collector's recursion state.
        #[operation(local)]
        async fn finish_collector(&self, collector: BaseTypeVarCollector<'db>) -> Result<FxIndexSet<BoundTypeVarInstance<'db>>, Self::Error>;
    }

    #[synchronous(SynchronousBaseTypeVarWalkEffects)]
    pub(in crate::types) trait BaseTypeVarWalkEffects<'db> {
        type Error;
        #[operation(child)]
        async fn push(&self, collector: &mut BaseTypeVarCollector<'db>, action: WalkAction<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_event(&self, collector: &mut BaseTypeVarCollector<'db>) -> Result<Option<TypeWalkEvent<'db>>, Self::Error>;
        #[operation(child)]
        async fn remember(&self, collector: &mut BaseTypeVarCollector<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn record(&self, collector: &mut BaseTypeVarCollector<'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn expand(&self, collector: &mut BaseTypeVarCollector<'db>, kind: NonAtomicType<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl BaseTypeVarFacts {
        fn kind<'db>(&self, ty: Type<'db>) -> TypeKind<'db> {
            TypeKind::from(ty)
        }
    }

    #[synchronous(typevars_referenced_in_bases_sync)]
    #[capabilities(effects = BaseTypeVarEffects)]
    #[passive_values()]
    pub(in crate::types) async fn typevars_referenced_in_bases_with<'db, E: BaseTypeVarEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<FxIndexSet<BoundTypeVarInstance<'db>>, E::Error> {
        let mut collector = effects.new_collector(class).await?;
        let bases = effects.explicit_bases(class).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(base) = effects.next_base(bases, &mut cursor).await? {
            effects.visit_base(&mut collector, base).await?;
        }
        effects.finish_collector(collector).await
    }

    /// Drains one base's stored children while preserving visits and variables from earlier bases.
    /// Bound occurrences are retained unchanged; lazy alias values do not contribute children.
    #[synchronous(visit_base_typevars_sync)]
    #[capabilities(effects = BaseTypeVarWalkEffects, facts = BaseTypeVarFacts)]
    #[passive_values(WalkAction::Visit, WalkAction::Expand)]
    pub(in crate::types) async fn visit_base_typevars_with<'db, E: BaseTypeVarWalkEffects<'db>>(
        collector: &mut BaseTypeVarCollector<'db>,
        base: Type<'db>,
        facts: BaseTypeVarFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.push(collector, WalkAction::Visit(base)).await?;
        #[cursor_loop]
        while let Some(event) = effects.next_event(collector).await? {
            match event {
                TypeWalkEvent::Visit(ty) => {
                    if let TypeKind::NonAtomic(kind) = facts.kind(ty)
                        && !effects.remember(collector, ty).await?
                    {
                        effects.push(collector, WalkAction::Expand(kind)).await?;
                    }
                }
                TypeWalkEvent::Expand(NonAtomicType::TypeVar(variable)) => {
                    effects.record(collector, variable).await?;
                }
                // Re-expanding an alias would re-emit this event before checking lazy policy.
                // The ordinary collector skips its value and continues with other pending work.
                TypeWalkEvent::Expand(NonAtomicType::TypeAlias(_)) => {}
                TypeWalkEvent::Expand(kind @ (
                    NonAtomicType::Union(_) | NonAtomicType::Intersection(_)
                    | NonAtomicType::EnumComplement(_) | NonAtomicType::FunctionLiteral(_)
                    | NonAtomicType::BoundMethod(_) | NonAtomicType::BoundSuper(_)
                    | NonAtomicType::MethodWrapper(_) | NonAtomicType::Callable(_)
                    | NonAtomicType::GenericAlias(_) | NonAtomicType::KnownInstance(_)
                    | NonAtomicType::SubclassOf(_) | NonAtomicType::NominalInstance(_)
                    | NonAtomicType::PropertyInstance(_) | NonAtomicType::SlotDescriptor(_)
                    | NonAtomicType::TypeIs(_) | NonAtomicType::TypeGuard(_)
                    | NonAtomicType::TypeForm(_) | NonAtomicType::ProtocolInstance(_)
                    | NonAtomicType::TypedDict(_) | NonAtomicType::Recursive(_)
                    | NonAtomicType::NewTypeInstance(_)
                )) => effects.expand(collector, kind).await?,
                TypeWalkEvent::SkippedLazy | TypeWalkEvent::EndScope
                | TypeWalkEvent::ExitDepth { .. } => {}
            }
        }
        Ok(())
    }
}

pub(in crate::types) fn next_base_type<'db>(
    bases: &[Type<'db>],
    cursor: &mut usize,
) -> Option<Type<'db>> {
    let base = *bases.get(*cursor)?;
    *cursor += 1;
    Some(base)
}

pub(super) struct OrdinaryBaseTypeVarEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> OrdinaryBaseTypeVarEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousBaseTypeVarEffects<'db> for OrdinaryBaseTypeVarEffects<'db> {
    type Error = Infallible;

    fn new_collector(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<BaseTypeVarCollector<'db>, Infallible> {
        Ok(BaseTypeVarCollector::new(
            ProgramEnvironment::from_scope(class.body_scope(self.db)),
            #[cfg(test)]
            class,
        ))
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.db))
    }

    fn next_base(
        &self,
        bases: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(next_base_type(bases, cursor))
    }

    fn visit_base(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        base: Type<'db>,
    ) -> Result<(), Infallible> {
        visit_base_typevars_sync(collector, base, BaseTypeVarFacts, self)
    }

    fn finish_collector(
        &self,
        collector: BaseTypeVarCollector<'db>,
    ) -> Result<FxIndexSet<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(collector.into_typevars())
    }
}

impl<'db> SynchronousBaseTypeVarWalkEffects<'db> for OrdinaryBaseTypeVarEffects<'db> {
    type Error = Infallible;

    fn push(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: &collector.env,
            control: &mut Unrestricted,
            query: (),
        }
        .push_action(&mut collector.cursor, action)
    }

    fn next_event(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
    ) -> Result<Option<TypeWalkEvent<'db>>, Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: &collector.env,
            control: &mut Unrestricted,
            query: (),
        }
        .next_event(&mut collector.cursor, TypeWalkPolicy::base_variables())
    }

    fn remember(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: &collector.env,
            control: &mut Unrestricted,
            query: (),
        }
        .remember_type(&mut collector.recursion_guard, ty)
    }

    fn record(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        collector.typevars.insert(variable);
        Ok(())
    }

    fn expand(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        kind: NonAtomicType<'db>,
    ) -> Result<(), Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: &collector.env,
            control: &mut Unrestricted,
            query: (),
        }
        .expand_children(
            &mut collector.cursor,
            kind,
            TypeWalkPolicy::base_variables(),
        )
    }
}
