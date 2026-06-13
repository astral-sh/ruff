use rustc_hash::FxHashSet;

use crate::{
    Db,
    types::{
        BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType, EnumComplementType,
        GenericAlias, IntersectionType, KnownBoundMethodType, KnownInstanceType,
        NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, SubclassOfType, Type,
        TypeAliasType, TypeFormType, TypeGuardType, TypeIsType, TypeTag, TypedDictType, UnionType,
        bound_super::walk_bound_super_type,
        callable::walk_callable_type,
        class::walk_generic_alias,
        function::{FunctionType, walk_function_type},
        instance::{walk_nominal_instance_type, walk_protocol_instance_type},
        known_instance::walk_known_instance_type,
        method::{walk_bound_method_type, walk_method_wrapper_type},
        newtype::{NewType, walk_newtype_instance_type},
        set_theoretic::{walk_intersection_type, walk_union},
        subclass_of::walk_subclass_of_type,
        type_alias::walk_type_alias_type,
        type_form::walk_typeform_type,
        typed_dict::walk_typed_dict_type,
        typevar::{TypeVarInstance, walk_bound_type_var_type, walk_type_var_type},
        walk_property_instance_type, walk_typeguard_type, walk_typeis_type,
    },
};
use std::cell::{Cell, RefCell};

