use crate::Db;
use crate::types::TypeOperations;
use crate::types::recursive::StructuralTypeVisit;
use crate::types::recursive::TypeWalkKey;
use crate::types::recursive::{StructuralTraversal, StructuralTypeTraversal};
use std::cell::{Cell, RefCell};
use std::hash::Hash;

use rustc_hash::{FxBuildHasher, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::types::{
    ApplyTypeMappingVisitor, BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType,
    CycleKind, EnumComplementType, GenericAlias, IntersectionType, KnownBoundMethodType,
    KnownInstanceType, NominalInstanceType, PropertyInstanceType, ProtocolInstanceType,
    RecursiveType, SlotDescriptorType, StaticClassLiteral, SubclassOfType, Type, TypeAliasType,
    TypeFormType, TypeGuardType, TypeIsType, TypeWalk, TypedDictType, UnionType,
    bound_super::walk_bound_super_type,
    callable::{SignatureSource, walk_callable_type},
    class::walk_generic_alias,
    cyclic::{ActiveRecursionDetector, TypeIdentity},
    function::{FunctionType, walk_function_type},
    generics::walk_specialization_types,
    instance::{walk_nominal_instance_type, walk_protocol_instance_type},
    known_instance::walk_known_instance_type,
    method::{walk_bound_method_type, walk_method_wrapper_type},
    newtype::{NewType, walk_newtype_instance_type},
    protocol_class::walk_protocol_instance_interface,
    set_theoretic::{walk_intersection_type, walk_union},
    subclass_of::walk_subclass_of_type,
    type_alias::walk_type_alias_type,
    type_form::walk_typeform_type,
    typed_dict::walk_typed_dict_type,
    typevar::{TypeVarInstance, walk_bound_type_var_type, walk_type_var_type},
    walk_property_instance_type, walk_typeguard_type, walk_typeis_type,
};

/// A guarded walk through nested types.
///
/// Only the traversal wrapper and its active visits implement this trait. Walkers can
/// visit children, but cannot bypass the guard by dispatching a variant's hook directly.
pub(crate) trait TypeVisitor<'db>: sealed::Sealed {
    fn program_environment(&self) -> &TypeOperations<'db>;
    fn should_visit_lazy_type_attributes(&self) -> bool;
    fn notify_skipped_lazy_type_attributes(&self);
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>);
    fn visit_type_contents(&self, db: &'db dyn Db, ty: Type<'db>);
    fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>);
}

mod sealed {
    pub(crate) trait Sealed {}
}

/// Owns the active stack for one complete visitor operation.
pub(crate) struct TypeVisitorContext<'a, 'db, V> {
    visitor: &'a V,
    active: ActiveRecursionDetector<Type<'db>>,
}

impl<'a, 'db, V: TypeVisitorHooks<'db>> TypeVisitorContext<'a, 'db, V> {
    pub(crate) fn new(visitor: &'a V) -> Self {
        Self {
            visitor,
            active: ActiveRecursionDetector::default(),
        }
    }
}

impl<V> sealed::Sealed for TypeVisitorContext<'_, '_, V> {}

impl<'db, V: TypeVisitorHooks<'db>> TypeVisitor<'db> for TypeVisitorContext<'_, 'db, V> {
    fn program_environment(&self) -> &TypeOperations<'db> {
        self.visitor.program_environment()
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        self.visitor.should_visit_lazy_type_attributes()
    }
    fn notify_skipped_lazy_type_attributes(&self) {
        self.visitor.notify_skipped_lazy_type_attributes();
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        guarded_visit(db, ty, self.visitor, &self.active);
    }
    fn visit_type_contents(&self, db: &'db dyn Db, ty: Type<'db>) {
        guarded_visit_contents(db, ty, self.visitor, &self.active);
    }
    fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) {
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(typevar));
        self.active.visit(
            self.visitor.program_environment(),
            &ty,
            || (),
            || {
                self.visitor.visit_type_var_type(
                    db,
                    &TypeVisit {
                        visitor: self.visitor,
                        active: &self.active,
                        ty,
                    },
                    typevar,
                );
            },
        );
    }
}

/// Access to children of the type whose recursion guard is currently active.
pub(crate) struct TypeVisit<'a, 'db, V> {
    visitor: &'a V,
    active: &'a ActiveRecursionDetector<Type<'db>>,
    ty: Type<'db>,
}

impl<V> sealed::Sealed for TypeVisit<'_, '_, V> {}

