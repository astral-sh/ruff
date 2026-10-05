//! Callable expansion shares its entry decisions while effects provide metadata and mutate guard storage.
//!
//! The caller retains a partial scope through suspended dependencies. Only successful insertions
//! belong to that scope, so interruption cannot remove an ancestor's entry.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::Definition;

use super::{
    CallableDefinition, CallableExpansion, CallableRecursionGuard, CallableVisitScope,
    DefinitionUse, RecursiveDefinition, TypeIdentity,
};
use crate::types::function::{FunctionLiteral, FunctionType};
use crate::types::newtype::NewType;
use crate::types::{
    ClassLiteral, ClassType, DescriptorDispatches, DescriptorOrigin, NominalInstanceType,
    ProtocolInstanceType, Specialization, StaticClassLiteral, SubclassOfInner, SubclassOfType,
    Type,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

/// Dependencies that can interrupt callable-guard entry or constructor reuse when execution
/// resources or an implementation are unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableGuardOperation {
    StorageOrigin,
    IdentitySpecialization,
    ProtocolOrigin,
    RecursiveIdentity,
    RepeatedDefinitionProof,
    DescriptorDeclarations,
    DescriptorObservations,
    StructuralEmbedding,
    ConstructorSpecializationProof,
    ConstructorCache,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum CallableEntryDecision {
    ExactCycle,
    Growth,
    #[cfg(test)]
    Incomplete,
    Entered,
}

/// An exact entry either repeats an active key or gives its scope a new active entry.
#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ExactCallableEntryDecision {
    ExactCycle,
    Entered,
}

pub(in crate::types) struct CallableEntryFacts;

