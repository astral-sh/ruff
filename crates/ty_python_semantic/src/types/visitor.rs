use crate::Db;
use crate::ProgramEnvironment;
use std::cell::{Cell, RefCell};
use std::hash::Hash;

use rustc_hash::{FxBuildHasher, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType, ClassType,
    EnumComplementType, GenericAlias, GenericContext, IntersectionType, KnownBoundMethodType,
    KnownInstanceType, NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, Signature,
    SlotDescriptorType, StaticClassLiteral, SubclassOfType, Type, TypeAliasType, TypeFormType,
    TypeGuardType, TypeIsType, TypedDictType, UnionType,
    bound_super::walk_bound_super_type,
    callable::walk_callable_type,
    class::walk_generic_alias,
    cyclic::{ActiveRecursionDetector, TypeIdentity},
    function::{FunctionType, walk_function_type},
    generics::walk_specialization,
    instance::{walk_nominal_instance_type, walk_protocol_instance_type},
    known_instance::walk_known_instance_type,
    method::{walk_bound_method_type, walk_method_wrapper_type},
    newtype::{NewType, walk_newtype_instance_type},
    protocol_class::walk_protocol_instance_interface,
    set_theoretic::{walk_intersection_type, walk_union},
    signatures::walk_signature,
    subclass_of::walk_subclass_of_type,
    type_alias::{
        walk_type_alias_type, walk_type_alias_value, walk_type_alias_value_with_recursion_guard,
    },
    type_form::walk_typeform_type,
    typed_dict::{walk_typed_dict_fields, walk_typed_dict_type},
    typevar::{TypeVarInstance, walk_bound_type_var_type, walk_type_var_bounds},
    walk_property_instance_type, walk_typeguard_type, walk_typeis_type,
};

/// Visit a type and its nested types.
///
/// Lazy type attributes require a custom visitor with its own recursion guard.
pub(crate) trait TypeVisitor<'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db>;

    /// Notify the visitor that lazily-inferred type attributes were not visited.
    fn notify_skipped_lazy_type_attributes(&self) {}

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>);

    fn visit_union_type(&self, db: &'db dyn Db, union: UnionType<'db>) {
        walk_union(db, union, self);
    }

    fn visit_intersection_type(&self, db: &'db dyn Db, intersection: IntersectionType<'db>) {
        walk_intersection_type(db, intersection, self);
    }

    fn visit_enum_complement_type(&self, db: &'db dyn Db, complement: EnumComplementType<'db>) {
        for rest in complement.rest(db) {
            self.visit_type(db, *rest);
        }
    }

    fn visit_callable_type(&self, db: &'db dyn Db, callable: CallableType<'db>) {
        walk_callable_type(db, callable, self);
    }

    fn visit_signature(&self, db: &'db dyn Db, signature: &Signature<'db>) {
        walk_signature(db, signature, self);
    }

    fn visit_property_instance_type(&self, db: &'db dyn Db, property: PropertyInstanceType<'db>) {
        walk_property_instance_type(db, property, self);
    }

    fn visit_slot_descriptor_type(&self, db: &'db dyn Db, descriptor: SlotDescriptorType<'db>) {
        self.visit_type(db, descriptor.value_type(db));
    }

    fn visit_typeis_type(&self, db: &'db dyn Db, type_is: TypeIsType<'db>) {
        walk_typeis_type(db, type_is, self);
    }

    fn visit_typeguard_type(&self, db: &'db dyn Db, type_is: TypeGuardType<'db>) {
        walk_typeguard_type(db, type_is, self);
    }

    fn visit_typeform_type(&self, db: &'db dyn Db, typeform: TypeFormType<'db>) {
        walk_typeform_type(db, typeform, self);
    }

    fn visit_subclass_of_type(&self, db: &'db dyn Db, subclass_of: SubclassOfType<'db>) {
        walk_subclass_of_type(db, subclass_of, self);
    }

    fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
        walk_generic_alias(db, alias, self);
    }

    fn visit_function_type(&self, db: &'db dyn Db, function: FunctionType<'db>) {
        walk_function_type(db, function, self);
    }

    fn visit_bound_method_type(&self, db: &'db dyn Db, method: BoundMethodType<'db>) {
        walk_bound_method_type(db, method, self);
    }

    fn visit_bound_super_type(&self, db: &'db dyn Db, bound_super: BoundSuperType<'db>) {
        walk_bound_super_type(db, bound_super, self);
    }

    fn visit_nominal_instance_type(&self, db: &'db dyn Db, nominal: NominalInstanceType<'db>) {
        walk_nominal_instance_type(db, nominal, self);
    }

    fn visit_bound_type_var_type(&self, db: &'db dyn Db, bound_typevar: BoundTypeVarInstance<'db>) {
        walk_bound_type_var_type(db, bound_typevar, self);
    }

    fn visit_type_var_type(&self, _db: &'db dyn Db, _typevar: TypeVarInstance<'db>) {
        self.notify_skipped_lazy_type_attributes();
    }

    fn visit_protocol_instance_type(&self, db: &'db dyn Db, protocol: ProtocolInstanceType<'db>) {
        walk_protocol_instance_type(db, protocol, self);
    }

    fn visit_method_wrapper_type(
        &self,
        db: &'db dyn Db,
        method_wrapper: KnownBoundMethodType<'db>,
    ) {
        walk_method_wrapper_type(db, method_wrapper, self);
    }

    fn visit_known_instance_type(&self, db: &'db dyn Db, known_instance: KnownInstanceType<'db>) {
        walk_known_instance_type(db, known_instance, self);
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, type_alias: TypeAliasType<'db>) {
        walk_type_alias_type(db, type_alias, self);
    }

    fn visit_typed_dict_type(&self, db: &'db dyn Db, typed_dict: TypedDictType<'db>) {
        walk_typed_dict_type(db, typed_dict, self);
    }

    fn visit_newtype_instance_type(&self, _db: &'db dyn Db, _newtype: NewType<'db>) {
        self.notify_skipped_lazy_type_attributes();
    }
}