impl<'db, V: TypeVisitorHooks<'db>> TypeVisitor<'db> for TypeVisit<'_, 'db, V> {
    fn program_environment(&self) -> &TypeOperations<'db> {
        self.visitor.program_environment()
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        self.visitor.should_visit_lazy_type_attributes()
    }
    fn notify_skipped_lazy_type_attributes(&self) {
        self.visitor.notify_skipped_lazy_type_attributes();
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        guarded_visit(db, ty, self.visitor, self.active);
    }
    fn visit_type_contents(&self, db: &'db dyn Db, ty: Type<'db>) {
        guarded_visit_contents(db, ty, self.visitor, self.active);
    }
    fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) {
        self.visitor.visit_type_var_type(db, self, typevar);
    }
}

fn guarded_visit<'db, V: TypeVisitorHooks<'db>>(
    db: &'db dyn Db,
    ty: Type<'db>,
    visitor: &V,
    active: &ActiveRecursionDetector<Type<'db>>,
) {
    let visit = || {
        visitor.visit_type(
            db,
            &TypeVisit {
                visitor,
                active,
                ty,
            },
            ty,
        );
    };
    match TypeKind::from(ty) {
        TypeKind::Atomic => visit(),
        TypeKind::NonAtomic(_) => active.visit(visitor.program_environment(), &ty, || (), visit),
    }
}

/// Inspect a declaration or wrapper's contents without reporting it as a type occurrence.
/// Generic parameters declared on a callable, for example, are distinct from references to
/// those parameters in its annotations.
fn guarded_visit_contents<'db, V: TypeVisitorHooks<'db>>(
    db: &'db dyn Db,
    ty: Type<'db>,
    visitor: &V,
    active: &ActiveRecursionDetector<Type<'db>>,
) {
    active.visit(
        visitor.program_environment(),
        &ty,
        || (),
        || {
            TypeVisit {
                visitor,
                active,
                ty,
            }
            .walk_children(db);
        },
    );
}

/// Defines what a visitor observes while the traversal owns its recursion guards.
pub(crate) trait TypeVisitorHooks<'db>: Sized {
    fn program_environment(&self) -> &TypeOperations<'db>;

    /// Whether this walk inspects deferred attributes. Their expansion always uses the
    /// operation returned by `program_environment`.
    fn should_visit_lazy_type_attributes(&self) -> bool;

    /// Notify the visitor that lazily-inferred type attributes were not visited.
    fn notify_skipped_lazy_type_attributes(&self) {}

    fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, ty: Type<'db>);

    fn visit_union_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        union: UnionType<'db>,
    ) {
        walk_union(db, union, visit);
    }

    fn visit_intersection_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        intersection: IntersectionType<'db>,
    ) {
        walk_intersection_type(db, intersection, visit);
    }

    fn visit_enum_complement_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        complement: EnumComplementType<'db>,
    ) {
        for rest in complement.rest(db) {
            visit.visit_type(db, *rest);
        }
    }

    fn visit_callable_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        callable: CallableType<'db>,
    ) {
        walk_callable_type(db, callable, visit);
    }

    fn visit_property_instance_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        property: PropertyInstanceType<'db>,
    ) {
        walk_property_instance_type(db, property, visit);
    }

    fn visit_slot_descriptor_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        descriptor: SlotDescriptorType<'db>,
    ) {
        visit.visit_type(db, descriptor.value_type(db));
    }

    fn visit_typeis_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        type_is: TypeIsType<'db>,
    ) {
        walk_typeis_type(db, type_is, visit);
    }

    fn visit_typeguard_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        type_is: TypeGuardType<'db>,
    ) {
        walk_typeguard_type(db, type_is, visit);
    }

    fn visit_typeform_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        typeform: TypeFormType<'db>,
    ) {
        walk_typeform_type(db, typeform, visit);
    }

    fn visit_subclass_of_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        subclass_of: SubclassOfType<'db>,
    ) {
        walk_subclass_of_type(db, subclass_of, visit);
    }

    fn visit_generic_alias_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        alias: GenericAlias<'db>,
    ) {
        walk_generic_alias(db, alias, visit);
    }

    fn visit_function_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        function: FunctionType<'db>,
    ) {
        walk_function_type(db, function, visit);
    }

    fn visit_bound_method_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        method: BoundMethodType<'db>,
    ) {
        walk_bound_method_type(db, method, visit);
    }

    fn visit_bound_super_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        bound_super: BoundSuperType<'db>,
    ) {
        walk_bound_super_type(db, bound_super, visit);
    }

    fn visit_nominal_instance_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        nominal: NominalInstanceType<'db>,
    ) {
        walk_nominal_instance_type(db, nominal, visit);
    }

    fn visit_bound_type_var_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) {
        walk_bound_type_var_type(db, bound_typevar, visit);
    }

    fn visit_type_var_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        typevar: TypeVarInstance<'db>,
    ) {
        walk_type_var_type(db, typevar, visit);
    }

    fn visit_protocol_instance_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        protocol: ProtocolInstanceType<'db>,
    ) {
        walk_protocol_instance_type(db, protocol, visit);
    }

    fn visit_method_wrapper_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        method_wrapper: KnownBoundMethodType<'db>,
    ) {
        walk_method_wrapper_type(db, method_wrapper, visit);
    }

    fn visit_known_instance_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        known_instance: KnownInstanceType<'db>,
    ) {
        walk_known_instance_type(db, known_instance, visit);
    }

    fn visit_type_alias_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        type_alias: TypeAliasType<'db>,
    ) {
        walk_type_alias_type(db, type_alias, visit);
    }

    fn visit_typed_dict_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        typed_dict: TypedDictType<'db>,
    ) {
        walk_typed_dict_type(db, typed_dict, visit);
    }

    fn visit_newtype_instance_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        newtype: NewType<'db>,
    ) {
        walk_newtype_instance_type(db, newtype, visit);
    }

    fn visit_recursive_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        recursive: RecursiveType<'db>,
    );
}

