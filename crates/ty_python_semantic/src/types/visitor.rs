use crate::SemanticContext;
use std::cell::{Cell, RefCell};
use std::hash::Hash;

use rustc_hash::{FxBuildHasher, FxHashSet};
use smallvec::SmallVec;

use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType, EnumComplementType,
    GenericAlias, IntersectionType, KnownBoundMethodType, KnownInstanceType, NominalInstanceType,
    PropertyInstanceType, ProtocolInstanceType, StaticClassLiteral, SubclassOfType, Type,
    TypeAliasType, TypeFormType, TypeGuardType, TypeIsType, TypedDictType, UnionType,
    bound_super::walk_bound_super_type,
    callable::walk_callable_type,
    class::walk_generic_alias,
    cyclic::ActiveRecursionDetector,
    function::{FunctionType, walk_function_type},
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

/// A visitor trait that recurses into nested types.
///
/// The trait does not guard against infinite recursion out of the box,
/// but it makes it easy for implementors of the trait to do so.
/// See [`any_over_type`] for an example of how to do this.
pub(crate) trait TypeVisitor<'db> {
    /// Should the visitor trigger inference of and visit lazily-inferred type attributes?
    fn should_visit_lazy_type_attributes(&self) -> bool;

    fn visit_type(&self, ctx: &SemanticContext<'db>, ty: Type<'db>);

    fn visit_union_type(&self, ctx: &SemanticContext<'db>, union: UnionType<'db>) {
        walk_union(ctx, union, self);
    }

    fn visit_intersection_type(
        &self,
        ctx: &SemanticContext<'db>,
        intersection: IntersectionType<'db>,
    ) {
        walk_intersection_type(ctx, intersection, self);
    }

    fn visit_enum_complement_type(
        &self,
        ctx: &SemanticContext<'db>,
        complement: EnumComplementType<'db>,
    ) {
        let db = ctx.db();
        for rest in complement.rest(db) {
            self.visit_type(ctx, *rest);
        }
    }

    fn visit_callable_type(&self, ctx: &SemanticContext<'db>, callable: CallableType<'db>) {
        walk_callable_type(ctx, callable, self);
    }

    fn visit_property_instance_type(
        &self,
        ctx: &SemanticContext<'db>,
        property: PropertyInstanceType<'db>,
    ) {
        walk_property_instance_type(ctx, property, self);
    }

    fn visit_typeis_type(&self, ctx: &SemanticContext<'db>, type_is: TypeIsType<'db>) {
        walk_typeis_type(ctx, type_is, self);
    }

    fn visit_typeguard_type(&self, ctx: &SemanticContext<'db>, type_is: TypeGuardType<'db>) {
        walk_typeguard_type(ctx, type_is, self);
    }

    fn visit_typeform_type(&self, ctx: &SemanticContext<'db>, typeform: TypeFormType<'db>) {
        walk_typeform_type(ctx, typeform, self);
    }

    fn visit_subclass_of_type(&self, ctx: &SemanticContext<'db>, subclass_of: SubclassOfType<'db>) {
        walk_subclass_of_type(ctx, subclass_of, self);
    }

    fn visit_generic_alias_type(&self, ctx: &SemanticContext<'db>, alias: GenericAlias<'db>) {
        walk_generic_alias(ctx, alias, self);
    }

    fn visit_function_type(&self, ctx: &SemanticContext<'db>, function: FunctionType<'db>) {
        walk_function_type(ctx, function, self);
    }

    fn visit_bound_method_type(&self, ctx: &SemanticContext<'db>, method: BoundMethodType<'db>) {
        walk_bound_method_type(ctx, method, self);
    }

    fn visit_bound_super_type(&self, ctx: &SemanticContext<'db>, bound_super: BoundSuperType<'db>) {
        walk_bound_super_type(ctx, bound_super, self);
    }

    fn visit_nominal_instance_type(
        &self,
        ctx: &SemanticContext<'db>,
        nominal: NominalInstanceType<'db>,
    ) {
        walk_nominal_instance_type(ctx, nominal, self);
    }

    fn visit_bound_type_var_type(
        &self,
        ctx: &SemanticContext<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) {
        walk_bound_type_var_type(ctx, bound_typevar, self);
    }

    fn visit_type_var_type(&self, ctx: &SemanticContext<'db>, typevar: TypeVarInstance<'db>) {
        walk_type_var_type(ctx, typevar, self);
    }

    fn visit_protocol_instance_type(
        &self,
        ctx: &SemanticContext<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) {
        walk_protocol_instance_type(ctx, protocol, self);
    }

    fn visit_method_wrapper_type(
        &self,
        ctx: &SemanticContext<'db>,
        method_wrapper: KnownBoundMethodType<'db>,
    ) {
        walk_method_wrapper_type(ctx, method_wrapper, self);
    }

    fn visit_known_instance_type(
        &self,
        ctx: &SemanticContext<'db>,
        known_instance: KnownInstanceType<'db>,
    ) {
        walk_known_instance_type(ctx, known_instance, self);
    }

    fn visit_type_alias_type(&self, ctx: &SemanticContext<'db>, type_alias: TypeAliasType<'db>) {
        walk_type_alias_type(ctx, type_alias, self);
    }

    fn visit_typed_dict_type(&self, ctx: &SemanticContext<'db>, typed_dict: TypedDictType<'db>) {
        walk_typed_dict_type(ctx, typed_dict, self);
    }

    fn visit_newtype_instance_type(&self, ctx: &SemanticContext<'db>, newtype: NewType<'db>) {
        walk_newtype_instance_type(ctx, newtype, self);
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
    ctx: &SemanticContext<'db>,
    non_atomic_type: NonAtomicType<'db>,
    visitor: &V,
) {
    match non_atomic_type {
        NonAtomicType::FunctionLiteral(function) => {
            visitor.visit_function_type(ctx, function);
        }
        NonAtomicType::Intersection(intersection) => {
            visitor.visit_intersection_type(ctx, intersection);
        }
        NonAtomicType::EnumComplement(complement) => {
            visitor.visit_enum_complement_type(ctx, complement);
        }
        NonAtomicType::Union(union) => visitor.visit_union_type(ctx, union),
        NonAtomicType::BoundMethod(method) => {
            visitor.visit_bound_method_type(ctx, method);
        }
        NonAtomicType::BoundSuper(bound_super) => {
            visitor.visit_bound_super_type(ctx, bound_super);
        }
        NonAtomicType::MethodWrapper(method_wrapper) => {
            visitor.visit_method_wrapper_type(ctx, method_wrapper);
        }
        NonAtomicType::Callable(callable) => {
            visitor.visit_callable_type(ctx, callable);
        }
        NonAtomicType::GenericAlias(alias) => {
            visitor.visit_generic_alias_type(ctx, alias);
        }
        NonAtomicType::KnownInstance(known_instance) => {
            visitor.visit_known_instance_type(ctx, known_instance);
        }
        NonAtomicType::SubclassOf(subclass_of) => {
            visitor.visit_subclass_of_type(ctx, subclass_of);
        }
        NonAtomicType::NominalInstance(nominal) => {
            visitor.visit_nominal_instance_type(ctx, nominal);
        }
        NonAtomicType::PropertyInstance(property) => {
            visitor.visit_property_instance_type(ctx, property);
        }
        NonAtomicType::TypeIs(type_is) => visitor.visit_typeis_type(ctx, type_is),
        NonAtomicType::TypeGuard(type_guard) => {
            visitor.visit_typeguard_type(ctx, type_guard);
        }
        NonAtomicType::TypeForm(typeform) => {
            visitor.visit_typeform_type(ctx, typeform);
        }
        NonAtomicType::TypeVar(bound_typevar) => {
            visitor.visit_bound_type_var_type(ctx, bound_typevar);
        }
        NonAtomicType::ProtocolInstance(protocol) => {
            visitor.visit_protocol_instance_type(ctx, protocol);
        }
        NonAtomicType::TypedDict(typed_dict) => {
            visitor.visit_typed_dict_type(ctx, typed_dict);
        }
        NonAtomicType::TypeAlias(alias) => {
            visitor.visit_type_alias_type(ctx, alias);
        }
        NonAtomicType::NewTypeInstance(newtype) => {
            visitor.visit_newtype_instance_type(ctx, newtype);
        }
    }
}

