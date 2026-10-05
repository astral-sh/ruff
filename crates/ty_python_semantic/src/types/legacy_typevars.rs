//! Ordered collection of legacy variables without inspecting their bounds or defaults.

use std::collections::btree_map;
use std::convert::Infallible;

use ruff_python_ast::name::Name;
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::types::constraints::control::{Unrestricted, unrestricted};
use crate::types::constraints::{OwnedConstraintSet, OwnedConstraintTypeCursor};
use crate::types::instance::{
    MaterializedProtocolType, NominalVisitorChildren, Protocol, SynthesizedProtocolType,
};
use crate::types::known_instance::{InternedType, MethodWrapper, UnionTypeInstance};
use crate::types::protocol_class::{
    ProtocolClass, ProtocolInterface, ProtocolInterfaceView, ProtocolMember, ProtocolMemberData,
};
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::signatures::{Parameter, Signature};
use crate::types::tuple::{
    FixedLengthTuple, Tuple, TupleSpec, VariableLengthTuple, VariableSegment,
};
use crate::types::{
    BindingContext, BoundMethodType, BoundTypeVarInstance, CallableType, ClassType, DynamicType,
    EnumComplementType, FindLegacyTypeVarsVisitor, FunctionType, GenericAlias, GenericContext,
    IntersectionType, KnownBoundMethodType, KnownInstanceType, NominalInstanceType,
    PropertyInstanceType, ProtocolInstanceType, RecursiveType, SlotDescriptorType, Specialization,
    SubclassOfInner, SubclassOfType, Type, TypeAliasType, TypeFormType, TypeGuardType, TypeIsType,
    TypeVarKind, TypedDictType, UnfoldResult, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum LegacyTypeVarWork {
    Dispatch,
    Pending { len: usize, capacity: usize },
    Advance,
    Candidate,
    NormalizeParamSpec,
    Insert { len: usize, capacity: usize },
    Dependency,
    Resume,
    Publish,
}

/// These operations retain their existing recursive traversal and cycle-detector scopes.
/// A provider must admit them before invoking either a guard or a source-dependent helper.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum LegacyTypeVarDependency<'db> {
    Guarded {
        key: Type<'db>,
        operation: GuardedLegacyTypeVarDependency<'db>,
    },
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum GuardedLegacyTypeVarDependency<'db> {
    Recursive(RecursiveType<'db>),
    Function(FunctionType<'db>),
    BoundMethod(BoundMethodType<'db>),
    FunctionWrapper(InternedType<'db>),
    BoundMethodWrapper(BoundMethodType<'db>),
    Property(PropertyInstanceType<'db>),
    Slot(SlotDescriptorType<'db>),
    Alias(TypeAliasType<'db>),
}

pub(in crate::types) trait LegacyTypeVarEffects<'db> {
    type Error;

    fn checkpoint(&self, work: LegacyTypeVarWork) -> Result<(), Self::Error>;

    fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error>;

    fn deferred(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        dependency: LegacyTypeVarDependency<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub(in crate::types) enum Pending<'walk, 'db> {
    Type(Type<'db>),
    Types(&'db [Type<'db>]),
    Set(&'db FxOrderSet<Type<'db>>, usize),
    Negative(&'db NegativeIntersectionElements<'db>),
    Tuple(&'db TupleSpec<'db>),
    Specialization(Specialization<'db>),
    Variables(GenericContext<'db>, usize),
    Candidate(BoundTypeVarInstance<'db>),
    Signatures(&'walk [Signature<'db>]),
    Parameters(&'walk [Parameter<'db>]),
    ConstraintTypes(OwnedConstraintTypeCursor<'walk, 'db>),
    ProtocolMembers(btree_map::Iter<'db, Name, ProtocolMemberData<'db>>),
    /// Unresolved member types retain their slots while one type's descendants are visited.
    ProtocolMemberTypes([Option<Type<'db>>; 6], usize),
    /// Ends only the fresh visitor belonging to the completed synthesized member type.
    FinishFreshVisitor,
}

pub(in crate::types) const LEGACY_PENDING_INLINE_CAPACITY: usize = 8;

pub(in crate::types) type LegacyPendingStack<'walk, 'db> =
    SmallVec<[Pending<'walk, 'db>; LEGACY_PENDING_INLINE_CAPACITY]>;

/// Fresh member visitors retained until their pending descendants have completed.
pub(in crate::types) type LegacyVisitorScopes<'db> = Vec<FindLegacyTypeVarsVisitor<'db>>;

/// One stored member slot and the position from which its siblings resume.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct LegacyProtocolTypeStep<'db> {
    pub ty: Option<Type<'db>>,
    pub next: usize,
}

/// The stored signature children visited by legacy-variable collection.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct LegacySignatureChildren<'walk, 'db> {
    pub remaining: &'walk [Signature<'db>],
    pub receiver: Option<&'walk OwnedConstraintSet<'db>>,
    pub parameters: &'walk [Parameter<'db>],
    pub return_type: Type<'db>,
}

/// A parameter annotation and its already evaluated default, in visitation order.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct LegacyParameterChildren<'walk, 'db> {
    pub remaining: &'walk [Parameter<'db>],
    pub annotation: Type<'db>,
    pub default: Option<Type<'db>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyTypeVarOperation {
    NormalizeParamSpec,
    Recursive,
    Function,
    BoundMethod,
    FunctionWrapper,
    BoundMethodWrapper,
    Property,
    Slot,
    Alias,
}

pub(in crate::types) struct LegacyTypeVarFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLegacyTypeVarTraversalEffects)]
    pub(in crate::types) trait LegacyTypeVarTraversalEffects<'db> {
        type Error;
        #[operation(local)]
        async fn checkpoint(&self, work: LegacyTypeVarWork) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_visitor(&self) -> Result<FindLegacyTypeVarsVisitor<'db>, Self::Error>;
        #[operation(local)]
        async fn new_visitor_scopes(&self) -> Result<LegacyVisitorScopes<'db>, Self::Error>;
        #[operation(local)]
        async fn enter_fresh_visitor(&self, scopes: &mut LegacyVisitorScopes<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_fresh_visitor(&self, scopes: &mut LegacyVisitorScopes<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_pending<'walk>(&self) -> Result<LegacyPendingStack<'walk, 'db>, Self::Error> where 'db: 'walk;
        #[operation(local)]
        async fn push_type<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_types<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, types: &'db [Type<'db>]) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_set<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, types: &'db FxOrderSet<Type<'db>>, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_negative<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, types: &'db NegativeIntersectionElements<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_tuple<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, tuple: &'db TupleSpec<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_specialization<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, specialization: Specialization<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_variables<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, context: GenericContext<'db>, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_candidate<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_signatures<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, signatures: &'walk [Signature<'db>]) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_parameters<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, parameters: &'walk [Parameter<'db>]) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_protocol_members<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, members: btree_map::Iter<'db, Name, ProtocolMemberData<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_protocol_types<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, types: [Option<Type<'db>>; 6], index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_finish_fresh_visitor<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn next_protocol_member(&self, members: &mut btree_map::Iter<'db, Name, ProtocolMemberData<'db>>) -> Result<Option<(&'db Name, &'db ProtocolMemberData<'db>)>, Self::Error>;
        #[operation(local)]
        async fn protocol_member_types(&self, name: &'db Name, data: &'db ProtocolMemberData<'db>) -> Result<[Option<Type<'db>>; 6], Self::Error>;
        #[operation(local)]
        async fn protocol_type_step(&self, types: &[Option<Type<'db>>; 6], index: usize) -> Result<Option<LegacyProtocolTypeStep<'db>>, Self::Error>;
        #[operation(source)]
        async fn protocol_members(&self, interface: ProtocolInterface<'db>) -> Result<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>, Self::Error>;
        #[operation(source)]
        async fn materialized_protocol_origin(&self, materialized: MaterializedProtocolType<'db>) -> Result<ProtocolClass<'db>, Self::Error>;
        #[operation(local)]
        async fn protocol_inner(&self, protocol: ProtocolInstanceType<'db>) -> Result<Protocol<'db>, Self::Error>;
        #[operation(local)]
        async fn protocol_class_type(&self, class: ProtocolClass<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn synthesized_interface(&self, protocol: SynthesizedProtocolType<'db>) -> Result<ProtocolInterface<'db>, Self::Error>;
        #[operation(child)]
        async fn enqueue_protocol<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, protocol: ProtocolInstanceType<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_receiver<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, constraints: &'walk OwnedConstraintSet<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn requeue_receiver<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>, cursor: OwnedConstraintTypeCursor<'walk, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn signature_children<'walk>(&self, signatures: &'walk [Signature<'db>]) -> Result<Option<LegacySignatureChildren<'walk, 'db>>, Self::Error>;
        #[operation(local)]
        async fn parameter_children<'walk>(&self, parameters: &'walk [Parameter<'db>]) -> Result<Option<LegacyParameterChildren<'walk, 'db>>, Self::Error>;
        #[operation(local)]
        async fn receiver_step<'walk>(&self, cursor: &mut OwnedConstraintTypeCursor<'walk, 'db>) -> Result<Option<Option<[Type<'db>; 2]>>, Self::Error>;
        #[operation(source)]
        async fn callable_signatures(&self, callable: CallableType<'db>) -> Result<&'db [Signature<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_pending<'walk>(&self, pending: &mut LegacyPendingStack<'walk, 'db>) -> Result<Option<Pending<'walk, 'db>>, Self::Error>;
        #[operation(local)]
        async fn insert_variable(&self, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn unbound_recursive(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(source)]
        async fn variable_binding(&self, variable: BoundTypeVarInstance<'db>) -> Result<BindingContext<'db>, Self::Error>;
        #[operation(source)]
        async fn specialization_tuple(&self, specialization: Specialization<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
        #[operation(source)]
        async fn specialization_types(&self, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(source)]
        async fn context_variable(&self, context: GenericContext<'db>, index: usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(source)]
        async fn intersection_positive(&self, intersection: IntersectionType<'db>) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn intersection_negative(&self, intersection: IntersectionType<'db>) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error>;
        #[operation(source)]
        async fn enum_rest(&self, complement: EnumComplementType<'db>) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn alias_specialization(&self, alias: GenericAlias<'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(source)]
        async fn nominal_children(&self, instance: NominalInstanceType<'db>) -> Result<NominalVisitorChildren<'db>, Self::Error>;
        #[operation(source)]
        async fn type_is_argument(&self, ty: TypeIsType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn type_guard_return(&self, ty: TypeGuardType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn type_form_argument(&self, ty: TypeFormType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_value(&self, ty: UnionTypeInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn interned_type(&self, ty: InternedType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn method_wrapper_type(&self, ty: MethodWrapper<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn normalize_paramspec(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn collect_candidate(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>, binding_context: Option<Definition<'db>>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn collect_with_visitor(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, binding_context: Option<Definition<'db>>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, visitor: &FindLegacyTypeVarsVisitor<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn deferred(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, binding_context: Option<Definition<'db>>, dependency: LegacyTypeVarDependency<'db>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, visitor: &FindLegacyTypeVarsVisitor<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl LegacyTypeVarFacts {

        fn split_types<'db>(&self, types: &'db [Type<'db>]) -> Option<(Type<'db>, &'db [Type<'db>])> { types.split_first().map(|(first, rest)| (*first, rest)) }
        fn has_types(&self, types: &[Type<'_>]) -> bool { !types.is_empty() }
        fn set_element<'db>(&self, types: &'db FxOrderSet<Type<'db>>, index: usize) -> Option<&'db Type<'db>> { types.get_index(index) }
        fn next_index(&self, index: usize) -> usize { index + 1 }
        fn copy_type<'db>(&self, ty: &Type<'db>) -> Type<'db> { *ty }
        fn fixed_elements<'db>(&self, tuple: &'db FixedLengthTuple<Type<'db>>) -> &'db [Type<'db>] { tuple.all_elements() }
        fn suffix_elements<'db>(&self, tuple: &'db VariableLengthTuple<Type<'db>, VariableSegment<'db>>) -> &'db [Type<'db>] { tuple.suffix_elements() }
        fn prefix_elements<'db>(&self, tuple: &'db VariableLengthTuple<Type<'db>, VariableSegment<'db>>) -> &'db [Type<'db>] { tuple.prefix_elements() }
        fn variable_type<'db>(&self, tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>) -> Type<'db> { tuple.variable().tuple_class_type() }
        fn class_type<'db>(&self, class: ClassType<'db>) -> Type<'db> { class.into() }
        fn subclass_inner<'db>(&self, ty: SubclassOfType<'db>) -> SubclassOfInner<'db> { ty.subclass_of() }
        fn same_binding<'db>(&self, binding: BindingContext<'db>, definition: Definition<'db>) -> bool { binding == BindingContext::Definition(definition) }
        fn current_visitor<'a, 'db>(&self, scopes: &'a LegacyVisitorScopes<'db>, root: &'a FindLegacyTypeVarsVisitor<'db>) -> &'a FindLegacyTypeVarsVisitor<'db> { scopes.last().unwrap_or(root) }
    }

    /// Enqueues a protocol's stored specialization or interface without resolving its members.
    #[synchronous(enqueue_protocol_sync)]
    #[capabilities(effects = LegacyTypeVarTraversalEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enqueue_protocol_with_effects<'walk, 'db, E: LegacyTypeVarTraversalEffects<'db>>(
        pending: &mut LegacyPendingStack<'walk, 'db>, protocol: ProtocolInstanceType<'db>, effects: &E,
    ) -> Result<(), E::Error> {
        match effects.protocol_inner(protocol).await? {
            Protocol::FromClass(class) => effects.push_type(pending, effects.protocol_class_type(class).await?).await,
            Protocol::Materialized(materialized) => {
                let class = effects.materialized_protocol_origin(materialized).await?;
                effects.push_type(pending, effects.protocol_class_type(class).await?).await
            }
            Protocol::Synthesized(synthesized) => {
                let interface = effects.synthesized_interface(synthesized).await?;
                let members = effects.protocol_members(interface).await?;
                effects.push_protocol_members(pending, members).await
            }
        }
    }

    #[synchronous(collect_candidate_sync)]
    #[capabilities(effects = LegacyTypeVarTraversalEffects, facts = LegacyTypeVarFacts)]
    #[passive_values(LegacyTypeVarWork::Candidate, LegacyTypeVarWork::NormalizeParamSpec, LegacyTypeVarWork::Resume)]
    pub(in crate::types) async fn collect_candidate_with_effects<'db, E: LegacyTypeVarTraversalEffects<'db>>(
        db: &'db dyn Db, variable: BoundTypeVarInstance<'db>, binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, facts: LegacyTypeVarFacts, effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint(LegacyTypeVarWork::Candidate).await?;
        let kind = effects.variable_kind(variable).await?;
        if !matches!(kind, TypeVarKind::LegacyTypeVar | TypeVarKind::Pep613Alias | TypeVarKind::TypingSelf | TypeVarKind::LegacyTypeVarTuple | TypeVarKind::LegacyParamSpec) {
            return Ok(());
        }
        if let Some(context) = binding_context
            && !facts.same_binding(effects.variable_binding(variable).await?, context)
        { return Ok(()); }
        let variable = if matches!(kind, TypeVarKind::LegacyParamSpec) {
            // A ParamSpec contributes P itself, even when the encountered type was P.args or P.kwargs.
            effects.checkpoint(LegacyTypeVarWork::NormalizeParamSpec).await?;
            let normalized = effects.normalize_paramspec(db, variable).await?;
            effects.checkpoint(LegacyTypeVarWork::Resume).await?;
            normalized
        } else { variable };
        effects.insert_variable(variables, variable).await
    }

    /// Collects legacy variables from `ty` into `variables`.
    ///
    /// Starts a fresh cycle-detector scope, including when the caller collects several bases.
    #[synchronous(find_legacy_typevars_effects_sync)]
    #[capabilities(effects = LegacyTypeVarTraversalEffects)]
    #[passive_values()]
    pub(in crate::types) async fn find_legacy_typevars_with_effects<'db, E: LegacyTypeVarTraversalEffects<'db>>(
        db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, effects: &E,
    ) -> Result<(), E::Error> {
        let visitor = effects.new_visitor().await?;
        effects.collect_with_visitor(db, env, ty, binding_context, variables, &visitor).await
    }

    /// Visits pending types in stored order, adding their legacy variables to `variables`.
    ///
    /// On failure, `variables` is an unfinished accumulator and must not become a generic context.
    #[synchronous(collect_pending_effects_sync)]
    #[capabilities(effects = LegacyTypeVarTraversalEffects, facts = LegacyTypeVarFacts)]
    #[passive_values(LegacyTypeVarWork::Dispatch, LegacyTypeVarWork::Dependency, LegacyTypeVarWork::Resume, LegacyTypeVarWork::Publish, LegacyTypeVarDependency::Guarded, GuardedLegacyTypeVarDependency::Recursive, GuardedLegacyTypeVarDependency::Function, GuardedLegacyTypeVarDependency::BoundMethod, GuardedLegacyTypeVarDependency::FunctionWrapper, GuardedLegacyTypeVarDependency::BoundMethodWrapper, GuardedLegacyTypeVarDependency::Property, GuardedLegacyTypeVarDependency::Slot, GuardedLegacyTypeVarDependency::Alias, Type::TypeVar)]
    pub(in crate::types) async fn collect_pending_with_effects<'walk, 'db, E: LegacyTypeVarTraversalEffects<'db>>(
        db: &'db dyn Db, env: &ProgramEnvironment<'db>, pending: &mut LegacyPendingStack<'walk, 'db>, binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, visitor: &FindLegacyTypeVarsVisitor<'db>, facts: LegacyTypeVarFacts, effects: &E,
    ) -> Result<(), E::Error> {
    let mut scopes = effects.new_visitor_scopes().await?;
    #[cursor_loop]
    while let Some(item) = effects.next_pending(pending).await? {
        let ty = match item {
            Pending::Type(ty) => ty,
            Pending::Types(types) => {
                if let Some((first, rest)) = facts.split_types(types) {
                    if facts.has_types(rest) {
                        effects.push_types(pending, rest).await?;
                    }
                    effects.push_type(pending, first).await?;
                }
                continue;
            }
            Pending::Set(types, index) => {
                if let Some(ty) = facts.set_element(types, index) {
                    effects.push_set(pending, types, facts.next_index(index)).await?;
                    effects.push_type(pending, facts.copy_type(ty)).await?;
                }
                continue;
            }
            Pending::Negative(types) => {
                match types {
                    NegativeIntersectionElements::Empty => {}
                    NegativeIntersectionElements::Single(ty) => {
                        effects.push_type(pending, facts.copy_type(ty)).await?;
                    }
                    NegativeIntersectionElements::Multiple(types) => {
                        effects.push_set(pending, types, 0).await?;
                    }
                }
                continue;
            }
            Pending::Tuple(tuple) => {
                match tuple {
                    Tuple::Fixed(tuple) => {
                        effects.push_types(pending, facts.fixed_elements(tuple)).await?;
                    }
                    Tuple::Variable(tuple) => {
                        effects.push_types(pending, facts.suffix_elements(tuple)).await?;
                        effects.push_type(pending, facts.variable_type(tuple)).await?;
                        effects.push_types(pending, facts.prefix_elements(tuple)).await?;
                    }
                }
                continue;
            }
            Pending::Specialization(specialization) => {
                if let Some(tuple) = effects.specialization_tuple(specialization).await? {
                    effects.push_tuple(pending, tuple).await?;
                } else {
                    effects.push_types(pending, effects.specialization_types(specialization).await?).await?;
                }
                continue;
            }
            Pending::Variables(context, index) => {
                if let Some(variable) = effects.context_variable(context, index).await? {
                    effects.push_variables(pending, context, facts.next_index(index)).await?;
                    effects.push_candidate(pending, variable).await?;
                }
                continue;
            }
            Pending::Signatures(signatures) => {
                if let Some(children) = effects.signature_children(signatures).await? {
                    effects.push_signatures(pending, children.remaining).await?;
                    effects.push_type(pending, children.return_type).await?;
                    effects.push_parameters(pending, children.parameters).await?;
                    if let Some(receiver) = children.receiver {
                        effects.push_receiver(pending, receiver).await?;
                    }
                }
                continue;
            }
            Pending::Parameters(parameters) => {
                if let Some(children) = effects.parameter_children(parameters).await? {
                    effects.push_parameters(pending, children.remaining).await?;
                    if let Some(default) = children.default {
                        effects.push_type(pending, default).await?;
                    }
                    effects.push_type(pending, children.annotation).await?;
                }
                continue;
            }
            Pending::ConstraintTypes(mut cursor) => {
                if let Some(types) = effects.receiver_step(&mut cursor).await? {
                    effects.requeue_receiver(pending, cursor).await?;
                    if let Some([first, second]) = types {
                        effects.push_type(pending, second).await?;
                        effects.push_type(pending, first).await?;
                    }
                }
                continue;
            }
            Pending::ProtocolMembers(mut members) => {
                if let Some((name, data)) = effects.next_protocol_member(&mut members).await? {
                    effects.push_protocol_members(pending, members).await?;
                    let types = effects.protocol_member_types(name, data).await?;
                    effects.push_protocol_types(pending, types, 0).await?;
                }
                continue;
            }
            Pending::ProtocolMemberTypes(types, index) => {
                if let Some(step) = effects.protocol_type_step(&types, index).await? {
                    effects.push_protocol_types(pending, types, step.next).await?;
                    if let Some(ty) = step.ty {
                        effects.enter_fresh_visitor(&mut scopes).await?;
                        effects.push_finish_fresh_visitor(pending).await?;
                        effects.push_type(pending, ty).await?;
                    }
                }
                continue;
            }
            Pending::FinishFreshVisitor => {
                effects.finish_fresh_visitor(&mut scopes).await?;
                continue;
            }
            Pending::Candidate(variable) => {
                effects.collect_candidate(db, variable, binding_context, variables).await?;
                continue;
            }
        };
        effects.checkpoint(LegacyTypeVarWork::Dispatch).await?;
        let dependency = match ty {
            Type::RecursiveVar(_) => {
                effects.unbound_recursive().await?; None
            }
            Type::TypeVar(variable) => {
                effects.collect_candidate(db, variable, binding_context, variables).await?;
                None
            }
            Type::Recursive(recursive) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::Recursive(recursive),
            }),
            Type::FunctionLiteral(function) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::Function(function),
            }),
            Type::BoundMethod(method) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::BoundMethod(method),
            }),
            Type::KnownBoundMethod(
                KnownBoundMethodType::FunctionTypeDunderGet(function)
                | KnownBoundMethodType::DunderCall(function),
            ) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::FunctionWrapper(function),
            }),
            Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(method)) => {
                Some(LegacyTypeVarDependency::Guarded {
                    key: ty,
                    operation: GuardedLegacyTypeVarDependency::BoundMethodWrapper(method),
                })
            }
            Type::KnownBoundMethod(
                KnownBoundMethodType::PropertyDunderGet(property)
                | KnownBoundMethodType::PropertyDunderSet(property)
                | KnownBoundMethodType::PropertyDunderDelete(property),
            )
            | Type::PropertyInstance(property) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::Property(property),
            }),
            Type::Callable(callable) => {
                effects.push_signatures(pending, effects.callable_signatures(callable).await?).await?;
                None
            },
            Type::SlotDescriptor(descriptor) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::Slot(descriptor),
            }),
            Type::Union(union) => {
                effects.push_types(pending, effects.union_elements(union).await?).await?;
                None
            }
            Type::Intersection(intersection) => {
                effects.push_negative(pending, effects.intersection_negative(intersection).await?).await?;
                effects.push_set(pending, effects.intersection_positive(intersection).await?, 0).await?;
                None
            }
            Type::EnumComplement(complement) => {
                effects.push_set(pending, effects.enum_rest(complement).await?, 0).await?;
                None
            }
            Type::GenericAlias(alias) => {
                effects.push_specialization(pending, effects.alias_specialization(alias).await?).await?;
                None
            }
            Type::NominalInstance(instance) => {
                match effects.nominal_children(instance).await? {
                    NominalVisitorChildren::None => {}
                    NominalVisitorChildren::Class(class) => {
                        effects.push_type(pending, class).await?;
                    }
                    NominalVisitorChildren::Tuple(tuple) => {
                        effects.push_tuple(pending, tuple).await?;
                    }
                }
                None
            }
            Type::ProtocolInstance(instance) => {
                effects.enqueue_protocol(pending, instance).await?;
                None
            }
            Type::TypedDict(TypedDictType::Class(class)) => {
                effects.push_type(pending, facts.class_type(class)).await?;
                None
            }
            // Synthesized schemas can contain type variables, but their internal narrowing and
            // update constraints inherit those variables from an existing generic context.
            Type::TypedDict(TypedDictType::Synthesized(_)) => None,
            Type::NewTypeInstance(_) => {
                // A newtype can never be constructed from an unspecialized generic class, so it is
                // impossible to find legacy typevars in a newtype instance or its underlying class.
                None
            }
            Type::SubclassOf(subclass) => match facts.subclass_inner(subclass) {
                SubclassOfInner::Dynamic(_) => None,
                SubclassOfInner::Class(class) => {
                    effects.push_type(pending, facts.class_type(class)).await?;
                    None
                }
                SubclassOfInner::Protocol(protocol) => {
                    effects.enqueue_protocol(pending, protocol).await?;
                    None
                }
                SubclassOfInner::TypeVar(variable) => {
                    effects.push_type(pending, Type::TypeVar(variable)).await?;
                    None
                }
            },
            Type::TypeIs(type_is) => {
                effects.push_type(pending, effects.type_is_argument(type_is).await?).await?;
                None
            }
            Type::TypeGuard(type_guard) => {
                effects.push_type(pending, effects.type_guard_return(type_guard).await?).await?;
                None
            }
            Type::TypeForm(typeform) => {
                effects.push_type(pending, effects.type_form_argument(typeform).await?).await?;
                None
            }
            Type::TypeAlias(alias) => Some(LegacyTypeVarDependency::Guarded {
                key: ty,
                operation: GuardedLegacyTypeVarDependency::Alias(alias),
            }),
            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::UnionType(instance) => {
                    if let Some(union) = effects.union_value(instance).await? {
                        effects.push_type(pending, union).await?;
                    }
                    None
                }
                KnownInstanceType::Annotated(ty)
                | KnownInstanceType::TypeGenericAlias(ty)
                | KnownInstanceType::LiteralStringAlias(ty) => {
                    effects.push_type(pending, effects.interned_type(ty).await?).await?;
                    None
                }
                KnownInstanceType::Callable(callable) => {
                    effects.push_signatures(pending, effects.callable_signatures(callable).await?).await?;
                    None
                }
                KnownInstanceType::MethodWrapper(wrapper) => {
                    effects.push_type(pending, effects.method_wrapper_type(wrapper).await?).await?;
                    None
                }
                KnownInstanceType::SubscriptedProtocol(_)
                | KnownInstanceType::SubscriptedGeneric(_)
                | KnownInstanceType::TypeVar(_)
                | KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::Deprecated(_)
                | KnownInstanceType::Field(_)
                | KnownInstanceType::ConstraintSet(_)
                | KnownInstanceType::ConstraintSetSolution(_)
                | KnownInstanceType::GenericContext(_)
                | KnownInstanceType::Specialization(_)
                | KnownInstanceType::Literal(_)
                | KnownInstanceType::NamedTupleSpec(_)
                | KnownInstanceType::NewType(_)
                | KnownInstanceType::Sentinel(_)
                | KnownInstanceType::Range { .. }
                | KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::FunctoolsPartialCall(_) => {
                    // TODO: For some of these, we may need to try to find legacy typevars in inner types.
                    None
                }
            },
            Type::Dynamic(DynamicType::UnknownGeneric(context)) => {
                effects.push_variables(pending, context, 0).await?;
                None
            }
            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::LiteralValue(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_) => None,
        };
        if let Some(dependency) = dependency {
            effects.checkpoint(LegacyTypeVarWork::Dependency).await?;
            effects.deferred(db, env, binding_context, dependency, variables, facts.current_visitor(&scopes, visitor)).await?;
            effects.checkpoint(LegacyTypeVarWork::Resume).await?;
        }
    }
    effects.checkpoint(LegacyTypeVarWork::Publish).await?;
    Ok(())
    }
}

/// Collects a type's legacy variables using an admitted stack and the caller's cycle detector.
pub(in crate::types) async fn collect_with_visitor_with_effects<
    'db,
    E: LegacyTypeVarTraversalEffects<'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    binding_context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
    facts: LegacyTypeVarFacts,
    effects: &E,
) -> Result<(), E::Error> {
    #[cfg(all(test, feature = "experimental-analysis"))]
    let _lifetime = crate::types::infer::legacy_callable_observations::collector_started();
    let mut pending = effects.new_pending().await?;
    effects.push_type(&mut pending, ty).await?;
    #[cfg(all(test, feature = "experimental-analysis"))]
    let _scope_lifetime = crate::types::infer::legacy_callable_observations::scopes_started();
    collect_pending_with_effects(
        db,
        env,
        &mut pending,
        binding_context,
        variables,
        visitor,
        facts,
        effects,
    )
    .await
}

fn collect_with_visitor_effects_sync<'db, E: SynchronousLegacyTypeVarTraversalEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    binding_context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
    facts: LegacyTypeVarFacts,
    effects: &E,
) -> Result<(), E::Error> {
    let mut pending = effects.new_pending()?;
    effects.push_type(&mut pending, ty)?;
    #[cfg(all(test, feature = "experimental-analysis"))]
    let _scope_lifetime = crate::types::infer::legacy_callable_observations::scopes_started();
    collect_pending_effects_sync(
        db,
        env,
        &mut pending,
        binding_context,
        variables,
        visitor,
        facts,
        effects,
    )
}

/// Adds legacy variables from the supplied signatures to `variables` using the caller's visitor.
/// Uses the stored-child order shared with [`collect_with_visitor_with_effects`].
pub(in crate::types) fn collect_signature_legacy_typevars<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    signatures: &[Signature<'db>],
    binding_context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
) {
    let effects = InlineTraversal {
        db,
        effects: &InlineLegacyTypeVarEffects,
    };
    let result = (|| {
        let mut pending = effects.new_pending()?;
        effects.push_signatures(&mut pending, signatures)?;
        #[cfg(all(test, feature = "experimental-analysis"))]
        let _scope_lifetime = crate::types::infer::legacy_callable_observations::scopes_started();
        collect_pending_effects_sync(
            db,
            env,
            &mut pending,
            binding_context,
            variables,
            visitor,
            LegacyTypeVarFacts,
            &effects,
        )
    })();
    match result {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

pub(in crate::types) fn find_legacy_typevars_with<'db, E: LegacyTypeVarEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    binding_context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    effects: &E,
) -> Result<(), E::Error> {
    find_legacy_typevars_effects_sync(
        db,
        env,
        ty,
        binding_context,
        variables,
        &InlineTraversal { db, effects },
    )
}

pub(in crate::types) fn collect_with_visitor<'db, E: LegacyTypeVarEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    binding_context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    collect_with_visitor_effects_sync(
        db,
        env,
        ty,
        binding_context,
        variables,
        visitor,
        LegacyTypeVarFacts,
        &InlineTraversal { db, effects },
    )
}

struct InlineTraversal<'db, 'effects, E> {
    db: &'db dyn Db,
    effects: &'effects E,
}

impl<'db, E: LegacyTypeVarEffects<'db>> InlineTraversal<'db, '_, E> {
    fn push_legacy_pending<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        make: impl FnOnce() -> Pending<'walk, 'db>,
    ) -> Result<(), E::Error> {
        self.effects.checkpoint(LegacyTypeVarWork::Pending {
            len: pending.len(),
            capacity: pending.capacity(),
        })?;
        pending.push(make());
        Ok(())
    }
}

impl<'db, E: LegacyTypeVarEffects<'db>> SynchronousLegacyTypeVarTraversalEffects<'db>
    for InlineTraversal<'db, '_, E>
{
    type Error = E::Error;
    fn checkpoint(&self, work: LegacyTypeVarWork) -> Result<(), Self::Error> {
        self.effects.checkpoint(work)
    }
    fn new_visitor(&self) -> Result<FindLegacyTypeVarsVisitor<'db>, Self::Error> {
        Ok(FindLegacyTypeVarsVisitor::default())
    }
    fn new_visitor_scopes(&self) -> Result<LegacyVisitorScopes<'db>, Self::Error> {
        Ok(Vec::new())
    }
    fn enter_fresh_visitor(
        &self,
        scopes: &mut LegacyVisitorScopes<'db>,
    ) -> Result<(), Self::Error> {
        scopes.push(self.new_visitor()?);
        Ok(())
    }
    fn finish_fresh_visitor(
        &self,
        scopes: &mut LegacyVisitorScopes<'db>,
    ) -> Result<(), Self::Error> {
        scopes.pop();
        Ok(())
    }
    fn new_pending<'walk>(&self) -> Result<LegacyPendingStack<'walk, 'db>, Self::Error>
    where
        'db: 'walk,
    {
        Ok(SmallVec::new())
    }
    fn push_type<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Type(ty))
    }
    fn push_types<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: &'db [Type<'db>],
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Types(types))
    }
    fn push_set<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: &'db FxOrderSet<Type<'db>>,
        index: usize,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Set(types, index))
    }
    fn push_negative<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: &'db NegativeIntersectionElements<'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Negative(types))
    }
    fn push_tuple<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        tuple: &'db TupleSpec<'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Tuple(tuple))
    }
    fn push_specialization<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        specialization: Specialization<'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Specialization(specialization))
    }
    fn push_variables<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        context: GenericContext<'db>,
        index: usize,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Variables(context, index))
    }
    fn push_candidate<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Candidate(variable))
    }
    fn push_signatures<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        signatures: &'walk [Signature<'db>],
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Signatures(signatures))
    }
    fn push_parameters<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        parameters: &'walk [Parameter<'db>],
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::Parameters(parameters))
    }
    fn push_protocol_members<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        members: btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::ProtocolMembers(members))
    }
    fn push_protocol_types<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: [Option<Type<'db>>; 6],
        index: usize,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::ProtocolMemberTypes(types, index))
    }
    fn push_finish_fresh_visitor<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::FinishFreshVisitor)
    }
    fn next_protocol_member(
        &self,
        members: &mut btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> Result<Option<(&'db Name, &'db ProtocolMemberData<'db>)>, Self::Error> {
        Ok(members.next())
    }
    fn protocol_member_types(
        &self,
        name: &'db Name,
        data: &'db ProtocolMemberData<'db>,
    ) -> Result<[Option<Type<'db>>; 6], Self::Error> {
        Ok(ProtocolMember::from_stored(name, data, None).stored_types_for_visitor())
    }
    fn protocol_type_step(
        &self,
        types: &[Option<Type<'db>>; 6],
        index: usize,
    ) -> Result<Option<LegacyProtocolTypeStep<'db>>, Self::Error> {
        Ok(types.get(index).map(|ty| LegacyProtocolTypeStep {
            ty: *ty,
            next: index + 1,
        }))
    }
    fn protocol_members(
        &self,
        interface: ProtocolInterface<'db>,
    ) -> Result<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>, Self::Error> {
        Ok(ProtocolInterfaceView::new(interface, None)
            .members_with_fields(salsa::FieldReads::new(self.db)))
    }
    fn materialized_protocol_origin(
        &self,
        materialized: MaterializedProtocolType<'db>,
    ) -> Result<ProtocolClass<'db>, Self::Error> {
        Ok(materialized.origin(self.db))
    }
    fn enqueue_protocol<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<(), Self::Error> {
        enqueue_protocol_sync(pending, protocol, self)
    }
    fn protocol_inner(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<Protocol<'db>, Self::Error> {
        Ok(protocol.inner)
    }
    fn protocol_class_type(&self, class: ProtocolClass<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::from(class))
    }
    fn synthesized_interface(
        &self,
        protocol: SynthesizedProtocolType<'db>,
    ) -> Result<ProtocolInterface<'db>, Self::Error> {
        Ok(protocol.interface())
    }
    fn push_receiver<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        constraints: &'walk OwnedConstraintSet<'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || {
            Pending::ConstraintTypes(OwnedConstraintTypeCursor::new(constraints))
        })
    }
    fn requeue_receiver<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        cursor: OwnedConstraintTypeCursor<'walk, 'db>,
    ) -> Result<(), Self::Error> {
        self.push_legacy_pending(pending, || Pending::ConstraintTypes(cursor))
    }
    fn next_pending<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
    ) -> Result<Option<Pending<'walk, 'db>>, Self::Error> {
        self.effects.checkpoint(LegacyTypeVarWork::Advance)?;
        Ok(pending.pop())
    }
    fn signature_children<'walk>(
        &self,
        signatures: &'walk [Signature<'db>],
    ) -> Result<Option<LegacySignatureChildren<'walk, 'db>>, Self::Error> {
        Ok(signatures
            .split_first()
            .map(|(signature, remaining)| LegacySignatureChildren {
                remaining,
                receiver: signature.receiver_constraints(),
                parameters: signature.parameters().as_slice(),
                return_type: signature.return_ty,
            }))
    }
    fn parameter_children<'walk>(
        &self,
        parameters: &'walk [Parameter<'db>],
    ) -> Result<Option<LegacyParameterChildren<'walk, 'db>>, Self::Error> {
        Ok(parameters
            .split_first()
            .map(|(parameter, remaining)| LegacyParameterChildren {
                remaining,
                annotation: parameter.annotated_type(),
                default: parameter.eager_default_type(),
            }))
    }
    fn receiver_step<'walk>(
        &self,
        cursor: &mut OwnedConstraintTypeCursor<'walk, 'db>,
    ) -> Result<Option<Option<[Type<'db>; 2]>>, Self::Error> {
        Ok(unrestricted(cursor.next_with(&mut Unrestricted)))
    }
    fn callable_signatures(
        &self,
        callable: CallableType<'db>,
    ) -> Result<&'db [Signature<'db>], Self::Error> {
        Ok(&callable.signatures(self.db).overloads)
    }
    fn insert_variable(
        &self,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        self.effects.checkpoint(LegacyTypeVarWork::Insert {
            len: variables.len(),
            capacity: variables.capacity(),
        })?;
        variables.insert(variable);
        Ok(())
    }
    fn unbound_recursive(&self) -> Result<(), Self::Error> {
        unreachable!("semantic operation on an unbound recursive variable")
    }
    fn variable_kind(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarKind, Self::Error> {
        Ok(variable.typevar(self.db).kind(self.db))
    }
    fn variable_binding(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BindingContext<'db>, Self::Error> {
        Ok(variable.binding_context(self.db))
    }
    fn specialization_tuple(
        &self,
        ty: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        Ok(ty.tuple(self.db))
    }
    fn specialization_types(
        &self,
        ty: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(ty.types(self.db))
    }
    fn context_variable(
        &self,
        ty: GenericContext<'db>,
        index: usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(ty.variable_at(self.db, index))
    }
    fn union_elements(&self, ty: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(ty.elements(self.db))
    }
    fn intersection_positive(
        &self,
        ty: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        Ok(ty.positive(self.db))
    }
    fn intersection_negative(
        &self,
        ty: IntersectionType<'db>,
    ) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error> {
        Ok(ty.negative(self.db))
    }
    fn enum_rest(
        &self,
        ty: EnumComplementType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        Ok(ty.rest(self.db))
    }
    fn alias_specialization(
        &self,
        ty: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(ty.specialization(self.db))
    }
    fn nominal_children(
        &self,
        ty: NominalInstanceType<'db>,
    ) -> Result<NominalVisitorChildren<'db>, Self::Error> {
        Ok(ty.children_for_visitor(self.db))
    }
    fn type_is_argument(&self, ty: TypeIsType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.type_argument(self.db))
    }
    fn type_guard_return(&self, ty: TypeGuardType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.return_type(self.db))
    }
    fn type_form_argument(&self, ty: TypeFormType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.type_argument(self.db))
    }
    fn union_value(&self, ty: UnionTypeInstance<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty.union_type(self.db).as_ref().ok().copied())
    }
    fn interned_type(&self, ty: InternedType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.inner(self.db))
    }
    fn method_wrapper_type(&self, ty: MethodWrapper<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.wrapped(self.db))
    }
    fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error> {
        self.effects.normalize_paramspec(db, variable)
    }
    fn collect_candidate(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Self::Error> {
        collect_candidate_sync(
            db,
            variable,
            binding_context,
            variables,
            LegacyTypeVarFacts,
            self,
        )
    }
    fn collect_with_visitor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), Self::Error> {
        collect_with_visitor_effects_sync(
            db,
            env,
            ty,
            binding_context,
            variables,
            visitor,
            LegacyTypeVarFacts,
            self,
        )
    }
    fn deferred(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        dependency: LegacyTypeVarDependency<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), Self::Error> {
        self.effects
            .deferred(db, env, binding_context, dependency, variables, visitor)
    }
}