struct VisitRecursiveBody;

impl<'db, V: TypeVisitor<'db> + ?Sized> TypeWalk<'db, &V, StructuralTraversal>
    for VisitRecursiveBody
{
    fn recursion_key(&self) -> TypeWalkKey<'db> {
        TypeWalkKey::VisitRecursiveBody
    }

    type Output = ();

    fn on_cycle(
        &self,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        visitor: &V,
    ) {
        visitor.notify_skipped_lazy_type_attributes();
    }

    fn visit(
        &self,
        _db: &'db dyn Db,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        visitor: &V,
    ) {
        visitor.notify_skipped_lazy_type_attributes();
    }

    fn visit_recursive_body(
        &self,
        db: &'db dyn Db,
        _traversal: &StructuralTypeVisit<'_, '_, 'db, Self>,
        body: Type<'db>,
        visitor: &V,
    ) {
        visitor.visit_type(db, body);
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
    Recursive(super::RecursiveType<'db>),
    NewTypeInstance(NewType<'db>),
}

pub(super) enum TypeKind<'db> {
    Atomic,
    NonAtomic(NonAtomicType<'db>),
}

impl<'db> From<Type<'db>> for TypeKind<'db> {
    fn from(ty: Type<'db>) -> Self {
        match ty {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
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
            Type::Recursive(recursive) => TypeKind::NonAtomic(NonAtomicType::Recursive(recursive)),
            Type::NewTypeInstance(newtype) => {
                TypeKind::NonAtomic(NonAtomicType::NewTypeInstance(newtype))
            }
        }
    }
}

impl<'db, V: TypeVisitorHooks<'db>> TypeVisit<'_, 'db, V> {
    pub(crate) fn walk_children(&self, db: &'db dyn Db) {
        let TypeKind::NonAtomic(non_atomic_type) = TypeKind::from(self.ty) else {
            return;
        };
        match non_atomic_type {
            NonAtomicType::FunctionLiteral(function) => {
                self.visitor.visit_function_type(db, self, function);
            }
            NonAtomicType::Intersection(intersection) => {
                self.visitor.visit_intersection_type(db, self, intersection);
            }
            NonAtomicType::EnumComplement(complement) => {
                self.visitor
                    .visit_enum_complement_type(db, self, complement);
            }
            NonAtomicType::Union(union) => self.visitor.visit_union_type(db, self, union),
            NonAtomicType::BoundMethod(method) => {
                self.visitor.visit_bound_method_type(db, self, method);
            }
            NonAtomicType::BoundSuper(bound_super) => {
                self.visitor.visit_bound_super_type(db, self, bound_super);
            }
            NonAtomicType::MethodWrapper(method_wrapper) => {
                self.visitor
                    .visit_method_wrapper_type(db, self, method_wrapper);
            }
            NonAtomicType::Callable(callable) => {
                self.visitor.visit_callable_type(db, self, callable);
            }
            NonAtomicType::GenericAlias(alias) => {
                self.visitor.visit_generic_alias_type(db, self, alias);
            }
            NonAtomicType::KnownInstance(known_instance) => {
                self.visitor
                    .visit_known_instance_type(db, self, known_instance);
            }
            NonAtomicType::SubclassOf(subclass_of) => {
                self.visitor.visit_subclass_of_type(db, self, subclass_of);
            }
            NonAtomicType::NominalInstance(nominal) => {
                self.visitor.visit_nominal_instance_type(db, self, nominal);
            }
            NonAtomicType::PropertyInstance(property) => {
                self.visitor
                    .visit_property_instance_type(db, self, property);
            }
            NonAtomicType::SlotDescriptor(descriptor) => {
                self.visitor
                    .visit_slot_descriptor_type(db, self, descriptor);
            }
            NonAtomicType::TypeIs(type_is) => self.visitor.visit_typeis_type(db, self, type_is),
            NonAtomicType::TypeGuard(type_guard) => {
                self.visitor.visit_typeguard_type(db, self, type_guard);
            }
            NonAtomicType::TypeForm(typeform) => {
                self.visitor.visit_typeform_type(db, self, typeform);
            }
            NonAtomicType::TypeVar(bound_typevar) => {
                self.visitor
                    .visit_bound_type_var_type(db, self, bound_typevar);
            }
            NonAtomicType::ProtocolInstance(protocol) => {
                self.visitor
                    .visit_protocol_instance_type(db, self, protocol);
            }
            NonAtomicType::TypedDict(typed_dict) => {
                self.visitor.visit_typed_dict_type(db, self, typed_dict);
            }
            NonAtomicType::TypeAlias(alias) => {
                self.visitor.visit_type_alias_type(db, self, alias);
            }
            NonAtomicType::Recursive(recursive) => {
                self.visitor.visit_recursive_type(db, self, recursive);
            }
            NonAtomicType::NewTypeInstance(newtype) => {
                self.visitor.visit_newtype_instance_type(db, self, newtype);
            }
        }
    }

    pub(crate) fn walk_children_once(&self, db: &'db dyn Db, seen: &TypeCollector<'db>) {
        if matches!(TypeKind::from(self.ty), TypeKind::NonAtomic(_))
            && !seen.type_was_already_seen(self.ty)
        {
            self.walk_children(db);
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
pub(super) enum DynamicContentMode {
    All,
    NonAny,
    /// Inspect type requirements affected by materialization.
    Materialization,
}

/// Determine whether `ty` contains any dynamic type.
pub(super) fn dynamic_content<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
) -> DynamicContent {
    dynamic_content_impl(db, env, ty, DynamicContentMode::All)
}

/// Determine whether `ty` contains a dynamic type other than `Any`.
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
    env: &TypeOperations<'db>,
    ty: Type<'db>,
) -> DynamicContent {
    dynamic_content_impl(db, env, ty, DynamicContentMode::NonAny)
}

pub(super) fn dynamic_content_impl<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    mode: DynamicContentMode,
) -> DynamicContent {
    struct DynamicContentVisitor<'a, 'db> {
        env: &'a TypeOperations<'db>,
        mapping: ApplyTypeMappingVisitor<'a, 'db>,
        recursion_guard: TypeCollector<'db>,
        active_class_protocols: ActiveRecursionDetector<StaticClassLiteral<'db>>,
        active_class_typed_dicts: ActiveRecursionDetector<StaticClassLiteral<'db>>,
        active_type_aliases: ActiveRecursionDetector<Definition<'db>>,
        active_recursive_types: ActiveRecursionDetector<RecursiveType<'db>>,
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

    impl<'db> TypeVisitorHooks<'db> for DynamicContentVisitor<'_, 'db> {
        fn program_environment(&self) -> &TypeOperations<'db> {
            self.env
        }

        fn should_visit_lazy_type_attributes(&self) -> bool {
            true
        }

        fn notify_skipped_lazy_type_attributes(&self) {
            if self.content.get().is_absent() {
                self.record(DynamicContent::Indeterminate);
            }
        }

        fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, ty: Type<'db>) {
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

            visit.walk_children_once(db, &self.recursion_guard);
        }

        fn visit_callable_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            callable: CallableType<'db>,
        ) {
            if !self.content.get().is_absent() {
                return;
            }
            // Proving that a materialization is redundant is optional. Inferring a deferred
            // signature here can specialize its arguments and start another materialization.
            if matches!(self.mode, DynamicContentMode::Materialization)
                && !matches!(callable.signature_source(db), SignatureSource::Explicit(_))
            {
                self.record(DynamicContent::Indeterminate);
                return;
            }
            walk_callable_type(db, callable, visit);
        }

        fn visit_function_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            function: FunctionType<'db>,
        ) {
            if !self.content.get().is_absent() {
                return;
            }

            if matches!(self.mode, DynamicContentMode::Materialization) {
                // The ordinary walker only visits updated signatures. Inferring an original
                // signature here could re-enter recursive `TypeOf` evaluation, so do not claim
                // that materialization leaves this function's requirements unchanged.
                self.record(DynamicContent::Indeterminate);
            } else {
                walk_function_type(db, function, visit);
            }
        }

        fn visit_known_instance_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            known: KnownInstanceType<'db>,
        ) {
            if matches!(self.mode, DynamicContentMode::Materialization)
                && let KnownInstanceType::FunctoolsPartial(partial)
                | KnownInstanceType::FunctoolsPartialCall(partial) = known
            {
                // Materialization maps both the reduced signature and the wrapped callable.
                visit.visit_type(db, partial.wrapped(db).inner(db));
            }
            if self.content.get().is_absent() {
                walk_known_instance_type(db, known, visit);
            }
        }

        fn visit_generic_alias_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            alias: GenericAlias<'db>,
        ) {
            // Use `walk_specialization_types` rather than `walk_specialization` to avoid walking
            // the bounds/constraints/defaults of the generic context.
            // Only the types the class was actually specialized with are relevant to whether
            // the `GenericAlias` contains a dynamic type.
            walk_specialization_types(db, alias.specialization(db), visit);
        }

        fn visit_typeis_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            type_is: TypeIsType<'db>,
        ) {
            if type_is.materialization_kind(db).is_none() {
                walk_typeis_type(db, type_is, visit);
            }
        }

        fn visit_type_alias_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            alias: TypeAliasType<'db>,
        ) {
            self.active_type_aliases.visit(
                self.env,
                &alias.definition(db),
                || self.record(DynamicContent::Indeterminate),
                || walk_type_alias_type(db, alias, visit),
            );
        }

        fn visit_recursive_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            recursive: RecursiveType<'db>,
        ) {
            self.active_recursive_types.visit_structure(
                &recursive.constructor(db),
                &self.mapping,
                || self.record(DynamicContent::Indeterminate),
                |expansion| {
                    expansion
                        .recursive_body(db, recursive)
                        .body_or_reference()
                        .visit(db, visit);
                },
            );
        }

        fn visit_protocol_instance_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            protocol: ProtocolInstanceType<'db>,
        ) {
            let protocol_ty = Type::ProtocolInstance(protocol);
            let Some((origin, specialization)) = protocol
                .class_origin(db)
                .and_then(|class| class.static_class_literal(db))
            else {
                // The synthesized interface is immutable. A changed member produces a different
                // protocol type; lazy types inside its members are still inspected here.
                walk_protocol_instance_interface(
                    db,
                    protocol.interface(db, self.env),
                    protocol_ty,
                    visit,
                );
                return;
            };

            if let Some(specialization) = specialization {
                // Bounds and defaults in the generic context do not describe the specialized
                // instance; only inspect the types assigned to its parameters.
                for ty in specialization.types(db) {
                    visit.visit_type(db, *ty);
                    if !self.content.get().is_absent() {
                        return;
                    }
                }
            }

            self.active_class_protocols.visit(
                self.env,
                &origin,
                || self.record(DynamicContent::Indeterminate),
                || {
                    walk_protocol_instance_interface(
                        db,
                        protocol.interface(db, self.env),
                        protocol_ty,
                        visit,
                    );
                },
            );
        }

        fn visit_typed_dict_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            typed_dict: TypedDictType<'db>,
        ) {
            let Some((origin, _)) = typed_dict
                .defining_class()
                .and_then(|class| class.static_class_literal(db))
            else {
                // Synthesized schemas and inline functional TypedDict schemas are immutable
                // snapshots; definition-backed schemas re-read their tracked field queries.
                walk_typed_dict_type(db, typed_dict, visit);
                return;
            };

            self.active_class_typed_dicts.visit(
                self.env,
                &origin,
                || self.record(DynamicContent::Indeterminate),
                || walk_typed_dict_type(db, typed_dict, visit),
            );
        }
    }

    let visitor = DynamicContentVisitor {
        env,
        mapping: ApplyTypeMappingVisitor::new(env),
        recursion_guard: TypeCollector::default(),
        active_class_protocols: ActiveRecursionDetector::default(),
        active_class_typed_dicts: ActiveRecursionDetector::default(),
        active_type_aliases: ActiveRecursionDetector::default(),
        active_recursive_types: ActiveRecursionDetector::default(),
        content: Cell::new(DynamicContent::Absent),
        mode,
    };
    TypeVisitorContext::new(&visitor).visit_type(db, ty);
    visitor.content.get()
}

