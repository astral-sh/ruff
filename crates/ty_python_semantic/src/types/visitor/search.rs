//! Ordered type search with explicit pending traversal state.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod binding_tests;

use std::collections::btree_map;
use std::convert::Infallible;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use super::{NonAtomicType, TypeCollector, TypeKind, TypeSearchMode, admit_type_walk_access_with};
use crate::types::class::{NamedTupleField, NamedTupleSpec};
use crate::types::constraints::control::{
    AllocationKind, TddControl, TddError, TddWork, reserve_smallvec, sequence_growth,
};
use crate::types::constraints::{OwnedConstraintTypeCursor, TypeVarSolution};
use crate::types::function::FunctionType;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::instance::{NominalVisitorChildren, ProtocolVisitorChildren};
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedConstraintSetSolution, InternedType,
    MethodWrapper, UnionTypeInstance,
};
use crate::types::newtype::{NewType, NewTypeBase};
use crate::types::protocol_class::{ProtocolInterfaceView, ProtocolMember, ProtocolMemberData};
use crate::types::set_theoretic::{
    NegativeIntersectionElements, NegativeIntersectionElementsIterator,
};
use crate::types::tuple::{Tuple, TupleSpec, VariableSegment};
use crate::types::typed_dict::{
    SynthesizedTypedDictType, TypedDictField, TypedDictOpenness, TypedDictSchema,
};
use crate::types::typevar::{TypeVarBoundOrConstraints, TypeVarConstraints, TypeVarInstance};
use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableSignature, CallableType,
    ClassType, EnumComplementType, GenericAlias, IntersectionType, KnownBoundMethodType,
    KnownInstanceType, LiteralValueType, LiteralValueTypeKind, MaterializationKind,
    NominalInstanceType, Parameter, PropertyInstanceClass, PropertyInstanceType,
    ProtocolInstanceType, RecursiveType, Signature, SlotDescriptorType, Type, TypeAliasType,
    TypeFormType, TypeGuardType, TypeIsType, TypedDictType, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

mod effects;
pub(in crate::types) use effects::{OrdinaryTypeWalk, TypeWalkWork};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SearchWork {
    PendingFrame { held: usize },
    Advance,
    Predicate,
    RememberType,
    Semantic(SearchOperation),
}

/// Operations that read stored fields, infer source, or process generated types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchOperation {
    StoredField(TypeWalkFieldOperation),
    ProtocolInterface,
    ProtocolMember,
    TypeVarBounds,
    TypeVarDefault,
    AliasValue,
    RecursiveUnfold,
    TypedDictItems,
    TypedDictOpenness,
    NewTypeBase,
    NewTypeInstance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeWalkFieldOperation {
    UnionElements,
    IntersectionPositive,
    IntersectionNegative,
    EnumRest,
    FunctionSignature,
    FunctionImplementations,
    CallableSignatures,
    MethodFunc,
    MethodSelf,
    MethodReceiver,
    BoundSuperChildren,
    AliasSpecialization,
    SpecializationContext,
    SpecializationTypes,
    SpecializationTuple,
    ContextVariable,
    BoundTypeVar,
    EagerTypeVarBounds,
    EagerTypeVarDefault,
    ConstraintElements,
    NominalChildren,
    ProtocolChildren,
    PropertyClass,
    PropertyGetter,
    PropertySetter,
    PropertyDeleter,
    SlotValue,
    TypeIsArgument,
    TypeGuardReturn,
    TypeFormArgument,
    AliasArguments,
    RecursiveArguments,
    InterfaceMembers,
    SynthesizedTypedDictItems,
    SynthesizedTypedDictOpenness,
    EagerNewTypeBase,
    SolutionBindings,
    FieldDefault,
    FieldConverter,
    UnionValue,
    InternedType,
    NamedTupleFields,
    PartialCallable,
    MethodWrapperType,
}

pub(in crate::types) trait SearchControl {
    type Error;

    fn admit(&mut self, work: SearchWork) -> Result<(), Self::Error>;
}

pub(in crate::types) struct Unrestricted;

impl SearchControl for Unrestricted {
    type Error = Infallible;

    fn admit(&mut self, _: SearchWork) -> Result<(), Infallible> {
        Ok(())
    }
}