/// Enumeration of types that may contain other types, such as unions, intersections, and generics.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum NonAtomicType<'db> {
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    EnumComplement(EnumComplementType<'db>),
    FunctionLiteral(FunctionType<'db>),
    BoundMethod(BoundMethodType<'db>),
    BoundSuper(BoundSuperType<'db>),
    MethodWrapper(KnownBoundMethodType<'db>),
    Callable(CallableType<'db>),
    GenericAlias(GenericAlias<'db>),
    KnownInstance(KnownInstanceType<'db>),
    SubclassOf(SubclassOfType<'db>),
    NominalInstance(NominalInstanceType<'db>),
    PropertyInstance(PropertyInstanceType<'db>),
    SlotDescriptor(SlotDescriptorType<'db>),
    TypeIs(TypeIsType<'db>),
    TypeGuard(TypeGuardType<'db>),
    TypeForm(TypeFormType<'db>),
    TypeVar(BoundTypeVarInstance<'db>),
    ProtocolInstance(ProtocolInstanceType<'db>),
    TypedDict(TypedDictType<'db>),
    TypeAlias(TypeAliasType<'db>),
    NewTypeInstance(NewType<'db>),
}

pub(super) enum TypeKind<'db> {
    Atomic,
    NonAtomic(NonAtomicType<'db>),
}

impl<'db> From<Type<'db>> for TypeKind<'db> {
    fn from(ty: Type<'db>) -> Self {
        match ty {
            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::Never
            | Type::LiteralValue(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::WrapperDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::SpecialForm(_)
            | Type::Divergent(_)
            | Type::Dynamic(_) => TypeKind::Atomic,

            // Non-atomic types
            Type::FunctionLiteral(function) => {
                TypeKind::NonAtomic(NonAtomicType::FunctionLiteral(function))
            }
            Type::Intersection(intersection) => {
                TypeKind::NonAtomic(NonAtomicType::Intersection(intersection))
            }
            Type::EnumComplement(complement) => {
                TypeKind::NonAtomic(NonAtomicType::EnumComplement(complement))
            }
            Type::Union(union) => TypeKind::NonAtomic(NonAtomicType::Union(union)),
            Type::BoundMethod(method) => TypeKind::NonAtomic(NonAtomicType::BoundMethod(method)),
            Type::BoundSuper(bound_super) => {
                TypeKind::NonAtomic(NonAtomicType::BoundSuper(bound_super))
            }
            Type::KnownBoundMethod(method_wrapper) => {
                TypeKind::NonAtomic(NonAtomicType::MethodWrapper(method_wrapper))
            }
            Type::Callable(callable) => TypeKind::NonAtomic(NonAtomicType::Callable(callable)),
            Type::GenericAlias(alias) => TypeKind::NonAtomic(NonAtomicType::GenericAlias(alias)),
            Type::KnownInstance(known_instance) => {
                TypeKind::NonAtomic(NonAtomicType::KnownInstance(known_instance))
            }
            Type::SubclassOf(subclass_of) => {
                TypeKind::NonAtomic(NonAtomicType::SubclassOf(subclass_of))
            }
            Type::NominalInstance(nominal) => {
                TypeKind::NonAtomic(NonAtomicType::NominalInstance(nominal))
            }
            Type::ProtocolInstance(protocol) => {
                TypeKind::NonAtomic(NonAtomicType::ProtocolInstance(protocol))
            }
            Type::PropertyInstance(property) => {
                TypeKind::NonAtomic(NonAtomicType::PropertyInstance(property))
            }
            Type::SlotDescriptor(descriptor) => {
                TypeKind::NonAtomic(NonAtomicType::SlotDescriptor(descriptor))
            }
            Type::TypeVar(bound_typevar) => {
                TypeKind::NonAtomic(NonAtomicType::TypeVar(bound_typevar))
            }
            Type::TypeIs(type_is) => TypeKind::NonAtomic(NonAtomicType::TypeIs(type_is)),
            Type::TypeGuard(type_guard) => {
                TypeKind::NonAtomic(NonAtomicType::TypeGuard(type_guard))
            }
            Type::TypeForm(typeform) => TypeKind::NonAtomic(NonAtomicType::TypeForm(typeform)),
            Type::TypedDict(typed_dict) => {
                TypeKind::NonAtomic(NonAtomicType::TypedDict(typed_dict))
            }
            Type::TypeAlias(alias) => TypeKind::NonAtomic(NonAtomicType::TypeAlias(alias)),
            Type::NewTypeInstance(newtype) => {
                TypeKind::NonAtomic(NonAtomicType::NewTypeInstance(newtype))
            }
        }
    }
}

pub(super) fn walk_non_atomic_type<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    non_atomic_type: NonAtomicType<'db>,
    visitor: &V,
) {
    match non_atomic_type {
        NonAtomicType::FunctionLiteral(function) => {
            visitor.visit_function_type(db, function);
        }
        NonAtomicType::Intersection(intersection) => {
            visitor.visit_intersection_type(db, intersection);
        }
        NonAtomicType::EnumComplement(complement) => {
            visitor.visit_enum_complement_type(db, complement);
        }
        NonAtomicType::Union(union) => visitor.visit_union_type(db, union),
        NonAtomicType::BoundMethod(method) => {
            visitor.visit_bound_method_type(db, method);
        }
        NonAtomicType::BoundSuper(bound_super) => {
            visitor.visit_bound_super_type(db, bound_super);
        }
        NonAtomicType::MethodWrapper(method_wrapper) => {
            visitor.visit_method_wrapper_type(db, method_wrapper);
        }
        NonAtomicType::Callable(callable) => {
            visitor.visit_callable_type(db, callable);
        }
        NonAtomicType::GenericAlias(alias) => {
            visitor.visit_generic_alias_type(db, alias);
        }
        NonAtomicType::KnownInstance(known_instance) => {
            visitor.visit_known_instance_type(db, known_instance);
        }
        NonAtomicType::SubclassOf(subclass_of) => {
            visitor.visit_subclass_of_type(db, subclass_of);
        }
        NonAtomicType::NominalInstance(nominal) => {
            visitor.visit_nominal_instance_type(db, nominal);
        }
        NonAtomicType::PropertyInstance(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        NonAtomicType::SlotDescriptor(descriptor) => {
            visitor.visit_slot_descriptor_type(db, descriptor);
        }
        NonAtomicType::TypeIs(type_is) => visitor.visit_typeis_type(db, type_is),
        NonAtomicType::TypeGuard(type_guard) => {
            visitor.visit_typeguard_type(db, type_guard);
        }
        NonAtomicType::TypeForm(typeform) => {
            visitor.visit_typeform_type(db, typeform);
        }
        NonAtomicType::TypeVar(bound_typevar) => {
            visitor.visit_bound_type_var_type(db, bound_typevar);
        }
        NonAtomicType::ProtocolInstance(protocol) => {
            visitor.visit_protocol_instance_type(db, protocol);
        }
        NonAtomicType::TypedDict(typed_dict) => {
            visitor.visit_typed_dict_type(db, typed_dict);
        }
        NonAtomicType::TypeAlias(alias) => {
            visitor.visit_type_alias_type(db, alias);
        }
        NonAtomicType::NewTypeInstance(newtype) => {
            visitor.visit_newtype_instance_type(db, newtype);
        }
    }
}

pub(crate) fn walk_type_with_recursion_guard<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    visitor: &impl TypeVisitor<'db>,
    recursion_guard: &TypeCollector<'db>,
) {
    match TypeKind::from(ty) {
        TypeKind::Atomic => {}
        TypeKind::NonAtomic(non_atomic_type) => {
            if recursion_guard.type_was_already_seen(ty) {
                // If we have already seen this type, we can skip it.
                return;
            }
            walk_non_atomic_type(db, non_atomic_type, visitor);
        }
    }
}

#[derive(Default, Debug)]
pub(crate) struct TypeCollector<'db>(RefCell<CollectedTypes<'db>>);

impl<'db> TypeCollector<'db> {
    fn type_was_already_seen(&self, ty: Type<'db>) -> bool {
        !self.0.borrow_mut().insert(ty)
    }
}

// Most guarded walks are shallow; avoid allocating a hash table until linear search is costly.
type CollectedTypes<'db> = SmallSet<Type<'db>, 8>;

/// A set optimized for values that usually contain only a few distinct elements.
#[derive(Debug)]
enum SmallSet<T, const INLINE_CAPACITY: usize> {
    Inline(SmallVec<[T; INLINE_CAPACITY]>),
    Spilled(FxHashSet<T>),
}

impl<T, const INLINE_CAPACITY: usize> Default for SmallSet<T, INLINE_CAPACITY> {
    fn default() -> Self {
        Self::Inline(SmallVec::new())
    }
}

impl<T, const INLINE_CAPACITY: usize> SmallSet<T, INLINE_CAPACITY> {
    #[inline]
    fn insert(&mut self, value: T) -> bool
    where
        T: Hash + Eq,
    {
        match self {
            Self::Inline(inline) => {
                if inline.contains(&value) {
                    return false;
                }

                if inline.len() < INLINE_CAPACITY {
                    inline.push(value);
                    return true;
                }

                *self = Self::Spilled(Self::spill(inline, value));
                true
            }
            Self::Spilled(set) => set.insert(value),
        }
    }

    #[cold]
    fn spill(inline: &mut SmallVec<[T; INLINE_CAPACITY]>, value: T) -> FxHashSet<T>
    where
        T: Hash + Eq,
    {
        let mut set = FxHashSet::with_capacity_and_hasher(inline.len() + 1, FxBuildHasher);
        set.extend(inline.drain(..));
        let inserted = set.insert(value);
        debug_assert!(inserted);
        set
    }

    #[cfg(test)]
    const fn is_spilled(&self) -> bool {
        matches!(self, Self::Spilled(_))
    }
}

/// Whether a type contains a dynamic type matching the requested filter.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum DynamicContent {
    /// The type was fully inspected and contains no matching dynamic type.
    Absent,
    /// The type contains a matching dynamic type.
    Present,
    /// Lazy type information or recursive specialization prevented a complete inspection.
    Indeterminate,
}

impl DynamicContent {
    pub(super) const fn is_absent(self) -> bool {
        matches!(self, Self::Absent)
    }
}

#[derive(Clone, Copy)]
enum DynamicContentMode {
    All,
    NonAny,
    /// Require enough information to prove that materialization preserves type requirements.
    Materialization,
}

/// Determine whether `ty` contains any dynamic type.
pub(super) fn dynamic_content<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> DynamicContent {
    dynamic_content_impl(db, env, ty, DynamicContentMode::All)
}

/// Whether both materializations preserve the requirements described by `ty`.
///
/// Unlike ordinary static-content checks, this proof includes type-variable bounds and cannot
/// ignore lazy function signatures or the wrapped callable of a partial. It does not compare
/// metadata such as parameter-default types, which do not affect whether one callable satisfies
/// another's requirements.
pub(super) fn materialization_is_noop<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> bool {
    dynamic_content_impl(db, env, ty, DynamicContentMode::Materialization).is_absent()
}

/// Determine whether `ty` contains a dynamic type other than `Any`.
///
/// Bounds and constraints are included: a cast involving a type variable bounded by `Unknown`
/// is not reported as redundant, even when the source and target are considered equivalent.
///
/// Class-based protocol interfaces can be recursively specialized. An exact recursive cycle adds
/// no new information, but revisiting the same protocol definition under a different
/// specialization may expose different members and is therefore indeterminate.
///
/// ```python
/// class Exact[T](Protocol):
///     next: Exact[T]
///
/// class Growing[T](Protocol):
///     next: Growing[list[T]]
/// ```
///
/// Walking `Exact[int]` can skip its exact back-edge. Walking `Growing[int]` is indeterminate
/// because each recursive edge creates a new specialization.
pub(super) fn non_any_dynamic_content<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> DynamicContent {
    dynamic_content_impl(db, env, ty, DynamicContentMode::NonAny)
}

fn dynamic_content_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    mode: DynamicContentMode,
) -> DynamicContent {
    struct DynamicContentVisitor<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        recursion_guard: TypeCollector<'db>,
        active_class_protocols: ActiveRecursionDetector<StaticClassLiteral<'db>>,
        active_class_typed_dicts: ActiveRecursionDetector<StaticClassLiteral<'db>>,
        active_types: ActiveRecursionDetector<TypeIdentity<'db>>,
        content: Cell<DynamicContent>,
        mode: DynamicContentMode,
    }

    impl DynamicContentVisitor<'_, '_> {
        fn record(&self, content: DynamicContent) {
            debug_assert!(self.content.get().is_absent());
            debug_assert!(!content.is_absent());
            self.content.set(content);
        }
    }

    impl<'db> TypeVisitor<'db> for DynamicContentVisitor<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if !self.content.get().is_absent() {
                return;
            }

            if matches!(self.mode, DynamicContentMode::Materialization) && ty.is_divergent() {
                self.record(DynamicContent::Indeterminate);
                return;
            }

            if ty.is_dynamic()
                && (!matches!(self.mode, DynamicContentMode::NonAny)
                    || !matches!(ty, Type::Dynamic(crate::types::DynamicType::Any)))
            {
                self.record(DynamicContent::Present);
                return;
            }

            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }

        fn visit_function_type(&self, db: &'db dyn Db, function: FunctionType<'db>) {
            if !self.content.get().is_absent() {
                return;
            }

            if matches!(self.mode, DynamicContentMode::Materialization) {
                // The ordinary walker only visits updated signatures. Inferring an original
                // signature here could re-enter recursive `TypeOf` evaluation, so do not claim
                // that materialization leaves this function's requirements unchanged.
                self.record(DynamicContent::Indeterminate);
            } else {
                walk_function_type(db, function, self);
            }
        }

        fn visit_known_instance_type(&self, db: &'db dyn Db, known: KnownInstanceType<'db>) {
            if matches!(self.mode, DynamicContentMode::Materialization)
                && let KnownInstanceType::FunctoolsPartial(partial)
                | KnownInstanceType::FunctoolsPartialCall(partial) = known
            {
                // Materialization maps both the reduced signature and the wrapped callable.
                self.visit_type(db, partial.wrapped(db).inner(db));
            }
            if self.content.get().is_absent() {
                walk_known_instance_type(db, known, self);
            }
        }

        fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) {
            if !self.content.get().is_absent() || matches!(self.mode, DynamicContentMode::All) {
                return;
            }

            // Bounds affect materialization and redundant-cast checks, but do not make the type
            // variable itself gradual for inference.
            self.active_types.visit(
                &Type::KnownInstance(KnownInstanceType::TypeVar(typevar)).to_type_identity(db),
                || self.record(DynamicContent::Indeterminate),
                || {
                    if let Some(bounds) = typevar.bound_or_constraints(db, self.env) {
                        walk_type_var_bounds(db, bounds, self);
                    }
                },
            );
        }

        fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
            self.active_types.visit(
                &Type::TypeAlias(alias).to_type_identity(db),
                || self.record(DynamicContent::Indeterminate),
                || walk_type_alias_value(db, alias, self),
            );
        }

        fn visit_newtype_instance_type(&self, db: &'db dyn Db, newtype: NewType<'db>) {
            self.active_types.visit(
                &Type::NewTypeInstance(newtype).to_type_identity(db),
                || self.record(DynamicContent::Indeterminate),
                || walk_newtype_instance_type(db, newtype, self),
            );
        }

        fn visit_protocol_instance_type(
            &self,
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
        ) {
            let protocol_ty = Type::ProtocolInstance(protocol);
            let Some((origin, specialization)) = protocol
                .class_origin(db)
                .and_then(|class| class.static_class_literal(db))
            else {
                walk_protocol_instance_interface(db, protocol.interface(db), protocol_ty, self);
                return;
            };

            if let Some(specialization) = specialization {
                walk_specialization(db, specialization, self);
                if !self.content.get().is_absent() {
                    return;
                }
            }

            self.active_class_protocols.visit(
                &origin,
                || self.record(DynamicContent::Indeterminate),
                || {
                    walk_protocol_instance_interface(db, protocol.interface(db), protocol_ty, self);
                },
            );
        }

        fn visit_typed_dict_type(&self, db: &'db dyn Db, typed_dict: TypedDictType<'db>) {
            let Some((origin, specialization)) = typed_dict
                .defining_class()
                .and_then(|class| class.static_class_literal(db))
            else {
                walk_typed_dict_fields(db, typed_dict, self);
                return;
            };

            if let Some(specialization) = specialization {
                walk_specialization(db, specialization, self);
                if !self.content.get().is_absent() {
                    return;
                }
            }

            self.active_class_typed_dicts.visit(
                &origin,
                || self.record(DynamicContent::Indeterminate),
                || walk_typed_dict_fields(db, typed_dict, self),
            );
        }
    }

    let visitor = DynamicContentVisitor {
        env,
        recursion_guard: TypeCollector::default(),
        active_class_protocols: ActiveRecursionDetector::default(),
        active_class_typed_dicts: ActiveRecursionDetector::default(),
        active_types: ActiveRecursionDetector::default(),
        content: Cell::new(DynamicContent::Absent),
        mode,
    };
    visitor.visit_type(db, ty);
    visitor.content.get()
}

/// Return whether `ty` depends on a type variable matched by `query`.
///
/// Local protocols and `TypedDict`s can capture outer type variables in their members.
/// For global classes, only their type arguments can introduce a dependency.
/// A callable's own type parameters do not count as dependencies.
pub(super) fn contains_typevar_dependency<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    query: impl Fn(BoundTypeVarInstance<'db>) -> bool,
) -> bool {
    struct TypeVarDependencyVisitor<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        query: &'a dyn Fn(BoundTypeVarInstance<'db>) -> bool,
        bound_contexts: RefCell<SmallVec<[GenericContext<'db>; 2]>>,
        visited_types: RefCell<CollectedTypes<'db>>,
        active_types: ActiveRecursionDetector<TypeIdentity<'db>>,
        active_structural_definitions: ActiveRecursionDetector<Definition<'db>>,
        found: Cell<bool>,
    }

    impl<'db> TypeVarDependencyVisitor<'_, 'db> {
        fn visit_guarded(&self, db: &'db dyn Db, ty: Type<'db>, visit: impl FnOnce()) {
            self.active_types
                .visit(&ty.to_type_identity(db), || {}, visit);
        }

        fn visit_structural_class(
            &self,
            db: &'db dyn Db,
            class: ClassType<'db>,
            ty: Type<'db>,
            visit_members: impl FnOnce(),
        ) {
            self.visit_type(db, class.into());

            if self.found.get() {
                return;
            }

            match class.definition(db) {
                Some(definition) if definition.file_scope(db).is_global() => {}
                Some(definition) => {
                    self.active_structural_definitions
                        .visit(&definition, || {}, visit_members);
                }
                None => self.visit_guarded(db, ty, visit_members),
            }
        }
    }

    impl<'db> TypeVisitor<'db> for TypeVarDependencyVisitor<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn visit_signature(&self, db: &'db dyn Db, signature: &Signature<'db>) {
            let Some(context) = signature.generic_context else {
                walk_signature(db, signature, self);
                return;
            };

            // A type variable bound by this signature can be free outside it, so visited types
            // cannot be shared across callable scopes. Active recursion guards remain shared.
            self.bound_contexts.borrow_mut().push(context);
            let visited_types = self.visited_types.take();

            walk_signature(db, signature, self);

            self.visited_types.replace(visited_types);
            self.bound_contexts.borrow_mut().pop();
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if self.found.get() {
                return;
            }

            if let Type::TypeVar(typevar) = ty {
                let bound = self
                    .bound_contexts
                    .borrow()
                    .iter()
                    .any(|context| context.contains(db, typevar.identity(db)));
                self.found.set(!bound && (self.query)(typevar));
                return;
            }

            let TypeKind::NonAtomic(non_atomic) = TypeKind::from(ty) else {
                return;
            };
            if !self.visited_types.borrow_mut().insert(ty) {
                return;
            }

            walk_non_atomic_type(db, non_atomic, self);
        }

        fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
            walk_type_alias_value_with_recursion_guard(db, alias, self, &self.active_types);
        }

        fn visit_protocol_instance_type(
            &self,
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
        ) {
            if let Some(class) = protocol.class_origin(db) {
                let ty = Type::ProtocolInstance(protocol);
                self.visit_structural_class(db, *class, ty, || {
                    walk_protocol_instance_interface(db, protocol.interface(db), ty, self);
                });
            } else {
                walk_protocol_instance_type(db, protocol, self);
            }
        }

        fn visit_typed_dict_type(&self, db: &'db dyn Db, typed_dict: TypedDictType<'db>) {
            if let Some(class) = typed_dict.defining_class() {
                self.visit_structural_class(db, class, Type::TypedDict(typed_dict), || {
                    walk_typed_dict_fields(db, typed_dict, self);
                });
            } else {
                walk_typed_dict_type(db, typed_dict, self);
            }
        }

        fn visit_newtype_instance_type(&self, db: &'db dyn Db, newtype: NewType<'db>) {
            self.visit_guarded(db, Type::NewTypeInstance(newtype), || {
                walk_newtype_instance_type(db, newtype, self);
            });
        }

        fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) {
            self.visit_guarded(
                db,
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar)),
                || {
                    if let Some(bounds) = typevar.bound_or_constraints(db, self.env) {
                        walk_type_var_bounds(db, bounds, self);
                    }
                },
            );
        }
    }

    let visitor = TypeVarDependencyVisitor {
        env,
        query: &query,
        bound_contexts: RefCell::default(),
        visited_types: RefCell::default(),
        active_types: ActiveRecursionDetector::default(),
        active_structural_definitions: ActiveRecursionDetector::default(),
        found: Cell::new(false),
    };
    visitor.visit_type(db, ty);
    visitor.found.get()
}

/// Search `ty` and its nested types, expanding aliases, type-variable bounds, and `NewType` bases.
///
/// Structural members and type-variable defaults are not visited.
pub(super) fn any_over_expanded_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    find_over_expanded_type(db, env, ty, |ty| query(ty).then_some(())).is_some()
}

/// Return the first match using the same traversal as [`any_over_expanded_type`].
pub(super) fn find_over_expanded_type<'db, T: Copy>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> Option<T>,
) -> Option<T> {
    struct ExpandedTypeVisitor<'a, 'db, T> {
        env: &'a ProgramEnvironment<'db>,
        query: &'a dyn Fn(Type<'db>) -> Option<T>,
        recursion_guard: TypeCollector<'db>,
        active_types: ActiveRecursionDetector<TypeIdentity<'db>>,
        found: Cell<Option<T>>,
    }

    impl<'db, T> ExpandedTypeVisitor<'_, 'db, T> {
        fn visit_guarded(&self, db: &'db dyn Db, ty: Type<'db>, visit: impl FnOnce()) {
            self.active_types
                .visit(&ty.to_type_identity(db), || {}, visit);
        }
    }

    impl<'db, T: Copy> TypeVisitor<'db> for ExpandedTypeVisitor<'_, 'db, T> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if self.found.get().is_some() {
                return;
            }

            if let Some(found) = (self.query)(ty) {
                self.found.set(Some(found));
                return;
            }

            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }

        fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
            walk_type_alias_value_with_recursion_guard(db, alias, self, &self.active_types);
        }

        fn visit_protocol_instance_type(
            &self,
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
        ) {
            if let Some(class) = protocol.class_origin(db) {
                self.visit_type(db, (*class).into());
            }
        }

        fn visit_typed_dict_type(&self, db: &'db dyn Db, typed_dict: TypedDictType<'db>) {
            if let Some(class) = typed_dict.defining_class() {
                self.visit_type(db, class.into());
            }
        }

        fn visit_newtype_instance_type(&self, db: &'db dyn Db, newtype: NewType<'db>) {
            self.visit_guarded(db, Type::NewTypeInstance(newtype), || {
                walk_newtype_instance_type(db, newtype, self);
            });
        }

        fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) {
            self.visit_guarded(
                db,
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar)),
                || {
                    if let Some(bounds) = typevar.bound_or_constraints(db, self.env) {
                        walk_type_var_bounds(db, bounds, self);
                    }
                },
            );
        }
    }

    let visitor = ExpandedTypeVisitor {
        env,
        query: &query,
        recursion_guard: TypeCollector::default(),
        active_types: ActiveRecursionDetector::default(),
        found: Cell::new(None),
    };
    visitor.visit_type(db, ty);
    visitor.found.get()
}