/// Whether inspecting `ty` can encounter recursive types with changing specializations.
///
/// Exact recursive types are safe to inspect once. Protocol methods and lambda returns can
/// introduce further specializations while their signatures are inferred, so this inspection
/// conservatively stops at another use of the same active definition.
pub(super) fn contains_growing_type<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
) -> bool {
    struct GrowingTypeVisitor<'visit, 'a, 'db> {
        recursive: &'visit StructuralTypeTraversal<'a, 'db, VisitRecursiveBody>,
        env: &'a TypeOperations<'db>,
        recursion_guard: TypeCollector<'db>,
        active_class_protocols: ActiveRecursionDetector<StaticClassLiteral<'db>>,
        found: Cell<bool>,
    }

    impl<'db> TypeVisitorHooks<'db> for GrowingTypeVisitor<'_, '_, 'db> {
        fn program_environment(&self) -> &TypeOperations<'db> {
            self.env
        }

        fn should_visit_lazy_type_attributes(&self) -> bool {
            true
        }

        fn visit_recursive_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            recursive: RecursiveType<'db>,
        ) {
            self.recursive
                .visit_with(db, Type::Recursive(recursive), visit);
        }

        fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, ty: Type<'db>) {
            if self.found.get() {
                return;
            }

            let is_generic = match ty {
                Type::TypeAlias(alias) => alias.generic_context(db).is_some(),
                Type::Recursive(recursive) => recursive.parameters(db).is_some(),
                Type::ProtocolInstance(protocol) => protocol
                    .class_origin(db)
                    .and_then(|class| class.class_literal(db).generic_context(db))
                    .is_some(),
                Type::TypedDict(typed_dict) => typed_dict
                    .defining_class()
                    .and_then(|class| class.class_literal(db).generic_context(db))
                    .is_some(),
                _ => false,
            };
            if is_generic
                && matches!(
                    ty.to_type_identity(db),
                    TypeIdentity::GrowingTypeAlias(_)
                        | TypeIdentity::GrowingRecursive { .. }
                        | TypeIdentity::GrowingProtocol(_)
                        | TypeIdentity::GrowingTypedDict(_)
                )
            {
                self.found.set(true);
                return;
            }

            visit.walk_children_once(db, &self.recursion_guard);
        }

        fn visit_protocol_instance_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            protocol: ProtocolInstanceType<'db>,
        ) {
            let protocol_ty = Type::ProtocolInstance(protocol);
            let Some((origin, specialization)) = protocol
                .class_origin(db)
                .and_then(|class| class.static_class_literal(db))
            else {
                walk_protocol_instance_interface(
                    db,
                    protocol.interface(db, self.env),
                    protocol_ty,
                    visit,
                );
                return;
            };

            if let Some(specialization) = specialization {
                // Inspect arguments before activating the definition so finite nesting such as
                // `P[P[int]]` does not look like an expanding recursive declaration.
                walk_specialization_types(db, specialization, visit);
                if self.found.get() {
                    return;
                }
            }

            self.active_class_protocols.visit(
                self.env,
                &origin,
                || self.found.set(true),
                || {
                    // Bind implicit receivers so they do not introduce recursive edges of their
                    // own. Explicitly recursive method signatures still need to be inspected.
                    walk_protocol_instance_interface(
                        db,
                        protocol.interface(db, self.env),
                        protocol_ty,
                        visit,
                    );
                },
            );
        }
    }

    StructuralTypeTraversal::with_root(env, VisitRecursiveBody, |recursive| {
        let visitor = GrowingTypeVisitor {
            recursive,
            env,
            recursion_guard: TypeCollector::default(),
            active_class_protocols: ActiveRecursionDetector::default(),
            found: Cell::new(false),
        };
        TypeVisitorContext::new(&visitor).visit_type(db, ty);
        visitor.found.get()
    })
}