pub(crate) fn walk_type_with_recursion_guard<'db>(
    ctx: &SemanticContext<'db>,
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
            walk_non_atomic_type(ctx, non_atomic_type, visitor);
        }
    }
}

#[derive(Default, Debug)]
pub(crate) struct TypeCollector<'db>(RefCell<CollectedTypes<'db>>);

impl<'db> TypeCollector<'db> {
    pub(crate) fn type_was_already_seen(&self, ty: Type<'db>) -> bool {
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
    pub(super) fn insert(&mut self, value: T) -> bool
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
    pub(super) const fn is_spilled(&self) -> bool {
        matches!(self, Self::Spilled(_))
    }
}

/// Whether a type contains a non-`Any` dynamic type.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum DynamicContent {
    /// The type was fully inspected and contains no non-`Any` dynamic type.
    Absent,
    /// The type contains a non-`Any` dynamic type.
    Present,
    /// Recursive specialization prevented the type from being fully inspected.
    Indeterminate,
}

impl DynamicContent {
    pub(super) const fn is_absent(self) -> bool {
        matches!(self, Self::Absent)
    }
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
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
) -> DynamicContent {
    struct DynamicContentVisitor<'db> {
        recursion_guard: TypeCollector<'db>,
        active_class_protocols: ActiveRecursionDetector<StaticClassLiteral<'db>>,
        content: Cell<DynamicContent>,
    }

    impl DynamicContentVisitor<'_> {
        fn record(&self, content: DynamicContent) {
            debug_assert!(self.content.get().is_absent());
            debug_assert!(!content.is_absent());
            self.content.set(content);
        }
    }

    impl<'db> TypeVisitor<'db> for DynamicContentVisitor<'db> {
        fn should_visit_lazy_type_attributes(&self) -> bool {
            true
        }

        fn visit_type(&self, ctx: &SemanticContext<'db>, ty: Type<'db>) {
            if !self.content.get().is_absent() {
                return;
            }

            if ty.is_dynamic() && !matches!(ty, Type::Dynamic(crate::types::DynamicType::Any)) {
                self.record(DynamicContent::Present);
                return;
            }

            walk_type_with_recursion_guard(ctx, ty, self, &self.recursion_guard);
        }

        fn visit_protocol_instance_type(
            &self,
            ctx: &SemanticContext<'db>,
            protocol: ProtocolInstanceType<'db>,
        ) {
            let db = ctx.db();
            let protocol_ty = Type::ProtocolInstance(protocol);
            let Some(class) = protocol.as_class_based() else {
                walk_protocol_instance_interface(ctx, protocol.interface(ctx), protocol_ty, self);
                return;
            };
            let Some((origin, specialization)) = class.static_class_literal(db) else {
                walk_protocol_instance_interface(ctx, protocol.interface(ctx), protocol_ty, self);
                return;
            };

            if let Some(specialization) = specialization {
                // Bounds and defaults in the generic context do not describe the specialized
                // instance; only inspect the types assigned to its parameters.
                for ty in specialization.types(db) {
                    self.visit_type(ctx, *ty);
                    if !self.content.get().is_absent() {
                        return;
                    }
                }
            }

            self.active_class_protocols.visit(
                &origin,
                || self.record(DynamicContent::Indeterminate),
                || {
                    walk_protocol_instance_interface(
                        ctx,
                        protocol.interface(ctx),
                        protocol_ty,
                        self,
                    );
                },
            );
        }
    }

    let visitor = DynamicContentVisitor {
        recursion_guard: TypeCollector::default(),
        active_class_protocols: ActiveRecursionDetector::default(),
        content: Cell::new(DynamicContent::Absent),
    };
    visitor.visit_type(ctx, ty);
    visitor.content.get()
}