/// A visitor trait that recurses into nested types.
///
/// The trait does not guard against infinite recursion out of the box,
/// but it makes it easy for implementors of the trait to do so.
/// See [`any_over_type`] for an example of how to do this.
pub(crate) trait TypeVisitor<'db> {
    /// Should the visitor trigger inference of and visit lazily-inferred type attributes?
    fn should_visit_lazy_type_attributes(&self) -> bool;

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

    fn visit_property_instance_type(&self, db: &'db dyn Db, property: PropertyInstanceType<'db>) {
        walk_property_instance_type(db, property, self);
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

    fn visit_type_var_type(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) {
        walk_type_var_type(db, typevar, self);
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

    fn visit_newtype_instance_type(&self, db: &'db dyn Db, newtype: NewType<'db>) {
        walk_newtype_instance_type(db, newtype, self);
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
        match ty.tag {
            TypeTag::AlwaysFalsy
            | TypeTag::AlwaysTruthy
            | TypeTag::Never
            | TypeTag::LiteralValue
            | TypeTag::DataclassDecorator
            | TypeTag::DataclassTransformer
            | TypeTag::WrapperDescriptor
            | TypeTag::ModuleLiteral
            | TypeTag::ClassLiteral
            | TypeTag::SpecialForm
            | TypeTag::Divergent
            | TypeTag::Dynamic => TypeKind::Atomic,

            // Non-atomic types
            TypeTag::FunctionLiteral => {
                TypeKind::NonAtomic(NonAtomicType::FunctionLiteral(ty.payload()))
            }
            TypeTag::Intersection => TypeKind::NonAtomic(NonAtomicType::Intersection(ty.payload())),
            TypeTag::EnumComplement => {
                TypeKind::NonAtomic(NonAtomicType::EnumComplement(ty.payload()))
            }
            TypeTag::Union => TypeKind::NonAtomic(NonAtomicType::Union(ty.payload())),
            TypeTag::BoundMethod => TypeKind::NonAtomic(NonAtomicType::BoundMethod(ty.payload())),
            TypeTag::BoundSuper => TypeKind::NonAtomic(NonAtomicType::BoundSuper(ty.payload())),
            TypeTag::Callable => TypeKind::NonAtomic(NonAtomicType::Callable(ty.payload())),
            TypeTag::GenericAlias => TypeKind::NonAtomic(NonAtomicType::GenericAlias(ty.payload())),
            TypeTag::PropertyInstance => {
                TypeKind::NonAtomic(NonAtomicType::PropertyInstance(ty.payload()))
            }
            TypeTag::TypeVar => TypeKind::NonAtomic(NonAtomicType::TypeVar(ty.payload())),
            TypeTag::TypeIs => TypeKind::NonAtomic(NonAtomicType::TypeIs(ty.payload())),
            TypeTag::TypeGuard => TypeKind::NonAtomic(NonAtomicType::TypeGuard(ty.payload())),
            TypeTag::TypeForm => TypeKind::NonAtomic(NonAtomicType::TypeForm(ty.payload())),
            TypeTag::TypeAlias => {
                TypeKind::NonAtomic(NonAtomicType::TypeAlias(ty.as_lazy_type_alias().unwrap()))
            }
            TypeTag::NewTypeInstance => {
                TypeKind::NonAtomic(NonAtomicType::NewTypeInstance(ty.payload()))
            }
            TypeTag::KnownBoundMethod => match ty.data() {
                crate::types::TypeData::KnownBoundMethod(value) => {
                    TypeKind::NonAtomic(NonAtomicType::MethodWrapper(value))
                }
                _ => unreachable!(),
            },
            TypeTag::KnownInstance => match ty.data() {
                crate::types::TypeData::KnownInstance(value) => {
                    TypeKind::NonAtomic(NonAtomicType::KnownInstance(value))
                }
                _ => unreachable!(),
            },
            TypeTag::SubclassOf => match ty.data() {
                crate::types::TypeData::SubclassOf(value) => {
                    TypeKind::NonAtomic(NonAtomicType::SubclassOf(value))
                }
                _ => unreachable!(),
            },
            TypeTag::NominalInstance => match ty.data() {
                crate::types::TypeData::NominalInstance(value) => {
                    TypeKind::NonAtomic(NonAtomicType::NominalInstance(value))
                }
                _ => unreachable!(),
            },
            TypeTag::ProtocolInstance => match ty.data() {
                crate::types::TypeData::ProtocolInstance(value) => {
                    TypeKind::NonAtomic(NonAtomicType::ProtocolInstance(value))
                }
                _ => unreachable!(),
            },
            TypeTag::TypedDict => match ty.data() {
                crate::types::TypeData::TypedDict(value) => {
                    TypeKind::NonAtomic(NonAtomicType::TypedDict(value))
                }
                _ => unreachable!(),
            },
        }
    }
}

pub(super) fn walk_non_atomic_type<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    non_atomic_type: NonAtomicType<'db>,
    visitor: &V,
) {
    match non_atomic_type {
        NonAtomicType::FunctionLiteral(function) => visitor.visit_function_type(db, function),
        NonAtomicType::Intersection(intersection) => {
            visitor.visit_intersection_type(db, intersection);
        }
        NonAtomicType::EnumComplement(complement) => {
            visitor.visit_enum_complement_type(db, complement);
        }
        NonAtomicType::Union(union) => visitor.visit_union_type(db, union),
        NonAtomicType::BoundMethod(method) => visitor.visit_bound_method_type(db, method),
        NonAtomicType::BoundSuper(bound_super) => visitor.visit_bound_super_type(db, bound_super),
        NonAtomicType::MethodWrapper(method_wrapper) => {
            visitor.visit_method_wrapper_type(db, method_wrapper);
        }
        NonAtomicType::Callable(callable) => visitor.visit_callable_type(db, callable),
        NonAtomicType::GenericAlias(alias) => visitor.visit_generic_alias_type(db, alias),
        NonAtomicType::KnownInstance(known_instance) => {
            visitor.visit_known_instance_type(db, known_instance);
        }
        NonAtomicType::SubclassOf(subclass_of) => visitor.visit_subclass_of_type(db, subclass_of),
        NonAtomicType::NominalInstance(nominal) => visitor.visit_nominal_instance_type(db, nominal),
        NonAtomicType::PropertyInstance(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        NonAtomicType::TypeIs(type_is) => visitor.visit_typeis_type(db, type_is),
        NonAtomicType::TypeGuard(type_guard) => visitor.visit_typeguard_type(db, type_guard),
        NonAtomicType::TypeForm(typeform) => visitor.visit_typeform_type(db, typeform),
        NonAtomicType::TypeVar(bound_typevar) => {
            visitor.visit_bound_type_var_type(db, bound_typevar);
        }
        NonAtomicType::ProtocolInstance(protocol) => {
            visitor.visit_protocol_instance_type(db, protocol);
        }
        NonAtomicType::TypedDict(typed_dict) => visitor.visit_typed_dict_type(db, typed_dict),
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
pub(crate) struct TypeCollector<'db>(RefCell<FxHashSet<Type<'db>>>);

impl<'db> TypeCollector<'db> {
    pub(crate) fn type_was_already_seen(&self, ty: Type<'db>) -> bool {
        !self.0.borrow_mut().insert(ty)
    }
}

/// Implementation for `any_over_type` and `find_over_type`.
fn any_over_type_impl<'db, F, T>(
    db: &'db dyn Db,
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

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
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
            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }
    }

    let visitor = AnyOverTypeVisitor {
        query: &query,
        recursion_guard: TypeCollector::default(),
        found_matching_type: Cell::default(),
        should_visit_lazy_type_attributes,
    };
    visitor.visit_type(db, ty);
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
    db: &'db dyn Db,
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: impl Fn(Type<'db>) -> bool,
) -> bool {
    any_over_type_impl(db, ty, should_visit_lazy_type_attributes, query)
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
    ty: Type<'db>,
    should_visit_lazy_type_attributes: bool,
    query: impl Fn(Type<'db>) -> Option<T>,
) -> Option<T>
where
    T: Copy + PartialEq,
{
    any_over_type_impl(db, ty, should_visit_lazy_type_attributes, query)
}