#[derive(Clone, Copy)]
enum TypeSearchMode {
    SkipLazyAttributes,
    IncludeLazyAttributes,
    /// Visit alias arguments without evaluating alias bodies or other lazy attributes.
    IncludeAliasArguments,
}

impl TypeSearchMode {
    const fn should_visit_lazy_type_attributes(self) -> bool {
        matches!(self, Self::IncludeLazyAttributes)
    }

    const fn should_visit_alias_arguments(self) -> bool {
        matches!(self, Self::IncludeAliasArguments)
    }
}

/// Shared implementation for type searches.
fn any_over_type_impl<'db, F, T>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    mode: TypeSearchMode,
    query: F,
) -> T
where
    T: Copy + Default + PartialEq,
    F: Fn(Type<'db>) -> T,
{
    struct AnyOverTypeVisitor<'db, 'a, 'visit, U> {
        recursive: &'visit StructuralTypeTraversal<'a, 'db, VisitRecursiveBody>,
        env: &'a TypeOperations<'db>,
        query: &'a dyn Fn(Type<'db>) -> U,
        recursion_guard: TypeCollector<'db>,
        found_matching_type: Cell<U>,
        mode: TypeSearchMode,
    }

    impl<'db, U> TypeVisitorHooks<'db> for AnyOverTypeVisitor<'db, '_, '_, U>
    where
        U: Copy + Default + PartialEq,
    {
        fn program_environment(&self) -> &TypeOperations<'db> {
            self.env
        }

        fn should_visit_lazy_type_attributes(&self) -> bool {
            self.mode.should_visit_lazy_type_attributes()
        }

        fn visit_recursive_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            recursive: RecursiveType<'db>,
        ) {
            if self.should_visit_lazy_type_attributes() {
                self.recursive
                    .visit_with(db, Type::Recursive(recursive), visit);
            } else {
                self.notify_skipped_lazy_type_attributes();
            }
        }

        fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, ty: Type<'db>) {
            let default_value = U::default();
            let pre_existing = self.found_matching_type.get();
            if pre_existing != default_value {
                return;
            }
            let new_value = (self.query)(ty);
            self.found_matching_type.set(new_value);
            if new_value != default_value {
                return;
            }
            if self.mode.should_visit_alias_arguments() {
                let arguments = match ty {
                    Type::TypeAlias(alias)
                    | Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => {
                        Some(alias.specialization(db))
                    }
                    Type::Recursive(recursive) => Some(recursive.arguments(db)),
                    _ => None,
                };
                if let Some(arguments) = arguments {
                    if !self.recursion_guard.type_was_already_seen(ty)
                        && let Some(arguments) = arguments
                    {
                        walk_specialization_types(db, arguments, visit);
                    }
                    return;
                }
            }
            visit.walk_children_once(db, &self.recursion_guard);
        }
    }

    StructuralTypeTraversal::with_root(env, VisitRecursiveBody, |recursive| {
        let visitor = AnyOverTypeVisitor {
            recursive,
            env,
            query: &query,
            recursion_guard: TypeCollector::default(),
            found_matching_type: Cell::default(),
            mode,
        };
        TypeVisitorContext::new(&visitor).visit_type(db, ty);
        visitor.found_matching_type.get()
    })
}