/// Search alias values without expanding other lazy attributes.
///
/// An alias cycle counts as a match. This lets callers reject a fast path when the search cannot
/// establish that the type is finite and contains no matching type.
pub(super) fn any_over_type_expanding_aliases<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    struct AliasSearchVisitor<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        query: &'a dyn Fn(Type<'db>) -> bool,
        recursion_guard: TypeCollector<'db>,
        active_aliases: ActiveRecursionDetector<TypeIdentity<'db>>,
        found: Cell<bool>,
    }

    impl<'db> TypeVisitor<'db> for AliasSearchVisitor<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if self.found.get() {
                return;
            }

            if (self.query)(ty) {
                self.found.set(true);
                return;
            }

            if let Type::TypeAlias(alias) = ty {
                // Check active aliases before the exact-type guard: even an exact cycle means
                // that this search cannot establish finiteness.
                self.active_aliases.visit(
                    &Type::TypeAlias(alias).to_type_identity(db),
                    || self.found.set(true),
                    || walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard),
                );
            } else {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
            walk_type_alias_value(db, alias, self);
        }
    }

    let visitor = AliasSearchVisitor {
        env,
        query: &query,
        recursion_guard: TypeCollector::default(),
        active_aliases: ActiveRecursionDetector::default(),
        found: Cell::new(false),
    };
    visitor.visit_type(db, ty);
    visitor.found.get()
}

