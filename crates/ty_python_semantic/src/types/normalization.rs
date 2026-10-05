use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::set_theoretic::widening::{OrdinaryTupleWideningEffects, recovery_union_sync};
use super::visitor::any_over_type;
use super::{
    BoundMethodType, BoundSuperType, CallableType, ClassLiteral, DynamicType, EnumComplementType,
    FunctionType, GenericAlias, IntersectionType, KnownBoundMethodType, KnownInstanceType, NewType,
    NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, RecursiveVar,
    SlotDescriptorType, SubclassOfType, Type, TypeFormType, TypeGuardType, TypeIsType, UnionType,
    recursive_type_normalize_type_guard_like,
};
use crate::{Db, ProgramEnvironment};

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) mod source;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecursiveNormalizationOperation {
    UnboundRecursiveVariable,
    Union,
    Intersection,
    EnumComplement,
    Callable,
    ProtocolInstance,
    FunctionLiteral,
    PropertyInstance,
    SlotDescriptor,
    KnownBoundMethod,
    BoundMethod,
    BoundSuper,
    GenericAlias,
    ClassLiteral,
    SubclassOf,
    KnownInstance,
    TypeIs,
    TypeGuard,
    NewTypeInstance,
    NonTupleInstance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct RecursiveNormalizationRequest<'db> {
    pub(in crate::types) ty: Type<'db>,
    pub(in crate::types) divergent: Type<'db>,
    pub(in crate::types) nested: bool,
}

pub(in crate::types) struct RecursiveNormalizationFacts;

pub(in crate::types) struct NormalizationFacts;

pub(in crate::types) enum NormalizationSearch {
    AmbiguousOverload,
    Divergent,
}