/// Return `true` if `ty`, or any of the types contained in `ty`, match the closure passed in.
///
/// The function guards against infinite recursion
/// by keeping track of the non-atomic types it has already seen.
///
/// The `should_visit_lazy_type_attributes` parameter controls whether deferred type attributes
/// (value of a type alias, attributes of a class-based protocol, bounds/constraints of a typevar)
/// are visited or not.
pub(super) fn any_over_type<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    let mode = if should_visit_lazy_type_attributes {
        TypeSearchMode::IncludeLazyAttributes
    } else {
        TypeSearchMode::SkipLazyAttributes
    };
    any_over_type_impl(db, env, ty, mode, query)
}

/// Searches through alias arguments, including runtime alias objects, without evaluating alias
/// bodies or other lazy attributes.
/// This also visits arguments that the alias's value does not use.
/// Shared arguments use the same recursion guard, so their descendants are not visited repeatedly.
pub(super) fn any_over_type_including_alias_arguments<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    any_over_type_impl(db, env, ty, TypeSearchMode::IncludeAliasArguments, query)
}

/// Searches through type aliases without forcing other lazily inferred type attributes.
///
/// Revisiting a recursive alias counts as a match because its specialization can grow on each
/// visit. Distinct specializations of a nonrecursive alias remain separate, so nested uses such as
/// `Identity[Identity[int]]` are still considered finite.
pub(super) fn any_over_type_expanding_aliases<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    AliasSearch {
        env,
        query: &query,
        active_aliases: ActiveRecursionDetector::default(),
        mapping: ApplyTypeMappingVisitor::new(env),
    }
    .search(db, ty)
}