/// Return whether `query` matches `ty` or any of its nested types.
///
/// Note that lazy type attributes are not visited by this method, and require a custom type visitor.
pub(super) fn any_over_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    find_over_type(db, env, ty, |ty| query(ty).then_some(())).is_some()
}

/// Return the first non-`None` result using the same traversal as [`any_over_type`].
pub(super) fn find_over_type<'db, T: Copy>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> Option<T>,
) -> Option<T> {
    struct FindTypeVisitor<'a, 'db, T> {
        env: &'a ProgramEnvironment<'db>,
        query: &'a dyn Fn(Type<'db>) -> Option<T>,
        recursion_guard: TypeCollector<'db>,
        found: Cell<Option<T>>,
    }

    impl<'db, T: Copy> TypeVisitor<'db> for FindTypeVisitor<'_, 'db, T> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if self.found.get().is_some() {
                return;
            }

            if let Some(found) = (self.query)(ty) {
                self.found.set(Some(found));
                return;
            }

            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }
    }

    let visitor = FindTypeVisitor {
        env,
        query: &query,
        recursion_guard: TypeCollector::default(),
        found: Cell::new(None),
    };
    visitor.visit_type(db, ty);
    visitor.found.get()
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ty_python_core::ProgramFile;

    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::{DynamicType, SpecialFormType, Type};

    use super::{CollectedTypes, materialization_is_noop};

    #[test]
    fn materialization_noop_checks_hidden_function_types() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            from __future__ import annotations
            from functools import partial
            from typing import Any, Protocol
            from ty_extensions._internal import TypeOf

            def gradual_callback(value: Any) -> None: ...

            class Callbacks(Protocol):
                @property
                def callback(self) -> TypeOf[gradual_callback]: ...

            class Recursive[T](Protocol):
                @property
                def value(self) -> T: ...
                @property
                def child(self) -> Recursive[T]: ...

            plain: Recursive[int]
            callbacks: Recursive[Callbacks]
            partial_callback = partial(gradual_callback, 0)
            partial_call = partial_callback.__call__
            "#,
        )?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py")?;
        let module = ProgramFile::new(&db, file, env.program(&db));
        for (name, expected) in [
            ("plain", true),
            ("callbacks", false),
            ("partial_callback", false),
            ("partial_call", false),
        ] {
            let ty = global_symbol(&db, module, name).place.expect_type();
            assert_eq!(materialization_is_noop(&db, &env, ty), expected, "{name}");
        }
        Ok(())
    }

    #[test]
    fn materialization_noop_rejects_divergent_markers() {
        let db = setup_db();
        let env = db.program_environment();
        let divergent = Type::divergent(salsa::plumbing::Id::from_bits(1));

        for ty in [
            divergent,
            divergent.top_materialization(&db, &env),
            divergent.bottom_materialization(&db, &env),
        ] {
            assert!(!materialization_is_noop(&db, &env, ty));
        }
    }

    #[test]
    fn collected_types_spills_without_losing_deduplication() {
        let mut collected = CollectedTypes::default();
        let types = [
            Type::Never,
            Type::AlwaysTruthy,
            Type::AlwaysFalsy,
            Type::Dynamic(DynamicType::Any),
            Type::Dynamic(DynamicType::Unknown),
            Type::Dynamic(DynamicType::UnspecializedTypeVar),
            Type::Dynamic(DynamicType::InvalidConcatenateUnknown),
            Type::Dynamic(DynamicType::AmbiguousOverload),
            Type::SpecialForm(SpecialFormType::Any),
        ];

        for ty in types {
            assert!(collected.insert(ty));
        }

        assert!(collected.is_spilled());
        assert!(!collected.insert(Type::Never));
        assert!(!collected.insert(Type::SpecialForm(SpecialFormType::Any)));
        assert!(collected.insert(Type::SpecialForm(SpecialFormType::Unknown)));
    }
}