shared_semantic_family! {
    #[synchronous(SynchronousConstructorGuardEffects)]
    pub(in crate::types) trait ConstructorGuardEffects<'db> {
        type Error;

        #[operation(local)]
        async fn constructor_probe_active(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn constructor_definition(&self, receiver: Type<'db>) -> Result<Option<DefinitionUse<'db>>, Self::Error>;
        #[operation(child)]
        async fn constructor_unbounded_specialization(&self, target: RecursiveDefinition<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(SynchronousCallableDefinitionEffects)]
    pub(in crate::types) trait CallableDefinitionEffects<'db> {
        type Error;

        #[operation(child)]
        async fn identity_class(&self, class: ClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_class(&self, instance: NominalInstanceType<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn protocol_class(&self, protocol: ProtocolInstanceType<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(source)]
        async fn static_class(&self, class: ClassType<'db>) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
    }

    #[synchronous(SynchronousRecursiveIdentityEffects)]
    pub(in crate::types) trait RecursiveIdentityEffects<'db> {
        type Error;

        #[operation(source)]
        async fn function_literal(&self, function: FunctionType<'db>) -> Result<FunctionLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn newtype_definition(&self, newtype: NewType<'db>) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_definition(&self, ty: Type<'db>) -> Result<Option<RecursiveDefinition<'db>>, Self::Error>;
        #[operation(child)]
        async fn unbounded_specialization(&self, target: RecursiveDefinition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn definition(&self, target: RecursiveDefinition<'db>) -> Result<Definition<'db>, Self::Error>;
    }

    #[synchronous(SynchronousCallableGuardEntryEffects)]
    pub(in crate::types) trait CallableGuardEntryEffects<'db> {
        type Error;

        #[operation(local)]
        async fn constructor_probe(&self, scope: &mut CallableVisitScope<'_, 'db>, key: (CallableExpansion, Type<'db>)) -> Result<Option<CallableEntryDecision>, Self::Error>;
        #[operation(local)]
        async fn record_dependency(&self, scope: &CallableVisitScope<'_, 'db>, key: (CallableExpansion, Type<'db>)) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn contains_exact(&self, scope: &CallableVisitScope<'_, 'db>, key: (CallableExpansion, Type<'db>)) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn callable_definition(&self, ty: Type<'db>, mode: CallableExpansion) -> Result<Option<DefinitionUse<'db>>, Self::Error>;
        #[operation(local)]
        async fn has_previous_definition(&self, scope: &CallableVisitScope<'_, 'db>, reference: DefinitionUse<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn repeated_definition_growth(&self, scope: &CallableVisitScope<'_, 'db>, reference: DefinitionUse<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn dispatch(&self, scope: &CallableVisitScope<'_, 'db>) -> Result<DescriptorOrigin<'db>, Self::Error>;
        #[operation(child)]
        async fn retain_anchor(&self, scope: &mut CallableVisitScope<'_, 'db>, reference: DefinitionUse<'db>, dispatches: DescriptorDispatches<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn insert_definition(&self, scope: &mut CallableVisitScope<'_, 'db>, reference: DefinitionUse<'db>, origin: DescriptorOrigin<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn recursive_identity(&self, ty: Type<'db>) -> Result<Option<TypeIdentity<'db>>, Self::Error>;
        #[operation(local)]
        async fn insert_identity(&self, scope: &mut CallableVisitScope<'_, 'db>, key: (CallableExpansion, TypeIdentity<'db>)) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn record_growth(&self, scope: &CallableVisitScope<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn insert_exact(&self, scope: &mut CallableVisitScope<'_, 'db>, key: (CallableExpansion, Type<'db>)) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl CallableEntryFacts {
        fn subclass_inner<'db>(&self, subclass: SubclassOfType<'db>) -> SubclassOfInner<'db> {
            subclass.subclass_of()
        }

        fn is_subclass(&self, ty: Type<'_>) -> bool {
            matches!(ty, Type::SubclassOf(_))
        }

        fn dispatches<'db>(&self, origin: DescriptorOrigin<'db>) -> Option<DescriptorDispatches<'db>> {
            origin.dispatches
        }

        fn specialization_target<'db>(&self, reference: DefinitionUse<'db>) -> Option<RecursiveDefinition<'db>> {
            reference.specialization.map(|_| reference.target)
        }
    }

    /// Selects whether a new callable guard can reuse canonical constructor queries.
    /// Unspecialized definitions need no growth proof; specialized definitions use the same
    /// unbounded-specialization check as ordinary constructor conversion.
    #[synchronous(constructor_guard_cache_sync)]
    #[capabilities(effects = ConstructorGuardEffects, facts = CallableEntryFacts)]
    #[passive_values()]
    pub(in crate::types) async fn constructor_guard_cache_with<'db, E: ConstructorGuardEffects<'db>>(
        receiver: Type<'db>,
        facts: CallableEntryFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        if effects.constructor_probe_active().await? {
            return Ok(false);
        }
        let Some(reference) = effects.constructor_definition(receiver).await? else {
            return Ok(true);
        };
        let Some(target) = facts.specialization_target(reference) else {
            return Ok(true);
        };
        Ok(!effects.constructor_unbounded_specialization(target).await?)
    }

    #[synchronous(callable_definition_sync)]
    #[capabilities(effects = CallableDefinitionEffects, facts = CallableEntryFacts)]
    #[passive_values(ClassType::Generic, CallableDefinition::SubclassConstructor, CallableDefinition::Constructor, CallableDefinition::Instance, RecursiveDefinition::Callable, DefinitionUse)]
    pub(in crate::types) async fn callable_definition_with<'db, E: CallableDefinitionEffects<'db>>(
        ty: Type<'db>,
        mode: CallableExpansion,
        facts: CallableEntryFacts,
        effects: &E,
    ) -> Result<Option<DefinitionUse<'db>>, E::Error> {
        let (class, is_constructor) = match ty {
            Type::ClassLiteral(class) => (effects.identity_class(class).await?, true),
            Type::GenericAlias(alias) => (ClassType::Generic(alias), true),
            Type::SubclassOf(subclass) => match facts.subclass_inner(subclass) {
                SubclassOfInner::Class(class) => (class, true),
                SubclassOfInner::Protocol(protocol) => {
                    let Some(class) = effects.protocol_class(protocol).await? else {
                        return Ok(None);
                    };
                    (class, true)
                }
                _ => return Ok(None),
            },
            Type::NominalInstance(instance) => (effects.instance_class(instance).await?, false),
            Type::ProtocolInstance(protocol) => {
                let Some(class) = effects.protocol_class(protocol).await? else {
                    return Ok(None);
                };
                (class, false)
            }
            _ => return Ok(None),
        };
        let Some((origin, specialization)) = effects.static_class(class).await? else {
            return Ok(None);
        };
        let definition = if facts.is_subclass(ty) {
            CallableDefinition::SubclassConstructor(origin)
        } else if is_constructor {
            CallableDefinition::Constructor(origin)
        } else {
            CallableDefinition::Instance(origin)
        };
        Ok(Some(DefinitionUse {
            target: RecursiveDefinition::Callable(definition, mode),
            specialization,
        }))
    }

    #[synchronous(recursive_identity_sync)]
    #[capabilities(effects = RecursiveIdentityEffects)]
    #[passive_values(TypeIdentity::FunctionLiteral, TypeIdentity::NewTypeInstance, TypeIdentity::GrowingTypeAlias, TypeIdentity::GrowingProtocol, TypeIdentity::GrowingTypedDict, TypeIdentity::GrowingRecursive)]
    #[inline]
    pub(in crate::types) async fn recursive_identity_with<'db, E: RecursiveIdentityEffects<'db>>(
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Option<TypeIdentity<'db>>, E::Error> {
        match ty {
            // We can create a self-referential function type: e.g. `def f(x: "TypeOf[f]"): reveal_type(x)`
            // To avoid the difficulty of equality checking for function types containing this, we simply use `literal` for equality checking.
            Type::FunctionLiteral(function) => {
                Ok(Some(TypeIdentity::FunctionLiteral(effects.function_literal(function).await?)))
            }
            // Similarly, we can create a self-referential NewType: e.g. `T = NewType("T", list["T"])`
            Type::NewTypeInstance(newtype) => {
                Ok(Some(TypeIdentity::NewTypeInstance(effects.newtype_definition(newtype).await?)))
            }
            // Recursive aliases, protocols, and TypedDicts whose specialization can keep changing
            // (e.g. `type Growing[T] = T | Growing[list[T]]`) are collapsed to their definition so
            // that visits stop even though no exact type repeats. Recursion that revisits one
            // exact specialization (e.g. `type RecursiveT = int | tuple[RecursiveT, ...]`) needs
            // no definition-level identity: the detectors stop on the repeated type itself.
            Type::TypeAlias(_)
            | Type::ProtocolInstance(_)
            | Type::TypedDict(_)
            | Type::Recursive(_) => {
                let Some(target) = effects.recursive_definition(ty).await? else {
                    return Ok(None);
                };
                if !effects.unbounded_specialization(target).await? {
                    return Ok(None);
                }
                let definition = effects.definition(target).await?;
                Ok(Some(match target {
                    RecursiveDefinition::TypeAlias(_) => TypeIdentity::GrowingTypeAlias(definition),
                    RecursiveDefinition::Protocol(_) => TypeIdentity::GrowingProtocol(definition),
                    RecursiveDefinition::TypedDict(_) => TypeIdentity::GrowingTypedDict(definition),
                    RecursiveDefinition::Structural(_) => TypeIdentity::GrowingRecursive(definition),
                    RecursiveDefinition::Callable(_, _) => return Ok(None),
                }))
            }
            _ => Ok(None),
        }
    }

    /// Enters a callable expansion while leaving partial entry ownership with the caller.
    ///
    /// Effects install each successful insertion in `scope` before they return. If a dependency
    /// interrupts entry, the caller retains the scope until its suspended children have drained.
    /// Those effect futures can still depend on the active entries owned by this scope.
    #[synchronous(callable_enter_in_place_sync)]
    #[capabilities(effects = CallableGuardEntryEffects, facts = CallableEntryFacts)]
    #[passive_values(CallableEntryDecision::ExactCycle, CallableEntryDecision::Growth, CallableEntryDecision::Entered)]
    pub(in crate::types) async fn callable_enter_in_place_with<'db, E: CallableGuardEntryEffects<'db>>(
        key: (CallableExpansion, Type<'db>),
        scope: &mut CallableVisitScope<'_, 'db>,
        facts: CallableEntryFacts,
        effects: &E,
    ) -> Result<CallableEntryDecision, E::Error> {
        if let Some(decision) = effects.constructor_probe(scope, key).await? {
            return Ok(decision);
        }
        effects.record_dependency(scope, key).await?;
        if effects.contains_exact(scope, key).await? {
            return Ok(CallableEntryDecision::ExactCycle);
        }

        let (mode, ty) = key;
        if let Some(reference) = effects.callable_definition(ty, mode).await? {
            // The first use of a definition cannot grow relative to an active use. In particular,
            // it does not need the specialization-flow proof or descriptor observations.
            if effects.has_previous_definition(scope, reference).await?
                && effects.repeated_definition_growth(scope, reference).await?
            {
                effects.record_growth(scope).await?;
                return Ok(CallableEntryDecision::Growth);
            }
            let origin = effects.dispatch(scope).await?;
            if let Some(dispatches) = facts.dispatches(origin) {
                effects.retain_anchor(scope, reference, dispatches).await?;
            }
            let origin = effects.dispatch(scope).await?;
            effects.insert_definition(scope, reference, origin).await?;
        } else if let Some(identity) = effects.recursive_identity(ty).await?
            && !effects.insert_identity(scope, (mode, identity)).await?
        {
            effects.record_growth(scope).await?;
            return Ok(CallableEntryDecision::Growth);
        }

        if !effects.insert_exact(scope, key).await? {
            return Ok(CallableEntryDecision::ExactCycle);
        }
        Ok(CallableEntryDecision::Entered)
    }
}

/// Enters an exact callable key without approximating changing specializations.
///
/// Entry records its exact dependency before either successful result. That dependency
/// remains guard/cache state after this visit ends; it is not an active-entry removal receipt.
/// Effects install each successful exact insertion in `scope` before returning, including when
/// completion fails after mutation. A repeated key leaves its ancestor's removal ownership intact.
/// The caller retains `scope` until suspended children have drained, including when entry returns
/// an error, because those children can depend on its active entries. Growth is not a successful
/// outcome: distinct keys continue until their caller completes or interrupts.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) async fn callable_enter_exact_in_place_with<
    'db,
    E: CallableGuardEntryEffects<'db>,
>(
    key: (CallableExpansion, Type<'db>),
    scope: &mut CallableVisitScope<'_, 'db>,
    effects: &E,
) -> Result<ExactCallableEntryDecision, E::Error> {
    effects.record_dependency(scope, key).await?;
    if effects.contains_exact(scope, key).await? || !effects.insert_exact(scope, key).await? {
        return Ok(ExactCallableEntryDecision::ExactCycle);
    }
    Ok(ExactCallableEntryDecision::Entered)
}

pub(super) struct OrdinaryCallableDefinitionEffects<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

impl<'db> SynchronousCallableDefinitionEffects<'db> for OrdinaryCallableDefinitionEffects<'_, 'db> {
    type Error = Infallible;

    fn identity_class(&self, class: ClassLiteral<'db>) -> Result<ClassType<'db>, Infallible> {
        Ok(class.identity_specialization(self.db))
    }

    fn instance_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Infallible> {
        Ok(instance.class(self.db, self.env))
    }

    fn protocol_class(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(protocol.class_origin(self.db).map(|class| *class))
    }

    fn static_class(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Infallible> {
        Ok(class.static_class_literal(self.db))
    }
}

pub(super) struct OrdinaryRecursiveIdentityEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousRecursiveIdentityEffects<'db> for OrdinaryRecursiveIdentityEffects<'db> {
    type Error = Infallible;

    fn function_literal(
        &self,
        function: FunctionType<'db>,
    ) -> Result<FunctionLiteral<'db>, Infallible> {
        Ok(function.literal(self.db))
    }

    fn newtype_definition(&self, newtype: NewType<'db>) -> Result<Definition<'db>, Infallible> {
        Ok(newtype.definition(self.db))
    }

    fn recursive_definition(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<RecursiveDefinition<'db>>, Infallible> {
        Ok(RecursiveDefinition::from_type(self.db, ty).map(|reference| reference.target))
    }

    fn unbounded_specialization(
        &self,
        target: RecursiveDefinition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(target.may_have_unbounded_specialization(self.db))
    }

    fn definition(&self, target: RecursiveDefinition<'db>) -> Result<Definition<'db>, Infallible> {
        Ok(target.definition(self.db))
    }
}

pub(super) struct OrdinaryCallableGuardEntryEffects<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

impl<'db> SynchronousConstructorGuardEffects<'db> for OrdinaryCallableGuardEntryEffects<'_, 'db> {
    type Error = Infallible;

    fn constructor_probe_active(&self) -> Result<bool, Infallible> {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::active() {
            return Ok(true);
        }
        Ok(false)
    }

    fn constructor_definition(
        &self,
        receiver: Type<'db>,
    ) -> Result<Option<DefinitionUse<'db>>, Infallible> {
        Ok(CallableDefinition::from_type(
            self.db,
            self.env,
            receiver,
            CallableExpansion::Upcast,
        ))
    }

    fn constructor_unbounded_specialization(
        &self,
        target: RecursiveDefinition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(target.may_have_unbounded_specialization(self.db))
    }
}

impl<'db> SynchronousCallableGuardEntryEffects<'db> for OrdinaryCallableGuardEntryEffects<'_, 'db> {
    type Error = Infallible;

    fn constructor_probe(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<Option<CallableEntryDecision>, Infallible> {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::active() {
            if crate::types::constructor::expansion_probe::admit(self.db, key.0).is_err() {
                return Ok(Some(CallableEntryDecision::Incomplete));
            }
            if !scope.insert_exact_ordinary(key) {
                crate::types::constructor::expansion_probe::exact_cycle(self.db);
                return Ok(Some(CallableEntryDecision::Incomplete));
            }
            return Ok(Some(CallableEntryDecision::Entered));
        }
        let _ = (scope, key);
        Ok(None)
    }

    fn record_dependency(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<(), Infallible> {
        scope.guard.insert_dependency_ordinary(key);
        Ok(())
    }

    fn contains_exact(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<bool, Infallible> {
        Ok(scope.guard.active.seen.borrow().contains(&key))
    }

    fn callable_definition(
        &self,
        ty: Type<'db>,
        mode: CallableExpansion,
    ) -> Result<Option<DefinitionUse<'db>>, Infallible> {
        Ok(CallableDefinition::from_type(self.db, self.env, ty, mode))
    }

    fn has_previous_definition(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
    ) -> Result<bool, Infallible> {
        Ok(scope
            .guard
            .growth
            .active
            .seen
            .borrow()
            .iter()
            .any(|(previous, _)| previous.target == reference.target))
    }

    fn repeated_definition_growth(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
    ) -> Result<bool, Infallible> {
        Ok(scope
            .guard
            .growth
            .should_approximate_repeated(self.db, self.env, reference))
    }

    fn dispatch(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
    ) -> Result<DescriptorOrigin<'db>, Infallible> {
        Ok(scope.guard.descriptor_origin())
    }

    fn retain_anchor(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
        dispatches: DescriptorDispatches<'db>,
    ) -> Result<(), Infallible> {
        let already_retained =
            scope
                .guard
                .growth
                .anchors
                .borrow()
                .iter()
                .any(|(target, initial)| {
                    *target == reference.target
                        && dispatches.declarations(self.db) == initial.declarations(self.db)
                });
        if !already_retained {
            scope.push_anchor_ordinary((reference.target, dispatches));
        }
        Ok(())
    }

    fn insert_definition(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Result<(), Infallible> {
        scope.insert_definition_ordinary((reference, origin));
        Ok(())
    }

    fn recursive_identity(&self, ty: Type<'db>) -> Result<Option<TypeIdentity<'db>>, Infallible> {
        Ok(ty.recursive_identity(self.db))
    }

    fn insert_identity(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, TypeIdentity<'db>),
    ) -> Result<bool, Infallible> {
        Ok(scope.insert_identity_ordinary(key))
    }

    fn record_growth(&self, scope: &CallableVisitScope<'_, 'db>) -> Result<(), Infallible> {
        scope.guard.record_growth_recovery();
        Ok(())
    }

    fn insert_exact(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<bool, Infallible> {
        Ok(scope.insert_exact_ordinary(key))
    }
}

impl<'db> CallableRecursionGuard<'db> {
    pub(in crate::types) fn descriptor_origin(&self) -> DescriptorOrigin<'db> {
        self.growth.dispatch.get()
    }

    pub(in crate::types) fn record_growth_recovery(&self) {
        self.cache
            .growth_recoveries
            .set(self.cache.growth_recoveries.get() + 1);
    }
}

impl<'guard, 'db> CallableVisitScope<'guard, 'db> {
    pub(in crate::types) fn guard(&self) -> &'guard CallableRecursionGuard<'db> {
        self.guard
    }
}