pub(super) struct OrdinaryNormalizationEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousNormalizationEffects)]
    pub(in crate::types) trait NormalizationEffects<'db> {
        type Error;

        #[operation(child)]
        async fn contains(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, search: NormalizationSearch) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn merge_aliases(&self, current: GenericAlias<'db>, previous: GenericAlias<'db>) -> Result<Option<GenericAlias<'db>>, Self::Error>;
        #[operation(child)]
        async fn recovery_union(&self, env: &ProgramEnvironment<'db>, previous: Type<'db>, current: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn widen_tuples(&self, env: &ProgramEnvironment<'db>, previous: Type<'db>, current: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn normalize_heads(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, cycle: &salsa::Cycle<'_>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_head(&self, heads: &mut salsa::CycleHeadCandidates<'_>) -> Result<Option<salsa::CycleHeadCandidate>, Self::Error>;
        #[operation(child)]
        async fn recursive_normalize(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl NormalizationFacts {
        fn early_iteration(&self, cycle: &salsa::Cycle<'_>) -> bool {
            cycle.iteration() <= crate::TAINTED_CYCLES
        }

        fn head_cursor<'cycle>(
            &self,
            cycle: &'cycle salsa::Cycle<'_>,
        ) -> salsa::CycleHeadCandidates<'cycle> {
            cycle.head_candidates()
        }

        fn divergent<'db>(&self, id: salsa::Id) -> Type<'db> {
            Type::divergent(id)
        }

        fn normalized_or<'db>(&self, normalized: Option<Type<'db>>, fallback: Type<'db>) -> Type<'db> {
            normalized.unwrap_or(fallback)
        }
    }

    #[synchronous(cycle_normalized_sync)]
    #[capabilities(effects = NormalizationEffects, facts = NormalizationFacts)]
    #[passive_values(Type::GenericAlias, NormalizationSearch::AmbiguousOverload, NormalizationSearch::Divergent)]
    pub(in crate::types) async fn cycle_normalized_with<'db, E: NormalizationEffects<'db>>(
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
        facts: NormalizationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        // When we encounter a salsa cycle, we want to avoid oscillating between two or more types
        // without converging on a fixed-point result. Most of the time, we union together the
        // types from each cycle iteration to ensure that our result is monotonic, even if we
        // encounter oscillation.
        //
        // However, for the first couple iterations we are prone to get values including Divergent
        // that will soon converge, but where unioning in the early value causes a loss of
        // precision that we can't recover from. For example, a narrowing condition that looks like
        // `is not Divergent` instead of `is not None` in the first iteration may cause us to lose
        // the effect of that narrowing permanently, due to the union-previous-iteration behavior.
        // So we avoid unioning in the first couple iterations, and just use the later iteration's
        // result directly. We still ensure monotonicity after the first couple iterations, which
        // still ensures convergence in cases that are prone to oscillation.
        let result = if facts.early_iteration(cycle) {
            let self_degraded_by_overload =
                effects.contains(ty, env, NormalizationSearch::AmbiguousOverload).await?
                    && !effects.contains(ty, env, NormalizationSearch::Divergent).await?
                    && effects.contains(previous, env, NormalizationSearch::Divergent).await?;
            // Generally, the precision of type inference improves with each iteration.
            // However, overload is an exception; as iterations progress, overload matching may become ambiguous, and a reversal of precision can occur.
            // This kind of precision degradation can be determined by whether the type contains `DynamicType::AmbiguousOverload`.
            if self_degraded_by_overload {
                effects.recovery_union(env, previous, ty).await?
            } else {
                ty
            }
        } else if let (Type::GenericAlias(current), Type::GenericAlias(previous)) = (ty, previous)
            && let Some(merged) = effects.merge_aliases(current, previous).await?
        {
            Type::GenericAlias(merged)
        } else {
            // The current type is unioned to the previous type. Unioning in the reverse order can
            // cause the fixed-point iterations to converge slowly or even fail. Consider the case
            // where the order of union types is different between the previous and current cycle.
            // We should use the previous union type as the base and only add new element types in
            // this cycle, if any.
            effects.recovery_union(env, previous, ty).await?
        };
        // An inferred attribute updated with `self.items += (item,)` can settle on the
        // initializer plus a single update during the first few iterations. Widen new tuple
        // lengths during those iterations too, so repeated updates are represented.
        let widened = effects.widen_tuples(env, previous, result).await?;
        let result = facts.normalized_or(widened, result);
        effects.normalize_heads(result, env, cycle).await
    }

    #[synchronous(recursive_type_normalized_with_cycle_sync)]
    #[capabilities(effects = NormalizationEffects, facts = NormalizationFacts)]
    #[passive_values(salsa::CycleHeadCandidate::Present)]
    pub(in crate::types) async fn recursive_type_normalized_with_cycle<'db, E: NormalizationEffects<'db>>(
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
        facts: NormalizationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut heads = facts.head_cursor(cycle);
        #[passive_state]
        let mut result = ty;
        #[cursor_loop]
        while let Some(candidate) = effects.next_head(&mut heads).await? {
            if let salsa::CycleHeadCandidate::Present(id) = candidate {
                let divergent = facts.divergent(id);
                let normalized = effects.recursive_normalize(result, env, divergent, false).await?;
                result = facts.normalized_or(normalized, divergent);
            }
        }
        Ok(result)
    }
}