/// An alias search shares its guards while keeping each body's structural cache local.
pub(super) struct AliasSearch<'a, 'db> {
    env: &'a TypeOperations<'db>,
    query: &'a dyn Fn(Type<'db>) -> bool,
    active_aliases: ActiveRecursionDetector<TypeIdentity<'db>>,
    mapping: ApplyTypeMappingVisitor<'a, 'db>,
}

impl<'db> AliasSearch<'_, 'db> {
    pub(super) fn search(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        any_over_type(db, self.env, ty, false, |nested| {
            (self.query)(nested)
                || match nested {
                    Type::TypeAlias(alias) => self.active_aliases.visit_structure(
                        &nested.to_type_identity(db),
                        &self.mapping,
                        || true,
                        |expansion| expansion.alias_body(db, alias).search_aliases(db, self),
                    ),
                    Type::Recursive(recursive) => self.active_aliases.visit_structure(
                        &nested.to_type_identity(db),
                        &self.mapping,
                        || true,
                        |expansion| {
                            expansion
                                .recursive_body(db, recursive)
                                .body_or_reference()
                                .search_aliases(db, self)
                        },
                    ),
                    _ => false,
                }
        })
    }
}

/// Recurse into a type and calls the passed-in closure on every nested type
/// encountered, returning the first non-`None` value returned by the closure.
///
/// For example, if `ty` is `list[tuple[int, T]]` where `T` is a type variable
/// and the closure passed in is `|t| matches!(t, Type::TypeVar(_))`, then this
/// function will return `Some(T)`.
///
/// The function guards against infinite recursion
/// by keeping track of the non-atomic types it has already seen.
///
/// The `should_visit_lazy_type_attributes` parameter controls whether deferred type attributes
/// (value of a type alias, attributes of a class-based protocol, bounds/constraints of a typevar)
/// are visited or not.
pub(super) fn find_over_type<'db, T>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: impl Fn(Type<'db>) -> Option<T>,
) -> Option<T>
where
    T: Copy + PartialEq,
{
    let mode = if should_visit_lazy_type_attributes {
        TypeSearchMode::IncludeLazyAttributes
    } else {
        TypeSearchMode::SkipLazyAttributes
    };
    any_over_type_impl(db, env, ty, mode, query)
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ty_python_core::ProgramFile;

    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::known_instance::FunctoolsPartialInstance;
    use crate::types::{
        CallableType, DynamicType, InternedType, KnownInstanceType, Parameter, Parameters,
        SpecialFormType, Type,
    };

    use super::{
        CollectedTypes, DynamicContentMode, any_over_type, dynamic_content, dynamic_content_impl,
    };

    #[test]
    fn visits_callable_values_inside_wrappers() {
        let db = setup_db();
        let env = db.program_environment();
        let callable = CallableType::bottom(&db);
        let partial =
            FunctoolsPartialInstance::new(&db, InternedType::new(&db, Type::unknown()), callable);
        for wrapper in [
            KnownInstanceType::FunctoolsPartial(partial),
            KnownInstanceType::FunctoolsPartialCall(partial),
        ] {
            assert!(any_over_type(
                &db,
                &env,
                Type::KnownInstance(wrapper),
                false,
                |ty| { ty == Type::Callable(callable) }
            ));
        }
    }

    #[test]
    fn fully_static_paramspec_value_has_no_dynamic_content() {
        let db = setup_db();
        let env = db.program_environment();
        let paramspec_value = Type::paramspec_value_callable(
            &db,
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::object())
            ]),
        );
        assert!(dynamic_content(&db, &env, paramspec_value).is_absent());
    }

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

            class VariadicSetter(Protocol):
                @property
                def value(self) -> int: ...
                @value.setter
                def value(self, *values: Any) -> None: ...

            class Recursive[T](Protocol):
                @property
                def value(self) -> T: ...
                @property
                def child(self) -> Recursive[T]: ...

            callbacks: Recursive[Callbacks]
            variadic_setter: VariadicSetter
            partial_callback = partial(gradual_callback, 0)
            partial_call = partial_callback.__call__
            "#,
        )?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py")?;
        let module = ProgramFile::new(&db, file, env.program(&db));
        for name in [
            "callbacks",
            "variadic_setter",
            "partial_callback",
            "partial_call",
        ] {
            let ty = global_symbol(&db, module, name).place.expect_type();
            assert!(
                !dynamic_content_impl(&db, &env, ty, DynamicContentMode::Materialization)
                    .is_absent(),
                "{name}"
            );
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
            assert!(
                !dynamic_content_impl(&db, &env, ty, DynamicContentMode::Materialization)
                    .is_absent()
            );
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