pub(in crate::types) struct InlineLegacyTypeVarEffects;

impl<'db> LegacyTypeVarEffects<'db> for InlineLegacyTypeVarEffects {
    type Error = Infallible;

    fn checkpoint(&self, _work: LegacyTypeVarWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(variable.without_paramspec_attr(db))
    }

    fn deferred(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        dependency: LegacyTypeVarDependency<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), Infallible> {
        match dependency {
            LegacyTypeVarDependency::Guarded { key, operation } => visitor.visit(db, key, || {
                match operation {
                    GuardedLegacyTypeVarDependency::Recursive(recursive) => {
                        // A parameter can occur only in recursive arguments, so unfolding the body
                        // alone may never reach it before the cycle detector stops the traversal.
                        if let Some(arguments) = recursive.arguments(db) {
                            arguments.find_legacy_typevars_impl(
                                db,
                                env,
                                binding_context,
                                variables,
                                visitor,
                            );
                        }
                        if let UnfoldResult::Unfolded(unfolded) = recursive.unfold(db, env) {
                            unfolded.find_legacy_typevars_impl(
                                db,
                                env,
                                binding_context,
                                variables,
                                visitor,
                            );
                        }
                    }
                    GuardedLegacyTypeVarDependency::Function(function) => {
                        function.find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                    GuardedLegacyTypeVarDependency::BoundMethod(method) => {
                        method.self_instance(db).find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                        method.func(db).find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                    GuardedLegacyTypeVarDependency::FunctionWrapper(function) => {
                        function.inner(db).find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                    GuardedLegacyTypeVarDependency::BoundMethodWrapper(method) => {
                        // The wrapper's guard and the wrapped method's guard have distinct keys.
                        Type::BoundMethod(method).find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                    GuardedLegacyTypeVarDependency::Property(property) => {
                        property.find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                    GuardedLegacyTypeVarDependency::Slot(descriptor) => {
                        descriptor.value_type(db).find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                    GuardedLegacyTypeVarDependency::Alias(alias) => {
                        alias.value_type(db).find_legacy_typevars_impl(
                            db,
                            env,
                            binding_context,
                            variables,
                            visitor,
                        );
                    }
                }
            }),
        }
        Ok(())
    }
}