impl<'db> SynchronousNormalizationEffects<'db> for OrdinaryNormalizationEffects<'db> {
    type Error = Infallible;

    fn contains(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        search: NormalizationSearch,
    ) -> Result<bool, Infallible> {
        Ok(any_over_type(self.db, env, ty, false, |ty| match search {
            NormalizationSearch::AmbiguousOverload => {
                matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
            }
            NormalizationSearch::Divergent => ty.is_divergent(),
        }))
    }

    fn merge_aliases(
        &self,
        current: GenericAlias<'db>,
        previous: GenericAlias<'db>,
    ) -> Result<Option<GenericAlias<'db>>, Infallible> {
        Ok(current.merge_cycle_recovery(self.db, previous))
    }

    fn recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        recovery_union_sync(
            previous,
            current,
            env,
            &OrdinaryTupleWideningEffects { db: self.db },
        )
    }

    fn widen_tuples(
        &self,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(UnionType::widen_growing_tuples(
            self.db, env, previous, current,
        ))
    }

    fn normalize_heads(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Infallible> {
        recursive_type_normalized_with_cycle_sync(ty, env, cycle, NormalizationFacts, self)
    }

    fn next_head(
        &self,
        heads: &mut salsa::CycleHeadCandidates<'_>,
    ) -> Result<Option<salsa::CycleHeadCandidate>, Infallible> {
        Ok(heads.next())
    }

    fn recursive_normalize(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.recursive_type_normalized_impl(self.db, env, divergent, nested))
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousRecursiveNormalizationEffects)]
    pub(in crate::types) trait RecursiveNormalizationEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn unbound_recursive_variable(&self, value: RecursiveVar<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn union(&self, value: UnionType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn intersection(&self, value: IntersectionType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn enum_complement(&self, value: EnumComplementType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn callable(&self, value: CallableType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn protocol_instance(&self, value: ProtocolInstanceType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn nominal_instance(&self, value: NominalInstanceType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn function_literal(&self, value: FunctionType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn property_instance(&self, value: PropertyInstanceType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn slot_descriptor(&self, value: SlotDescriptorType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn known_bound_method(&self, value: KnownBoundMethodType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn bound_method(&self, value: BoundMethodType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn bound_super(&self, value: BoundSuperType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn generic_alias(&self, value: GenericAlias<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn class_literal(&self, value: ClassLiteral<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn subclass_of(&self, value: SubclassOfType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, value: KnownInstanceType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn type_is(&self, value: TypeIsType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn type_guard(&self, value: TypeGuardType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn new_type_instance(&self, value: NewType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn type_form_argument(&self, value: TypeFormType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn normalize_child(&self, request: RecursiveNormalizationRequest<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn intern_type_form(&self, argument: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl RecursiveNormalizationFacts {
        fn collapses(&self, request: RecursiveNormalizationRequest<'_>) -> bool {
            request.nested && request.ty.same_divergent_marker(request.divergent)
        }

        fn child<'db>(&self, ty: Type<'db>, divergent: Type<'db>) -> RecursiveNormalizationRequest<'db> {
            RecursiveNormalizationRequest { ty, divergent, nested: true }
        }

        fn dynamic<'db>(&self, dynamic: DynamicType<'db>) -> Type<'db> {
            Type::Dynamic(dynamic.recursive_type_normalized())
        }
    }

    #[synchronous(recursive_normalize_sync)]
    #[capabilities(effects = RecursiveNormalizationEffects, facts = RecursiveNormalizationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn recursive_normalize_with<'db, E: RecursiveNormalizationEffects<'db>>(
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        facts: RecursiveNormalizationFacts,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        if facts.collapses(request) {
            return Ok(None);
        }
        let divergent = request.divergent;
        let nested = request.nested;
        match request.ty {
            Type::RecursiveVar(value) => effects.unbound_recursive_variable(value, env, divergent, nested).await,
            Type::Union(value) => effects.union(value, env, divergent, nested).await,
            Type::Intersection(value) => effects.intersection(value, env, divergent, nested).await,
            Type::EnumComplement(value) => effects.enum_complement(value, env, divergent, nested).await,
            Type::Callable(value) => effects.callable(value, env, divergent, nested).await,
            Type::ProtocolInstance(value) => effects.protocol_instance(value, env, divergent, nested).await,
            Type::NominalInstance(value) => effects.nominal_instance(value, env, divergent, nested).await,
            Type::FunctionLiteral(value) => effects.function_literal(value, env, divergent, nested).await,
            Type::PropertyInstance(value) => effects.property_instance(value, env, divergent, nested).await,
            Type::SlotDescriptor(value) => effects.slot_descriptor(value, env, divergent, nested).await,
            Type::KnownBoundMethod(value) => effects.known_bound_method(value, env, divergent, nested).await,
            Type::BoundMethod(value) => effects.bound_method(value, env, divergent, nested).await,
            Type::BoundSuper(value) => effects.bound_super(value, env, divergent, nested).await,
            Type::GenericAlias(value) => effects.generic_alias(value, env, divergent, nested).await,
            Type::Recursive(_) => Ok(Some(request.ty)),
            Type::ClassLiteral(value) => effects.class_literal(value, env, divergent, nested).await,
            Type::SubclassOf(value) => effects.subclass_of(value, env, divergent, nested).await,
            Type::TypeVar(_) => Ok(Some(request.ty)),
            Type::KnownInstance(value) => effects.known_instance(value, env, divergent, nested).await,
            Type::TypeIs(value) => effects.type_is(value, env, divergent, nested).await,
            Type::TypeGuard(value) => effects.type_guard(value, env, divergent, nested).await,
            Type::TypeForm(value) => {
                let argument = effects.type_form_argument(value).await?;
                let normalized = effects.normalize_child(facts.child(argument, divergent), env).await?;
                match normalized {
                    Some(argument) => Ok(Some(effects.intern_type_form(argument).await?)),
                    None => Ok(None),
                }
            }
            Type::Divergent(_) => Ok(Some(request.ty)),
            Type::Dynamic(dynamic) => Ok(Some(facts.dynamic(dynamic))),
            Type::TypedDict(_) => {
                // TODO: Normalize TypedDicts
                Ok(Some(request.ty))
            }
            Type::TypeAlias(_) => Ok(Some(request.ty)),
            Type::NewTypeInstance(value) => effects.new_type_instance(value, env, divergent, nested).await,
            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::Never
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::LiteralValue(_) => Ok(Some(request.ty)),
        }
    }
}

impl<'db> SynchronousRecursiveNormalizationEffects<'db> for OrdinaryNormalizationEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn unbound_recursive_variable(
        &self,
        _value: RecursiveVar<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        unreachable!("semantic operation on an unbound recursive variable")
    }

    fn union(
        &self,
        value: UnionType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value.recursive_type_normalized_impl(self.db, env, divergent, nested))
    }

    fn intersection(
        &self,
        value: IntersectionType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::Intersection))
    }

    fn enum_complement(
        &self,
        value: EnumComplementType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .to_intersection(self.db, env)
            .recursive_type_normalized_impl(self.db, env, divergent, nested))
    }

    fn callable(
        &self,
        value: CallableType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::Callable))
    }

    fn protocol_instance(
        &self,
        value: ProtocolInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::ProtocolInstance))
    }

    fn nominal_instance(
        &self,
        value: NominalInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::NominalInstance))
    }

    fn function_literal(
        &self,
        value: FunctionType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::FunctionLiteral))
    }

    fn property_instance(
        &self,
        value: PropertyInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::PropertyInstance))
    }

    fn slot_descriptor(
        &self,
        value: SlotDescriptorType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        _nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .value_type(self.db)
            .recursive_type_normalized_impl(self.db, env, divergent, true)
            .map(|value_type| Type::SlotDescriptor(SlotDescriptorType::new(self.db, value_type))))
    }

    fn known_bound_method(
        &self,
        value: KnownBoundMethodType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::KnownBoundMethod))
    }

    fn bound_method(
        &self,
        value: BoundMethodType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::BoundMethod))
    }

    fn bound_super(
        &self,
        value: BoundSuperType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::BoundSuper))
    }

    fn generic_alias(
        &self,
        value: GenericAlias<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::GenericAlias))
    }

    fn class_literal(
        &self,
        value: ClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::ClassLiteral))
    }

    fn subclass_of(
        &self,
        value: SubclassOfType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::SubclassOf))
    }

    fn known_instance(
        &self,
        value: KnownInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::KnownInstance))
    }

    fn type_is(
        &self,
        value: TypeIsType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(recursive_type_normalize_type_guard_like(
            self.db, env, value, divergent, nested,
        ))
    }

    fn type_guard(
        &self,
        value: TypeGuardType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(recursive_type_normalize_type_guard_like(
            self.db, env, value, divergent, nested,
        ))
    }

    fn new_type_instance(
        &self,
        value: NewType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(Type::NewTypeInstance))
    }

    fn type_form_argument(&self, value: TypeFormType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(value.type_argument(self.db))
    }

    fn normalize_child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        recursive_normalize_sync(request, env, self, RecursiveNormalizationFacts)
    }

    fn intern_type_form(&self, argument: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(TypeFormType::from_type_expression(self.db, argument))
    }
}