// Iterator frames borrow stored payloads. They never own other frames, so abandoning a search
// drops its pending work without a recursive chain of destructors.
pub(in crate::types) enum WalkAction<'db> {
    SkippedLazy,
    EndScope,
    Visit(Type<'db>),
    Expand(NonAtomicType<'db>),
    ExitDepth {
        ty: Type<'db>,
        previous_depth: u16,
    },
    Types(&'db [Type<'db>]),
    StoredTypes(StoredTypeSequence<'db>),
    ConstraintTypes(OwnedConstraintTypeCursor<'db, 'db>),
    Signatures(&'db [Signature<'db>]),
    Parameters(&'db [Parameter<'db>]),
    GenericContext {
        context: GenericContext<'db>,
        index: usize,
    },
    SpecializationTypes(Specialization<'db>),
    TypeVarBounds(TypeVarInstance<'db>),
    TypeVarDefault(TypeVarInstance<'db>),
    FunctionImplementations(FunctionType<'db>),
    Callables(&'db [CallableType<'db>]),
    TypeAliasValue(TypeAliasType<'db>),
    ProtocolInterface(ProtocolInterfaceView<'db>),
    ProtocolMembers {
        members: std::collections::btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
        materialization: Option<MaterializationKind>,
    },
    ProtocolMember {
        member: ProtocolMember<'db, 'db>,
        materialized: bool,
    },
    TypedDictFields(TypedDictType<'db>),
    TypedDictExtra(TypedDictType<'db>),
    NewTypeBase(NewType<'db>),
    FieldConverter(FieldInstance<'db>),
}

pub(in crate::types) struct TypeWalkCursor<'db> {
    pub(in crate::types) pending: SmallVec<[WalkAction<'db>; 8]>,
}

/// The children selected by a search visit before consulting its seen set.
#[derive(Clone, Copy, Debug)]
enum TypeSearchContinuation<'db> {
    Expand(NonAtomicType<'db>),
    /// An alias with no arguments still enters the seen set.
    AliasArguments(Option<Specialization<'db>>),
}

/// A complete type key and the children to schedule if the key has not been seen.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct TypeSearchDescent<'db> {
    ty: Type<'db>,
    continuation: TypeSearchContinuation<'db>,
}

/// A visit's result and any seen-set work still needed after its predicate did not match.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum TypeSearchDecision<'db, T> {
    /// Finishes this visit; the caller must still advance any existing pending frames.
    Finished(T),
    Descend(T, TypeSearchDescent<'db>),
}

pub(in crate::types) enum TypeWalkEvent<'db> {
    Expand(NonAtomicType<'db>),
    EndScope,
    Visit(Type<'db>),
    ExitDepth { ty: Type<'db>, previous_depth: u16 },
    SkippedLazy,
}
pub(in crate::types) enum StoredTypeSequence<'db> {
    Ordered(ordermap::set::Iter<'db, Type<'db>>),
    Negative(NegativeIntersectionElementsIterator<'db, 'db>),
    SolutionBindings(std::slice::Iter<'db, TypeVarSolution<'db>>),
    NamedTupleFields(std::slice::Iter<'db, NamedTupleField<'db>>),
    TypedDictFields(std::collections::btree_map::Values<'db, Name, TypedDictField<'db>>),
}
impl<'db> StoredTypeSequence<'db> {
    pub(in crate::types) fn next_type(&mut self) -> Option<Type<'db>> {
        match self {
            Self::Ordered(types) => types.next().copied(),
            Self::Negative(types) => types.next().copied(),
            Self::SolutionBindings(bindings) => bindings.next().map(|binding| binding.solution),
            Self::NamedTupleFields(fields) => fields.next().map(|field| field.ty),
            Self::TypedDictFields(fields) => fields.next().map(|field| field.declared_ty),
        }
    }
}
#[derive(Clone, Copy)]
pub(in crate::types) struct TypeWalkPolicy {
    lazy: bool,
    expand_typevars: bool,
    declarations: bool,
    alias_arguments: bool,
    alias_stored_types_only: bool,
    report_skipped: bool,
    report_boundaries: bool,
}
impl TypeWalkPolicy {
    pub(in crate::types) fn search(mode: TypeSearchMode) -> Self {
        Self {
            lazy: mode.should_visit_lazy_type_attributes(),
            expand_typevars: true,
            declarations: true,
            alias_arguments: mode.should_visit_alias_arguments(),
            alias_stored_types_only: false,
            report_skipped: false,
            report_boundaries: false,
        }
    }
    pub(in crate::types) fn support() -> Self {
        // Declaration bounds, constraints, and defaults are not occurrences in the
        // constraint itself and must not contribute to its support.
        Self {
            lazy: false,
            expand_typevars: false,
            declarations: false,
            alias_arguments: false,
            alias_stored_types_only: true,
            report_skipped: true,
            report_boundaries: false,
        }
    }
    pub(in crate::types) fn eligibility() -> Self {
        Self {
            lazy: false,
            expand_typevars: false,
            declarations: true,
            alias_arguments: false,
            alias_stored_types_only: false,
            report_skipped: false,
            report_boundaries: false,
        }
    }
    /// Exposes expansion boundaries so occurrence collectors can retain their enclosing context.
    /// Alias and recursive children are selected by the collector; other lazy fields stay disabled.
    /// Generic-context declarations emit bound-variable occurrences without traversing their bounds or defaults.
    pub(in crate::types) fn locations() -> Self {
        Self { report_boundaries: true, ..Self::eligibility() }
    }

    /// Emits bound occurrences without their attributes, including generic declarations.
    /// A generic class base contributes its stored arguments rather than its formal parameters.
    /// The collector consumes alias expansion events without visiting their lazy values.
    pub(in crate::types) fn base_variables() -> Self {
        Self { alias_stored_types_only: true, ..Self::locations() }
    }

    pub(in crate::types) fn depth() -> Self {
        Self::eligibility()
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct TypeWalkFacts;

#[cfg(test)]
type TypeWalkVisit = crate::types::constructor::expansion_probe::search_observation::Visit;
#[cfg(not(test))]
type TypeWalkVisit = ();

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SyncTypeWalkEffects)]
pub(in crate::types) trait TypeWalkEffects<'db> {
    type Error;
    #[operation(source)]
    async fn union_elements(&mut self, ty: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
    #[operation(source)]
    async fn intersection_positive(&mut self, ty: IntersectionType<'db>) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn intersection_negative(&mut self, ty: IntersectionType<'db>) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error>;
    #[operation(source)]
    async fn enum_rest(&mut self, ty: EnumComplementType<'db>) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn function_signature(&mut self, ty: FunctionType<'db>) -> Result<Option<&'db CallableSignature<'db>>, Self::Error>;
    #[operation(source)]
    async fn function_implementations(&mut self, ty: FunctionType<'db>) -> Result<Option<&'db [CallableType<'db>]>, Self::Error>;
    #[operation(source)]
    async fn callable_signatures(&mut self, ty: CallableType<'db>) -> Result<&'db CallableSignature<'db>, Self::Error>;
    #[operation(source)]
    async fn method_func(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn method_self(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn method_receiver(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn bound_super_children(&mut self, ty: BoundSuperType<'db>) -> Result<[Option<Type<'db>>; 3], Self::Error>;
    #[operation(source)]
    async fn alias_specialization(&mut self, ty: GenericAlias<'db>) -> Result<Specialization<'db>, Self::Error>;
    #[operation(source)]
    async fn specialization_context(&mut self, ty: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
    #[operation(source)]
    async fn specialization_types(&mut self, ty: Specialization<'db>) -> Result<&'db [Type<'db>], Self::Error>;
    #[operation(source)]
    async fn specialization_tuple(&mut self, ty: Specialization<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
    #[operation(source)]
    async fn context_variable(&mut self, ty: GenericContext<'db>,
        index: usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    #[operation(source)]
    async fn bound_typevar(&mut self, ty: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
    #[operation(source)]
    async fn eager_typevar_bounds(&mut self, ty: TypeVarInstance<'db>) -> Result<(Option<TypeVarBoundOrConstraints<'db>>, bool), Self::Error>;
    #[operation(source)]
    async fn eager_typevar_default(&mut self, ty: TypeVarInstance<'db>) -> Result<(Option<Type<'db>>, bool), Self::Error>;
    #[operation(source)]
    async fn constraint_elements(&mut self, ty: TypeVarConstraints<'db>) -> Result<&'db [Type<'db>], Self::Error>;
    #[operation(source)]
    async fn nominal_children(&mut self, ty: NominalInstanceType<'db>) -> Result<NominalVisitorChildren<'db>, Self::Error>;
    #[operation(source)]
    async fn protocol_children(&mut self, ty: ProtocolInstanceType<'db>) -> Result<ProtocolVisitorChildren<'db>, Self::Error>;
    #[operation(source)]
    async fn property_class(&mut self, ty: PropertyInstanceType<'db>) -> Result<PropertyInstanceClass<'db>, Self::Error>;
    #[operation(source)]
    async fn property_getter(&mut self, ty: PropertyInstanceType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn property_setter(&mut self, ty: PropertyInstanceType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn property_deleter(&mut self, ty: PropertyInstanceType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn slot_value(&mut self, ty: SlotDescriptorType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn type_is_argument(&mut self, ty: TypeIsType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn type_guard_return(&mut self, ty: TypeGuardType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn type_form_argument(&mut self, ty: TypeFormType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn alias_arguments(&mut self, ty: TypeAliasType<'db>) -> Result<Option<Specialization<'db>>, Self::Error>;
    #[operation(source)]
    async fn recursive_arguments(&mut self, ty: RecursiveType<'db>) -> Result<Option<Specialization<'db>>, Self::Error>;
    #[operation(source)]
    async fn interface_members(&mut self, ty: ProtocolInterfaceView<'db>) -> Result<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>, Self::Error>;
    #[operation(source)]
    async fn synthesized_typed_dict_items(&mut self, ty: SynthesizedTypedDictType<'db>) -> Result<&'db TypedDictSchema<'db>, Self::Error>;
    #[operation(source)]
    async fn synthesized_typed_dict_openness(&mut self, ty: SynthesizedTypedDictType<'db>) -> Result<TypedDictOpenness<'db>, Self::Error>;
    #[operation(source)]
    async fn eager_newtype_base(&mut self, ty: NewType<'db>) -> Result<Option<NewTypeBase<'db>>, Self::Error>;
    #[operation(source)]
    async fn solution_bindings(&mut self, ty: InternedConstraintSetSolution<'db>) -> Result<&'db [TypeVarSolution<'db>], Self::Error>;
    #[operation(source)]
    async fn field_default(&mut self, ty: FieldInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn field_converter(&mut self, ty: FieldInstance<'db>) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
    #[operation(source)]
    async fn union_value(&mut self, ty: UnionTypeInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(source)]
    async fn interned_type(&mut self, ty: InternedType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(source)]
    async fn named_tuple_fields(&mut self, ty: NamedTupleSpec<'db>) -> Result<&'db [NamedTupleField<'db>], Self::Error>;
    #[operation(source)]
    async fn partial_callable(&mut self, ty: FunctoolsPartialInstance<'db>) -> Result<CallableType<'db>, Self::Error>;
    #[operation(source)]
    async fn method_wrapper_type(&mut self, ty: MethodWrapper<'db>) -> Result<Type<'db>, Self::Error>;

    #[operation(local)]
    async fn remember_type(
        &mut self,
        seen: &mut TypeCollector<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;

    /// A successful item pays Advance before removing the pending frame.
    #[operation(local)]
    #[progress]
    async fn take_action(&mut self, cursor: &mut TypeWalkCursor<'db>) -> Result<Option<WalkAction<'db>>, Self::Error>;
    #[operation(local)]
    async fn enqueue(&mut self, cursor: &mut TypeWalkCursor<'db>, action: WalkAction<'db>) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn enqueue_visits<const N: usize>(&mut self, cursor: &mut TypeWalkCursor<'db>, children: [Option<Type<'db>>; N]) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn next_stored(&mut self, types: &mut StoredTypeSequence<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(local)]
    async fn next_member(&mut self, members: &mut btree_map::Iter<'db, Name, ProtocolMemberData<'db>>) -> Result<Option<(&'db Name, &'db ProtocolMemberData<'db>)>, Self::Error>;
    #[operation(child)]
    async fn push_action(&mut self, cursor: &mut TypeWalkCursor<'db>, action: WalkAction<'db>) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn push_visit(&mut self, cursor: &mut TypeWalkCursor<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn push_tuple(&mut self, cursor: &mut TypeWalkCursor<'db>, tuple: &'db TupleSpec<'db>) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn expand_children(&mut self, cursor: &mut TypeWalkCursor<'db>, kind: NonAtomicType<'db>, policy: TypeWalkPolicy) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn expand_wrapper(&mut self, cursor: &mut TypeWalkCursor<'db>, wrapper: KnownBoundMethodType<'db>) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn expand_known(&mut self, cursor: &mut TypeWalkCursor<'db>, known: KnownInstanceType<'db>) -> Result<(), Self::Error>;
    #[operation(child)]
    #[progress]
    async fn next_event(&mut self, cursor: &mut TypeWalkCursor<'db>, policy: TypeWalkPolicy) -> Result<Option<TypeWalkEvent<'db>>, Self::Error>;
    #[operation(local)]
    async fn checkpoint(&mut self, work: TypeWalkWork) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn constraint_type_step(
        &mut self,
        cursor: &mut OwnedConstraintTypeCursor<'db, 'db>,
    ) -> Result<Option<Option<[Type<'db>; 2]>>, Self::Error>;
    #[operation(child)]
    async fn protocol_interface(
        &mut self,
        ty: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;
    #[operation(child)]
    async fn protocol_member_types(
        &mut self,
        member: ProtocolMember<'db, 'db>,
    ) -> Result<[Option<Type<'db>>; 6], Self::Error>;
    #[operation(child)]
    async fn typevar_bounds(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
    #[operation(child)]
    async fn typevar_default(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(child)]
    async fn alias_value(&mut self, ty: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(child)]
    async fn recursive_unfold(&mut self, ty: RecursiveType<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(child)]
    async fn typed_dict_items(
        &mut self,
        ty: TypedDictType<'db>,
    ) -> Result<&'db TypedDictSchema<'db>, Self::Error>;
    #[operation(child)]
    async fn typed_dict_extra(
        &mut self,
        ty: TypedDictType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(child)]
    async fn newtype_base(&mut self, ty: NewType<'db>) -> Result<NewTypeBase<'db>, Self::Error>;
    #[operation(child)]
    async fn newtype_instance(&mut self, base: NewTypeBase<'db>) -> Result<Type<'db>, Self::Error>;
}
#[synchronous(SyncTypeSearchEffects)]
pub(in crate::types) trait TypeSearchEffects<'db, T>:
    TypeWalkEffects<'db>
{
    /// Creates the empty pending cursor and seen set for a search.
    /// Controlled implementations admit construction and header retirement before creating either container.
    #[operation(local)]
    async fn new_state(&mut self) -> Result<(TypeWalkCursor<'db>, TypeCollector<'db>), Self::Error>;

    #[operation(child)]
    async fn predicate(&mut self, ty: Type<'db>) -> Result<T, Self::Error>;

    /// Retains `found` if it differs from the default; otherwise runs the predicate and selects children.
    /// This decision does not construct search state or consult the seen set.
    #[operation(child)]
    async fn decide_visit(
        &mut self,
        ty: Type<'db>,
        policy: TypeWalkPolicy,
        found: T,
    ) -> Result<TypeSearchDecision<'db, T>, Self::Error>
    where
        T: Copy + Default + PartialEq;

    /// Remembers the complete type key and schedules its selected children once.
    #[operation(child)]
    async fn schedule_descent(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        seen: &mut TypeCollector<'db>,
        descent: TypeSearchDescent<'db>,
    ) -> Result<(), Self::Error>;
}
#[synchronous(SyncTypeSupportEffects)]
pub(in crate::types) trait TypeSupportEffects<'db>: TypeWalkEffects<'db> {
    #[operation(local)]
    async fn record_occurrence(&mut self, typevar: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn skipped_lazy(&mut self) -> Result<(), Self::Error>;
}
#[synchronous(SyncTypeDepthEffects)]
pub(in crate::types) trait TypeDepthEffects<'db>: TypeWalkEffects<'db> {
    #[operation(child)]
    async fn nominal_class(
        &mut self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;
    #[operation(local)]
    async fn enter_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn leave_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error>;
}

#[finite_capability]
impl TypeWalkFacts {

    pub(super) fn empty_cursor<'db>(&self) -> TypeWalkCursor<'db> { TypeWalkCursor { pending: SmallVec::new() } }
    pub(super) fn empty_seen<'db>(&self) -> TypeCollector<'db> { TypeCollector::default() }
    fn empty_active<'db>(&self) -> FxHashSet<Type<'db>> { FxHashSet::default() }
    fn empty_result<T: Default>(&self) -> T { T::default() }
    fn has_result<T: Default + PartialEq>(&self, value: T) -> bool { value != T::default() }
    fn kind<'db>(&self, ty: Type<'db>) -> TypeKind<'db> { TypeKind::from(ty) }
    fn is_typevar(&self, ty: Type<'_>) -> bool { ty.is_type_var() }
    fn is_dynamic(&self, ty: Type<'_>) -> bool { ty.is_dynamic() }
    fn is_generic(&self, class: ClassType<'_>) -> bool { class.is_generic() }
    fn increment(&self, index: usize) -> usize { index + 1 }
    fn next_depth(&self, depth: u16) -> u16 { depth.saturating_add(1) }
    fn max_depth(&self, left: u16, right: u16) -> u16 { left.max(right) }
    fn search_policy(&self, mode: TypeSearchMode) -> TypeWalkPolicy { TypeWalkPolicy::search(mode) }
    fn depth_policy(&self) -> TypeWalkPolicy { TypeWalkPolicy::depth() }
    fn support_policy(&self) -> TypeWalkPolicy { TypeWalkPolicy::support() }
    fn eligibility_policy(&self) -> TypeWalkPolicy { TypeWalkPolicy::eligibility() }
    fn as_type<'db, T: Into<Type<'db>>>(&self, value: T) -> Type<'db> { value.into() }
    fn string_literal<'db>(&self, value: crate::types::StringLiteralType<'db>) -> Type<'db> {
        LiteralValueType::promotable(LiteralValueTypeKind::String(value)).into()
    }
    fn fixed_elements<'db>(&self, tuple: &'db TupleSpec<'db>) -> Option<&'db [Type<'db>]> {
        match tuple { Tuple::Fixed(tuple) => Some(tuple.all_elements()), Tuple::Variable(_) => None }
    }
    fn tuple_parts<'db>(&self, tuple: &'db TupleSpec<'db>) -> Option<(&'db [Type<'db>], Type<'db>, &'db [Type<'db>])> {
        match tuple {
            Tuple::Fixed(_) => None,
            Tuple::Variable(tuple) => Some((tuple.prefix_elements(), match tuple.variable() {
                VariableSegment::Homogeneous(element) => element,
                VariableSegment::TypeVarTuple(variable) => Type::TypeVar(variable),
            }, tuple.suffix_elements())),
        }
    }
    fn split_types<'db>(&self, types: &'db [Type<'db>]) -> Option<(Type<'db>, &'db [Type<'db>])> { types.split_first().map(|(head, tail)| (*head, tail)) }
    fn split_signatures<'db>(&self, values: &'db [Signature<'db>]) -> Option<(&'db Signature<'db>, &'db [Signature<'db>])> { values.split_first() }
    fn split_parameters<'db>(&self, values: &'db [Parameter<'db>]) -> Option<(&'db Parameter<'db>, &'db [Parameter<'db>])> { values.split_first() }
    fn split_callables<'db>(&self, values: &'db [CallableType<'db>]) -> Option<(CallableType<'db>, &'db [CallableType<'db>])> { values.split_first().map(|(head, tail)| (*head, tail)) }
    fn return_type<'db>(&self, signature: &Signature<'db>) -> Option<Type<'db>> { signature.return_type_for_visitor() }
    fn parameters<'db>(&self, signature: &'db Signature<'db>) -> &'db [Parameter<'db>] { signature.parameters().as_slice() }
    fn receiver_constraints<'db>(&self, signature: &'db Signature<'db>) -> Option<OwnedConstraintTypeCursor<'db, 'db>> { signature.receiver_constraints().map(OwnedConstraintTypeCursor::new) }
    fn annotated_type<'db>(&self, parameter: &Parameter<'db>) -> Type<'db> { parameter.annotated_type() }
    fn ordered<'db>(&self, types: &'db FxOrderSet<Type<'db>>) -> StoredTypeSequence<'db> { StoredTypeSequence::Ordered(types.iter()) }
    fn negative<'db>(&self, types: &'db NegativeIntersectionElements<'db>) -> StoredTypeSequence<'db> { StoredTypeSequence::Negative(types.iter()) }
    fn bindings<'db>(&self, types: &'db [TypeVarSolution<'db>]) -> StoredTypeSequence<'db> { StoredTypeSequence::SolutionBindings(types.iter()) }
    fn named_fields<'db>(&self, fields: &'db [NamedTupleField<'db>]) -> StoredTypeSequence<'db> { StoredTypeSequence::NamedTupleFields(fields.iter()) }
    fn dictionary_fields<'db>(&self, schema: &'db TypedDictSchema<'db>) -> StoredTypeSequence<'db> { StoredTypeSequence::TypedDictFields(schema.values()) }
    fn stored_member_types<'db>(&self, member: ProtocolMember<'db, 'db>) -> [Option<Type<'db>>; 6] { member.stored_types_for_visitor() }
    fn materialization(&self, interface: ProtocolInterfaceView<'_>) -> Option<MaterializationKind> { interface.materialization_kind() }
    fn member<'db>(&self, name: &'db Name, data: &'db ProtocolMemberData<'db>, materialization: Option<MaterializationKind>) -> ProtocolMember<'db, 'db> { ProtocolMember::from_stored(name, data, materialization) }
    fn extra_type<'db>(&self, openness: TypedDictOpenness<'db>) -> Option<Type<'db>> { openness.explicit_extra_items().map(|extra| extra.declared_ty) }
    fn search_visit(&self) -> TypeWalkVisit {
        #[cfg(test)] { crate::types::constructor::expansion_probe::search_observation::visit() }
        #[cfg(not(test))] { () }
    }
    /// Ends the root's visit observation before processing descendants.
    /// Consuming this value drops the test-only guard; production visits are unit values.
    fn finish_search_visit(&self, _visit: TypeWalkVisit) {}

}
#[synchronous(push_type_walk_action_sync)]
#[capabilities(effects = TypeWalkEffects, _facts = TypeWalkFacts)]
#[passive_values()]
pub(in crate::types) async fn push_type_walk_action_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    action: WalkAction<'db>,
    _facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.enqueue(cursor, action).await
}
#[synchronous(push_type_walk_visit_sync)]
#[capabilities(effects = TypeWalkEffects, _facts = TypeWalkFacts)]
#[passive_values(WalkAction::Visit)]
pub(in crate::types) async fn push_type_walk_visit_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    ty: Type<'db>,
    _facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.push_action(cursor, WalkAction::Visit(ty)).await
}
#[synchronous(push_type_walk_tuple_sync)]
#[capabilities(effects = TypeWalkEffects, facts = TypeWalkFacts)]
#[passive_values(WalkAction::Types)]
pub(in crate::types) async fn push_type_walk_tuple_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    tuple: &'db TupleSpec<'db>,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    if let Some(elements) = facts.fixed_elements(tuple) {
        effects.push_action(cursor, WalkAction::Types(elements)).await?;
    } else if let Some((prefix, variable, suffix)) = facts.tuple_parts(tuple) {
        effects.push_action(cursor, WalkAction::Types(suffix)).await?;
        effects.push_visit(cursor, variable).await?;
        effects.push_action(cursor, WalkAction::Types(prefix)).await?;
    }
    Ok(())
}
#[synchronous(expand_type_children_sync)]
#[capabilities(effects = TypeWalkEffects, facts = TypeWalkFacts)]
#[passive_values(WalkAction::Types, WalkAction::StoredTypes, WalkAction::FunctionImplementations, WalkAction::Signatures, WalkAction::SpecializationTypes, WalkAction::GenericContext, WalkAction::TypeVarBounds, WalkAction::ProtocolInterface, WalkAction::TypedDictFields, WalkAction::TypeAliasValue, WalkAction::SkippedLazy, WalkAction::NewTypeBase, ProtocolVisitorChildren::Interface, TypeWalkWork::Search, SearchWork::Semantic, SearchOperation::ProtocolInterface, SearchOperation::RecursiveUnfold)]
pub(in crate::types) async fn expand_type_children_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    kind: NonAtomicType<'db>,
    policy: TypeWalkPolicy,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    match kind {
        NonAtomicType::Union(union) => {
            let elements = effects.union_elements(union).await?;
            effects.push_action(cursor, WalkAction::Types(elements))
            .await
        }
        NonAtomicType::Intersection(intersection) => {
            let negative = effects.intersection_negative(intersection).await?;
            effects.push_action(cursor, WalkAction::StoredTypes(facts.negative(negative)))
            .await?;
            let positive = effects.intersection_positive(intersection).await?;
            effects.push_action(cursor, WalkAction::StoredTypes(facts.ordered(positive)))
            .await
        }
        NonAtomicType::EnumComplement(complement) => {
            let rest = effects.enum_rest(complement).await?;
            effects.push_action(cursor, WalkAction::StoredTypes(facts.ordered(rest)))
            .await
        }
        NonAtomicType::FunctionLiteral(function) => {
            effects.push_action(cursor, WalkAction::FunctionImplementations(function))
            .await?;
            if let Some(signatures) = effects.function_signature(function).await? {
                effects.push_action(cursor, WalkAction::Signatures(&signatures.overloads))
                .await?;
            }
            Ok(())
        }
        NonAtomicType::BoundMethod(method) => {
            let receiver = effects.method_receiver(method).await?;
            effects.push_visit(cursor, receiver).await?;
            let self_instance = effects.method_self(method).await?;
            effects.push_visit(cursor, self_instance).await?;
            let func = effects.method_func(method).await?;
            effects.push_visit(cursor, func).await
        }
        NonAtomicType::BoundSuper(bound) => {
            let children = effects.bound_super_children(bound).await?;
            effects.enqueue_visits(cursor, children).await
        }
        NonAtomicType::MethodWrapper(wrapper) => {
            effects.expand_wrapper(cursor, wrapper).await
        }
        NonAtomicType::Callable(callable) => {
            let signatures = effects.callable_signatures(callable).await?;
            effects.push_action(cursor, WalkAction::Signatures(&signatures.overloads))
            .await
        }
        NonAtomicType::GenericAlias(alias) => {
            let specialization = effects.alias_specialization(alias).await?;
            if policy.alias_stored_types_only {
                let types = effects.specialization_types(specialization).await?;
                return effects.push_action(cursor, WalkAction::Types(types)).await;
            }
            effects.push_action(cursor, WalkAction::SpecializationTypes(specialization))
            .await?;
            let context = effects.specialization_context(specialization).await?;
            effects.push_action(cursor, WalkAction::GenericContext {
                    context,
                    index: 0,
                })
            .await
        }
        NonAtomicType::KnownInstance(known) => {
            effects.expand_known(cursor, known).await
        }
        NonAtomicType::SubclassOf(subclass) => {
            effects.push_visit(cursor, facts.as_type(subclass)).await
        }
        NonAtomicType::NominalInstance(instance) => match effects.nominal_children(instance).await? {
            NominalVisitorChildren::None => Ok(()),
            NominalVisitorChildren::Class(class) => {
                effects.push_visit(cursor, class).await
            }
            NominalVisitorChildren::Tuple(tuple) => {
                effects.push_tuple(cursor, tuple).await
            }
        },
        NonAtomicType::PropertyInstance(property) => {
            if let Some(deleter) = effects.property_deleter(property).await? {
                effects.push_visit(cursor, deleter).await?;
            }
            if let Some(setter) = effects.property_setter(property).await? {
                effects.push_visit(cursor, setter).await?;
            }
            if let Some(getter) = effects.property_getter(property).await? {
                effects.push_visit(cursor, getter).await?;
            }
            if let PropertyInstanceClass::Subclass(class) = effects.property_class(property).await? {
                effects.push_visit(cursor, facts.as_type(class)).await?;
            }
            Ok(())
        }
        NonAtomicType::SlotDescriptor(descriptor) => {
            let value = effects.slot_value(descriptor).await?;
            effects.push_visit(cursor, value).await
        }
        NonAtomicType::TypeIs(ty) => {
            let argument = effects.type_is_argument(ty).await?;
            effects.push_visit(cursor, argument).await
        }
        NonAtomicType::TypeGuard(ty) => {
            let return_type = effects.type_guard_return(ty).await?;
            effects.push_visit(cursor, return_type).await
        }
        NonAtomicType::TypeForm(ty) => {
            let argument = effects.type_form_argument(ty).await?;
            effects.push_visit(cursor, argument).await
        }
        NonAtomicType::TypeVar(variable) => {
            if !policy.expand_typevars { return Ok(()); }
            let typevar = effects.bound_typevar(variable).await?;
            effects.push_action(cursor, WalkAction::TypeVarBounds(typevar))
            .await
        }
        NonAtomicType::ProtocolInstance(protocol) => {
            #[passive_state]
            let mut children = effects.protocol_children(protocol).await?;
            if policy.lazy
                && matches!(children, ProtocolVisitorChildren::Specialization(_))
            {
                effects
                    .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                        SearchOperation::ProtocolInterface,
                    )))
                    .await?;
                children =
                    ProtocolVisitorChildren::Interface(effects.protocol_interface(protocol).await?);
            }
            match children {
                ProtocolVisitorChildren::Interface(interface) => {
                    effects.push_action(cursor, WalkAction::ProtocolInterface(interface))
                    .await
                }
                ProtocolVisitorChildren::Specialization(specialization) => {
                    if let Some(specialization) = specialization {
                        effects.push_action(cursor, WalkAction::SpecializationTypes(specialization))
                        .await?;
                        let context = effects.specialization_context(specialization).await?;
                        effects.push_action(cursor, WalkAction::GenericContext {
                                context,
                                index: 0,
                            })
                        .await?;
                    }
                    if policy.report_skipped { effects.push_action(cursor, WalkAction::SkippedLazy).await?; }
                    Ok(())
                }
            }
        }
        NonAtomicType::TypedDict(typed_dict) => {
            if let TypedDictType::Class(class) = typed_dict {
                if policy.lazy {
                    effects.push_action(cursor, WalkAction::TypedDictFields(typed_dict))
                    .await?;
                } else if policy.report_skipped {
                    effects.push_action(cursor, WalkAction::SkippedLazy).await?;
                }
                effects.push_visit(cursor, facts.as_type(class)).await
            } else {
                effects.push_action(cursor, WalkAction::TypedDictFields(typed_dict))
                    .await
            }
        }
        NonAtomicType::TypeAlias(alias) => {
            effects.push_action(cursor, WalkAction::TypeAliasValue(alias)).await
        }
        NonAtomicType::Recursive(recursive) => {
            if policy.lazy {
                effects
                    .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                        SearchOperation::RecursiveUnfold,
                    )))
                    .await?;
                let unfolded = effects.recursive_unfold(recursive).await?;
                effects.push_visit(cursor, unfolded)
                .await?;
            } else if policy.report_skipped {
                effects.push_action(cursor, WalkAction::SkippedLazy).await?;
            }
            Ok(())
        }
        NonAtomicType::NewTypeInstance(newtype) => {
            effects.push_action(cursor, WalkAction::NewTypeBase(newtype)).await
        }
    }
}
#[synchronous(expand_method_wrapper_children_sync)]
#[capabilities(effects = TypeWalkEffects, facts = TypeWalkFacts)]
#[passive_values(Type::BoundMethod, WalkAction::Expand, NonAtomicType::PropertyInstance)]
pub(in crate::types) async fn expand_method_wrapper_children_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    wrapper: KnownBoundMethodType<'db>,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    match wrapper {
        KnownBoundMethodType::FunctionTypeDunderGet(function)
        | KnownBoundMethodType::DunderCall(function) => {
            let inner = effects.interned_type(function).await?;
            effects.push_visit(cursor, inner).await
        }
        KnownBoundMethodType::MethodTypeDunderGet(method) => {
            effects.push_visit(cursor, Type::BoundMethod(method)).await
        }
        KnownBoundMethodType::PropertyDunderGet(property)
        | KnownBoundMethodType::PropertyDunderSet(property)
        | KnownBoundMethodType::PropertyDunderDelete(property) => {
            effects.push_action(cursor, WalkAction::Expand(NonAtomicType::PropertyInstance(property)))
            .await
        }
        KnownBoundMethodType::StrStartswith(literal) => {
            effects.push_visit(cursor, facts.string_literal(literal))
            .await
        }
        KnownBoundMethodType::ConstraintSetLowerBound
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
        | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => Ok(()),
    }
}
#[synchronous(expand_known_instance_children_sync)]
#[capabilities(effects = TypeWalkEffects, facts = TypeWalkFacts)]
#[passive_values(WalkAction::GenericContext, WalkAction::TypeVarBounds, WalkAction::TypeAliasValue, WalkAction::StoredTypes, WalkAction::FieldConverter, WalkAction::Expand, NonAtomicType::Callable, WalkAction::NewTypeBase)]
pub(in crate::types) async fn expand_known_instance_children_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    known: KnownInstanceType<'db>,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    match known {
        KnownInstanceType::SubscriptedProtocol(context)
        | KnownInstanceType::SubscriptedGeneric(context) => {
            effects.push_action(cursor, WalkAction::GenericContext { context, index: 0 })
            .await
        }
        KnownInstanceType::TypeVar(variable) => {
            effects.push_action(cursor, WalkAction::TypeVarBounds(variable)).await
        }
        KnownInstanceType::TypeAliasType(alias) => {
            effects.push_action(cursor, WalkAction::TypeAliasValue(alias)).await
        }
        KnownInstanceType::Deprecated(_)
        | KnownInstanceType::Range { .. }
        | KnownInstanceType::ConstraintSet(_)
        | KnownInstanceType::GenericContext(_)
        | KnownInstanceType::Specialization(_)
        | KnownInstanceType::Sentinel(_) => Ok(()),
        KnownInstanceType::ConstraintSetSolution(solution) => {
            let bindings = effects.solution_bindings(solution).await?;
            effects.push_action(cursor, WalkAction::StoredTypes(facts.bindings(bindings)))
            .await
        }
        KnownInstanceType::Field(field) => {
            effects.push_action(cursor, WalkAction::FieldConverter(field)).await?;
            if let Some(default) = effects.field_default(field).await? {
                effects.push_visit(cursor, default).await?;
            }
            Ok(())
        }
        KnownInstanceType::UnionType(instance) => {
            if let Some(union) = effects.union_value(instance).await? {
                effects.push_visit(cursor, union).await?;
            }
            Ok(())
        }
        KnownInstanceType::Literal(ty)
        | KnownInstanceType::Annotated(ty)
        | KnownInstanceType::TypeGenericAlias(ty)
        | KnownInstanceType::LiteralStringAlias(ty) => {
            let inner = effects.interned_type(ty).await?;
            effects.push_visit(cursor, inner).await
        }
        KnownInstanceType::Callable(callable) => {
            effects.push_action(cursor, WalkAction::Expand(NonAtomicType::Callable(callable)))
            .await
        }
        KnownInstanceType::NewType(newtype) => {
            effects.push_action(cursor, WalkAction::NewTypeBase(newtype)).await
        }
        KnownInstanceType::NamedTupleSpec(spec) => {
            let fields = effects.named_tuple_fields(spec).await?;
            effects.push_action(cursor, WalkAction::StoredTypes(facts.named_fields(fields)))
            .await
        }
        KnownInstanceType::FunctoolsPartial(partial)
        | KnownInstanceType::FunctoolsPartialCall(partial) => {
            let callable = effects.partial_callable(partial).await?;
            effects.push_action(cursor, WalkAction::Expand(NonAtomicType::Callable(callable)))
            .await
        }
        KnownInstanceType::MethodWrapper(wrapper) => {
            let wrapped = effects.method_wrapper_type(wrapper).await?;
            effects.push_visit(cursor, wrapped).await
        }
    }
}
#[synchronous(next_type_walk_event_sync)]
#[capabilities(effects = TypeWalkEffects, facts = TypeWalkFacts)]
#[passive_values(NonAtomicType::TypeVar, NonAtomicType::TypeAlias, TypeWalkEvent::Expand, TypeWalkEvent::EndScope, WalkAction::SkippedLazy, TypeWalkEvent::SkippedLazy, WalkAction::Visit, TypeWalkEvent::Visit, WalkAction::ExitDepth, TypeWalkEvent::ExitDepth, WalkAction::Expand, WalkAction::Types, WalkAction::StoredTypes, WalkAction::ConstraintTypes, WalkAction::GenericContext, WalkAction::TypeVarBounds, WalkAction::SpecializationTypes, WalkAction::TypeVarDefault, TypeWalkWork::Search, SearchWork::Semantic, SearchOperation::TypeVarBounds, SearchOperation::TypeVarDefault, WalkAction::Signatures, WalkAction::Parameters, WalkAction::FunctionImplementations, WalkAction::Callables, NonAtomicType::Callable, WalkAction::TypeAliasValue, SearchOperation::AliasValue, WalkAction::ProtocolInterface, WalkAction::ProtocolMembers, WalkAction::ProtocolMember, SearchOperation::ProtocolMember, WalkAction::TypedDictFields, WalkAction::TypedDictExtra, SearchOperation::TypedDictItems, SearchOperation::TypedDictOpenness, WalkAction::NewTypeBase, SearchOperation::NewTypeBase, SearchOperation::NewTypeInstance, Type::NewTypeInstance, WalkAction::FieldConverter)]
pub(in crate::types) async fn next_type_walk_event_with<'db, E: TypeWalkEffects<'db>>(
    cursor: &mut TypeWalkCursor<'db>,
    policy: TypeWalkPolicy,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<Option<TypeWalkEvent<'db>>, E::Error> {
    #[cursor_loop]
    while let Some(action) = effects.take_action(cursor).await? {
        match action {
            WalkAction::SkippedLazy => return Ok(Some(TypeWalkEvent::SkippedLazy)),
            WalkAction::EndScope => return Ok(Some(TypeWalkEvent::EndScope)),
            WalkAction::Visit(ty) => return Ok(Some(TypeWalkEvent::Visit(ty))),
            WalkAction::ExitDepth { ty, previous_depth } => {
                return Ok(Some(TypeWalkEvent::ExitDepth { ty, previous_depth }));
            }
            WalkAction::Expand(ty) => {
                if policy.report_boundaries { return Ok(Some(TypeWalkEvent::Expand(ty))); }
                effects.expand_children(cursor, ty, policy).await?;
            }
            WalkAction::Types(types) => {
                if let Some((head, tail)) = facts.split_types(types) {
                    effects.push_action(cursor, WalkAction::Types(tail)).await?;
                    effects.push_visit(cursor, head).await?;
                }
            }
            WalkAction::StoredTypes(mut types) => {
                if let Some(ty) = effects.next_stored(&mut types).await? {
                    effects.push_action(cursor, WalkAction::StoredTypes(types))
                        .await?;
                    effects.push_visit(cursor, ty).await?;
                }
            }
            WalkAction::ConstraintTypes(mut steps) => {
                if let Some(types) = effects.constraint_type_step(&mut steps).await? {
                    effects.push_action(cursor, WalkAction::ConstraintTypes(steps))
                        .await?;
                    if let Some([first, second]) = types {
                        effects.push_visit(cursor, second).await?;
                        effects.push_visit(cursor, first).await?;
                    }
                }
            }
            WalkAction::GenericContext { context, index } => {
                if !policy.declarations { continue; }
                if let Some(variable) = effects.context_variable(context, index).await? {
                    effects.push_action(cursor, WalkAction::GenericContext {
                            context,
                            index: facts.increment(index),
                        })
                    .await?;
                    if policy.report_boundaries {
                        // The location collector overrides bound-variable visitation. A declaration
                        // is an occurrence there; its bounds and defaults are not traversed.
                        effects.push_action(cursor, WalkAction::Expand(NonAtomicType::TypeVar(variable))).await?;
                    } else {
                        let typevar = effects.bound_typevar(variable).await?;
                        effects.push_action(cursor, WalkAction::TypeVarBounds(typevar)).await?;
                    }
                }
            }
            WalkAction::SpecializationTypes(specialization) => {
                if let Some(tuple) = effects.specialization_tuple(specialization).await? {
                    effects.push_tuple(cursor, tuple).await?;
                }
                let types = effects.specialization_types(specialization).await?;
                effects.push_action(cursor, WalkAction::Types(types))
                .await?;
            }
            WalkAction::TypeVarBounds(variable) => {
                if !policy.declarations { continue; }
                effects.push_action(cursor, WalkAction::TypeVarDefault(variable))
                    .await?;
                let (eager, lazy) = effects.eager_typevar_bounds(variable).await?;
                #[passive_state]
                let mut bounds = eager;
                if lazy && policy.lazy {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::TypeVarBounds,
                        )))
                        .await?;
                    bounds = effects.typevar_bounds(variable).await?;
                } else if lazy && policy.report_skipped { effects.push_action(cursor, WalkAction::SkippedLazy).await?; }
                match bounds {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        effects.push_visit(cursor, bound).await?
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        let elements = effects.constraint_elements(constraints).await?;
                        effects.push_action(cursor, WalkAction::Types(elements))
                        .await?;
                    }
                    None => {}
                }
            }
            WalkAction::TypeVarDefault(variable) => {
                if !policy.declarations { continue; }
                let (eager, lazy) = effects.eager_typevar_default(variable).await?;
                #[passive_state]
                let mut default = eager;
                if lazy && policy.lazy {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::TypeVarDefault,
                        )))
                        .await?;
                    default = effects.typevar_default(variable).await?;
                } else if lazy && policy.report_skipped { return Ok(Some(TypeWalkEvent::SkippedLazy)); }
                if let Some(default) = default {
                    effects.push_visit(cursor, default).await?;
                }
            }
            WalkAction::Signatures(signatures) => {
                if let Some((signature, tail)) = facts.split_signatures(signatures) {
                    effects.push_action(cursor, WalkAction::Signatures(tail))
                        .await?;
                    if let Some(return_type) = facts.return_type(signature) {
                        effects.push_visit(cursor, return_type).await?;
                    }
                    effects.push_action(cursor, WalkAction::Parameters(facts.parameters(signature)))
                    .await?;
                    if let Some(constraints) = facts.receiver_constraints(signature) {
                        effects.push_action(cursor, WalkAction::ConstraintTypes(constraints))
                        .await?;
                    }
                    if let Some(context) = signature.generic_context {
                        effects.push_action(cursor, WalkAction::GenericContext { context, index: 0 })
                        .await?;
                    }
                }
            }
            WalkAction::Parameters(parameters) => {
                if let Some((parameter, tail)) = facts.split_parameters(parameters) {
                    effects.push_action(cursor, WalkAction::Parameters(tail))
                        .await?;
                    effects.push_visit(cursor, facts.annotated_type(parameter)).await?;
                }
            }
            WalkAction::FunctionImplementations(function) => {
                if let Some(callables) = effects.function_implementations(function).await? {
                    effects.push_action(cursor, WalkAction::Callables(callables))
                        .await?;
                }
            }
            WalkAction::Callables(callables) => {
                if let Some((head, tail)) = facts.split_callables(callables) {
                    effects.push_action(cursor, WalkAction::Callables(tail))
                        .await?;
                    effects.push_action(cursor, WalkAction::Expand(NonAtomicType::Callable(head)))
                    .await?;
                }
            }
            WalkAction::TypeAliasValue(alias) => {
                if policy.report_boundaries {
                    return Ok(Some(TypeWalkEvent::Expand(NonAtomicType::TypeAlias(alias))));
                }
                if policy.lazy {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::AliasValue,
                        )))
                        .await?;
                    let value = effects.alias_value(alias).await?;
                    effects.push_visit(cursor, value)
                        .await?;
                } else if policy.report_skipped {
                    return Ok(Some(TypeWalkEvent::SkippedLazy));
                }
            }
            WalkAction::ProtocolInterface(interface) => {
                let members = effects.interface_members(interface).await?;
                effects.push_action(cursor, WalkAction::ProtocolMembers {
                        members,
                        materialization: facts.materialization(interface),
                    })
                .await?;
            }
            WalkAction::ProtocolMembers {
                mut members,
                materialization,
            } => {
                if let Some((name, data)) = effects.next_member(&mut members).await? {
                    effects.push_action(cursor, WalkAction::ProtocolMembers {
                            members,
                            materialization,
                        })
                    .await?;
                    effects.push_action(cursor, WalkAction::ProtocolMember {
                            member: facts.member(name, data, materialization),
                            materialized: matches!(materialization, Some(_)),
                        })
                    .await?;
                }
            }
            WalkAction::ProtocolMember {
                member,
                materialized,
            } => {
                let types = if materialized {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::ProtocolMember,
                        )))
                        .await?;
                    effects.protocol_member_types(member).await?
                } else {
                    facts.stored_member_types(member)
                };
                effects.enqueue_visits(cursor, types).await?;
            }
            WalkAction::TypedDictFields(typed_dict) => {
                effects.push_action(cursor, WalkAction::TypedDictExtra(typed_dict))
                    .await?;
                if matches!(typed_dict, TypedDictType::Class(_)) {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::TypedDictItems,
                        )))
                        .await?;
                }
                let schema = match typed_dict {
                    TypedDictType::Class(_) => effects.typed_dict_items(typed_dict).await?,
                    TypedDictType::Synthesized(synthesized) => {
                        effects.synthesized_typed_dict_items(synthesized).await?
                    }
                };
                effects.push_action(cursor, WalkAction::StoredTypes(facts.dictionary_fields(schema)))
                .await?;
            }
            WalkAction::TypedDictExtra(typed_dict) => {
                if matches!(typed_dict, TypedDictType::Class(_)) {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::TypedDictOpenness,
                        )))
                        .await?;
                }
                let extra = match typed_dict {
                    TypedDictType::Class(_) => effects.typed_dict_extra(typed_dict).await?,
                    TypedDictType::Synthesized(synthesized) => facts.extra_type(effects.synthesized_typed_dict_openness(synthesized).await?),
                };
                if let Some(extra) = extra {
                    effects.push_visit(cursor, extra).await?;
                }
            }
            WalkAction::NewTypeBase(newtype) => {
                #[passive_state]
                let mut base = effects.eager_newtype_base(newtype).await?;
                if matches!(base, None) && policy.lazy {
                    effects
                        .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                            SearchOperation::NewTypeBase,
                        )))
                        .await?;
                    base = Some(effects.newtype_base(newtype).await?);
                } else if matches!(base, None) && policy.report_skipped { return Ok(Some(TypeWalkEvent::SkippedLazy)); }
                if let Some(base) = base {
                    if !matches!(base, NewTypeBase::NewType(_)) {
                        effects
                            .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(
                                SearchOperation::NewTypeInstance,
                            )))
                            .await?;
                    }
                    let ty = match base {
                        NewTypeBase::NewType(newtype) => Type::NewTypeInstance(newtype),
                        _ => effects.newtype_instance(base).await?,
                    };
                    effects.push_visit(cursor, ty).await?;
                }
            }
            WalkAction::FieldConverter(field) => {
                if let Some((input, output)) = effects.field_converter(field).await? {
                    effects.push_visit(cursor, output).await?;
                    effects.push_visit(cursor, input).await?;
                }
            }
        }
    }
    Ok(None)
}
/// Retains a previous match or evaluates the predicate and selects any required seen-set work.
/// A result that differs from the default skips classification and child-field reads.
#[synchronous(decide_type_search_visit_sync)]
#[capabilities(effects = TypeSearchEffects, facts = TypeWalkFacts)]
#[passive_values(TypeSearchDecision::Finished, TypeSearchDecision::Descend, TypeSearchDescent, TypeSearchContinuation::Expand, TypeSearchContinuation::AliasArguments, TypeWalkWork::Search, SearchWork::Predicate, Type::TypeAlias, Type::Recursive)]
pub(in crate::types) async fn decide_type_search_visit_with<'db, T, E: TypeSearchEffects<'db, T>>(
    ty: Type<'db>,
    policy: TypeWalkPolicy,
    found: T,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<TypeSearchDecision<'db, T>, E::Error>
where
    T: Copy + Default + PartialEq,
{
    if facts.has_result(found) {
        return Ok(TypeSearchDecision::Finished(found));
    }
    effects.checkpoint(TypeWalkWork::Search(SearchWork::Predicate)).await?;
    let found = effects.predicate(ty).await?;
    if facts.has_result(found) {
        return Ok(TypeSearchDecision::Finished(found));
    }
    if policy.alias_arguments {
        let arguments = match ty {
            Type::TypeAlias(alias) => Some(effects.alias_arguments(alias).await?),
            Type::Recursive(recursive) => Some(effects.recursive_arguments(recursive).await?),
            _ => None,
        };
        if let Some(arguments) = arguments {
            return Ok(TypeSearchDecision::Descend(found, TypeSearchDescent {
                ty,
                continuation: TypeSearchContinuation::AliasArguments(arguments),
            }));
        }
    }
    match facts.kind(ty) {
        TypeKind::Atomic => Ok(TypeSearchDecision::Finished(found)),
        TypeKind::NonAtomic(kind) => Ok(TypeSearchDecision::Descend(found, TypeSearchDescent {
            ty,
            continuation: TypeSearchContinuation::Expand(kind),
        })),
    }
}

/// Schedules a visit's selected children once per complete type key, remembering the key first.
#[synchronous(schedule_type_search_descent_sync)]
#[capabilities(effects = TypeSearchEffects, _facts = TypeWalkFacts)]
#[passive_values(TypeWalkWork::Search, SearchWork::RememberType, TypeSearchContinuation::Expand, TypeSearchContinuation::AliasArguments, WalkAction::SpecializationTypes, WalkAction::Expand)]
pub(in crate::types) async fn schedule_type_search_descent_with<'db, T, E: TypeSearchEffects<'db, T>>(
    cursor: &mut TypeWalkCursor<'db>,
    seen: &mut TypeCollector<'db>,
    descent: TypeSearchDescent<'db>,
    _facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(TypeWalkWork::Search(SearchWork::RememberType)).await?;
    if effects.remember_type(seen, descent.ty).await? {
        return Ok(());
    }
    match descent.continuation {
        TypeSearchContinuation::Expand(kind) => {
            effects.push_action(cursor, WalkAction::Expand(kind)).await?;
        }
        TypeSearchContinuation::AliasArguments(arguments) => {
            if let Some(arguments) = arguments {
                effects.push_action(cursor, WalkAction::SpecializationTypes(arguments)).await?;
            }
        }
    }
    Ok(())
}

#[synchronous(search_type_sync)]
#[capabilities(effects = TypeSearchEffects, facts = TypeWalkFacts)]
#[passive_values(TypeSearchDecision::Finished, TypeSearchDecision::Descend, TypeWalkEvent::ExitDepth, TypeWalkEvent::SkippedLazy, TypeWalkEvent::Visit)]
pub(in crate::types) async fn search_type_with<'db, T, E: TypeSearchEffects<'db, T>>(
    ty: Type<'db>,
    mode: TypeSearchMode,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<T, E::Error>
where
    T: Copy + Default + PartialEq,
{
    let policy = facts.search_policy(mode);
    let root_visit = facts.search_visit();
    let (found, descent) = match effects.decide_visit(ty, policy, facts.empty_result()).await? {
        TypeSearchDecision::Finished(found) => return Ok(found),
        TypeSearchDecision::Descend(found, descent) => (found, descent),
    };
    let (mut cursor, mut seen) = effects.new_state().await?;
    effects.schedule_descent(&mut cursor, &mut seen, descent).await?;
    facts.finish_search_visit(root_visit);
    #[passive_state]
    let mut found = found;
    #[cursor_loop]
    while let Some(event) = effects.next_event(&mut cursor, policy).await? {
        match event {
            TypeWalkEvent::ExitDepth { .. } | TypeWalkEvent::SkippedLazy | TypeWalkEvent::EndScope => {}
            TypeWalkEvent::Expand(kind) => effects.expand_children(&mut cursor, kind, policy).await?,
            TypeWalkEvent::Visit(ty) => {
                let _visit = facts.search_visit();
                match effects.decide_visit(ty, policy, found).await? {
                    TypeSearchDecision::Finished(result) => {
                        found = result;
                    }
                    TypeSearchDecision::Descend(result, descent) => {
                        found = result;
                        effects.schedule_descent(&mut cursor, &mut seen, descent).await?;
                    }
                }
            }
        }
    }
    Ok(found)
}
#[synchronous(support_type_sync)]
#[capabilities(effects = TypeSupportEffects, facts = TypeWalkFacts)]
#[passive_values(WalkAction::Visit, WalkAction::Expand, TypeWalkEvent::Visit, TypeWalkEvent::SkippedLazy, TypeWalkEvent::ExitDepth, Type::TypeVar)]
pub(in crate::types) async fn support_type_with<'db, E: TypeSupportEffects<'db>>(
    ty: Type<'db>, facts: TypeWalkFacts, effects: &mut E,
) -> Result<(), E::Error> {
    let mut cursor = facts.empty_cursor();
    let mut seen = facts.empty_seen();
    let policy = facts.support_policy();
    effects.push_action(&mut cursor, WalkAction::Visit(ty)).await?;
    #[cursor_loop]
    while let Some(event) = effects.next_event(&mut cursor, policy).await? {
        match event {
            TypeWalkEvent::SkippedLazy => effects.skipped_lazy().await?,
            TypeWalkEvent::ExitDepth { .. } | TypeWalkEvent::EndScope => {},
            TypeWalkEvent::Expand(kind) => effects.expand_children(&mut cursor, kind, policy).await?,
            TypeWalkEvent::Visit(ty) => {
                if let Type::TypeVar(typevar) = ty {
                    effects.record_occurrence(typevar).await?;
                }
                if let TypeKind::NonAtomic(kind) = facts.kind(ty)
                    && !effects.remember_type(&mut seen, ty).await?
                {
                    effects.push_action(&mut cursor, WalkAction::Expand(kind)).await?;
                }
            }
        }
    }
    Ok(())
}

#[synchronous(static_eligible_sync)]
#[capabilities(effects = TypeWalkEffects, facts = TypeWalkFacts)]
#[passive_values(WalkAction::Visit, WalkAction::Expand, TypeWalkEvent::Visit, TypeWalkEvent::SkippedLazy, TypeWalkEvent::ExitDepth)]
pub(in crate::types) async fn static_eligible_with<'db, E: TypeWalkEffects<'db>>(
    ty: Type<'db>, facts: TypeWalkFacts, effects: &mut E,
) -> Result<bool, E::Error> {
    let mut cursor = facts.empty_cursor();
    let mut seen = facts.empty_seen();
    let policy = facts.eligibility_policy();
    effects.push_action(&mut cursor, WalkAction::Visit(ty)).await?;
    #[cursor_loop]
    while let Some(event) = effects.next_event(&mut cursor, policy).await? {
        match event {
            TypeWalkEvent::SkippedLazy | TypeWalkEvent::ExitDepth { .. } | TypeWalkEvent::EndScope => {},
            TypeWalkEvent::Expand(kind) => effects.expand_children(&mut cursor, kind, policy).await?,
            TypeWalkEvent::Visit(ty) => {
                if facts.is_typevar(ty) { continue; }
                if facts.is_dynamic(ty) { return Ok(false); }
                if let TypeKind::NonAtomic(kind) = facts.kind(ty)
                    && !effects.remember_type(&mut seen, ty).await?
                {
                    effects.push_action(&mut cursor, WalkAction::Expand(kind)).await?;
                }
            }
        }
    }
    Ok(true)
}

#[synchronous(type_depth_sync)]
#[capabilities(effects = TypeDepthEffects, facts = TypeWalkFacts)]
#[passive_values(WalkAction::Visit, TypeWalkEvent::SkippedLazy, TypeWalkEvent::ExitDepth, TypeWalkWork::DepthExit, TypeWalkEvent::Visit, TypeWalkWork::DepthVisit, NonAtomicType::NominalInstance, TypeWalkWork::DepthEnter, WalkAction::ExitDepth, WalkAction::Expand)]
pub(in crate::types) async fn type_depth_with<'db, E: TypeDepthEffects<'db>>(
    ty: Type<'db>,
    facts: TypeWalkFacts,
    effects: &mut E,
) -> Result<(u16, u16), E::Error> {
    let mut cursor = facts.empty_cursor();
    let mut active = facts.empty_active();
    #[passive_state]
    let mut current_depth = 0u16;
    #[passive_state]
    let mut max_constructor_depth = 0u16;
    #[passive_state]
    let mut max_typevar_depth = 0u16;
    effects.push_action(&mut cursor, WalkAction::Visit(ty)).await?;
    let policy = facts.depth_policy();
    #[cursor_loop]
    while let Some(event) = effects.next_event(&mut cursor, policy).await? {
        match event {
            TypeWalkEvent::SkippedLazy | TypeWalkEvent::EndScope => {}
            TypeWalkEvent::Expand(kind) => effects.expand_children(&mut cursor, kind, policy).await?,
            TypeWalkEvent::ExitDepth { ty, previous_depth } => {
                current_depth = previous_depth;
                effects.checkpoint(TypeWalkWork::DepthExit).await?;
                effects.leave_active(&mut active, ty).await?;
            }
            TypeWalkEvent::Visit(ty) => {
                effects.checkpoint(TypeWalkWork::DepthVisit).await?;
                if facts.is_typevar(ty) {
                    max_typevar_depth = facts.max_depth(max_typevar_depth, current_depth);
                    continue;
                }
                let kind = match facts.kind(ty) {
                    TypeKind::Atomic => continue,
                    TypeKind::NonAtomic(kind) => kind,
                };
                // A non-generic nominal instance is an opaque leaf. Its class literal
                // identifies the leaf but does not add nested type structure.
                if let NonAtomicType::NominalInstance(instance) = kind
                    && !facts.is_generic(effects.nominal_class(instance).await?)
                {
                    continue;
                }
                effects.checkpoint(TypeWalkWork::DepthEnter).await?;
                if !effects.enter_active(&mut active, ty).await? {
                    continue;
                }
                let previous_depth = current_depth;
                current_depth = facts.next_depth(current_depth);
                max_constructor_depth = facts.max_depth(max_constructor_depth, current_depth);
                effects.push_action(&mut cursor, WalkAction::ExitDepth { ty, previous_depth })
                .await?;
                effects.push_action(&mut cursor, WalkAction::Expand(kind)).await?;
            }
        }
    }
    Ok((max_constructor_depth, max_typevar_depth))
}

}

pub(super) fn search<'db, T, C: SearchControl>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    mode: TypeSearchMode,
    query: impl Fn(Type<'db>) -> T,
    control: &mut C,
) -> Result<T, C::Error>
where
    T: Copy + Default + PartialEq,
{
    search_type_sync(
        ty,
        mode,
        TypeWalkFacts,
        &mut OrdinaryTypeWalk {
            db,
            env,
            control,
            query,
        },
    )
}
pub(in crate::types) fn reserve_walk_pending_with<'db, C: TddControl>(
    cursor: &mut TypeWalkCursor<'db>,
    additional: usize,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    reserve_smallvec(
        &mut cursor.pending,
        additional,
        AllocationKind::TypeWalkPending,
        control,
    )
}
pub(in crate::types) fn enter_depth_active_with<'db, C: TddControl>(
    active: &mut FxHashSet<Type<'db>>,
    ty: Type<'db>,
    control: &mut C,
) -> Result<bool, TddError<C::Error>> {
    admit_type_walk_access_with(ty, control)?;
    if active.contains(&ty) {
        return Ok(false);
    }
    if active.len() == active.capacity() {
        let required = active
            .len()
            .checked_add(1)
            .ok_or(TddError::CapacityExhausted)?;
        for value in active.iter() {
            admit_type_walk_access_with(*value, control)?;
        }
        let mut plan = sequence_growth::<Type<'db>, C::Error>(active.capacity(), required)?;
        plan.relocation_units = active.len();
        control.admit(TddWork::Grow {
            allocation: AllocationKind::TypeWalkActive,
            plan,
        })?;
        active.reserve(plan.requested_capacity - active.len());
    }
    Ok(active.insert(ty))
}
pub(in crate::types) fn leave_depth_active_with<'db, C: TddControl>(
    active: &mut FxHashSet<Type<'db>>,
    ty: Type<'db>,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    admit_type_walk_access_with(ty, control)?;
    active.remove(&ty);
    Ok(())
}