/// Implementation for `any_over_type` and `find_over_type`.
fn any_over_type_impl<'db, F, T>(
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: F,
) -> T
where
    T: Copy + Default + PartialEq,
    F: Fn(Type<'db>) -> T,
{
    struct AnyOverTypeVisitor<'db, 'a, U> {
        query: &'a dyn Fn(Type<'db>) -> U,
        recursion_guard: TypeCollector<'db>,
        found_matching_type: Cell<U>,
        should_visit_lazy_type_attributes: bool,
    }

    impl<'db, U> TypeVisitor<'db> for AnyOverTypeVisitor<'db, '_, U>
    where
        U: Copy + Default + PartialEq,
    {
        fn should_visit_lazy_type_attributes(&self) -> bool {
            self.should_visit_lazy_type_attributes
        }

        fn visit_type(&self, ctx: &SemanticContext<'db>, ty: Type<'db>) {
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
            walk_type_with_recursion_guard(ctx, ty, self, &self.recursion_guard);
        }
    }

    let visitor = AnyOverTypeVisitor {
        query: &query,
        recursion_guard: TypeCollector::default(),
        found_matching_type: Cell::default(),
        should_visit_lazy_type_attributes,
    };
    visitor.visit_type(ctx, ty);
    visitor.found_matching_type.get()
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
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    any_over_type_impl(ctx, ty, should_visit_lazy_type_attributes, query)
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
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: impl Fn(Type<'db>) -> Option<T>,
) -> Option<T>
where
    T: Copy + PartialEq,
{
    any_over_type_impl(ctx, ty, should_visit_lazy_type_attributes, query)
}

#[cfg(test)]
mod tests {
    use crate::types::{DynamicType, SpecialFormType, Type};

    use super::CollectedTypes;

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
