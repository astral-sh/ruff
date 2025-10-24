use rustc_hash::FxHashMap;

use crate::{
    Db, FxIndexSet,
    types::{
        BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType, GenericAlias,
        IntersectionType, KnownBoundMethodType, KnownInstanceType, NominalInstanceType,
        PropertyInstanceType, ProtocolInstanceType, SubclassOfType, Type, TypeAliasType,
        TypeIsType, TypeVarInstance, TypedDictType, UnionType,
        bound_super::walk_bound_super_type,
        class::walk_generic_alias,
        function::{FunctionType, walk_function_type},
        instance::{walk_nominal_instance_type, walk_protocol_instance_type},
        subclass_of::walk_subclass_of_type,
        walk_bound_method_type, walk_bound_type_var_type, walk_callable_type,
        walk_intersection_type, walk_known_instance_type, walk_method_wrapper_type,
        walk_property_instance_type, walk_type_alias_type, walk_type_var_type,
        walk_typed_dict_type, walk_typeis_type, walk_union,
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::hash_map::Entry,
};

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

    fn visit_callable_type(&self, db: &'db dyn Db, callable: CallableType<'db>) {
        walk_callable_type(db, callable, self);
    }

    fn visit_property_instance_type(&self, db: &'db dyn Db, property: PropertyInstanceType<'db>) {
        walk_property_instance_type(db, property, self);
    }

    fn visit_typeis_type(&self, db: &'db dyn Db, type_is: TypeIsType<'db>) {
        walk_typeis_type(db, type_is, self);
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
}

/// Enumeration of types that may contain other types, such as unions, intersections, and generics.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum NonAtomicType<'db> {
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
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
    TypeVar(BoundTypeVarInstance<'db>),
    ProtocolInstance(ProtocolInstanceType<'db>),
    TypedDict(TypedDictType<'db>),
    TypeAlias(TypeAliasType<'db>),
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
            | Type::LiteralString
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::WrapperDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::SpecialForm(_)
            | Type::Dynamic(_) => TypeKind::Atomic,

            // Non-atomic types
            Type::FunctionLiteral(function) => {
                TypeKind::NonAtomic(NonAtomicType::FunctionLiteral(function))
            }
            Type::Intersection(intersection) => {
                TypeKind::NonAtomic(NonAtomicType::Intersection(intersection))
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
            Type::TypedDict(typed_dict) => {
                TypeKind::NonAtomic(NonAtomicType::TypedDict(typed_dict))
            }
            Type::TypeAlias(alias) => TypeKind::NonAtomic(NonAtomicType::TypeAlias(alias)),
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
    }
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
    query: &dyn Fn(Type<'db>) -> bool,
    should_visit_lazy_type_attributes: bool,
) -> bool {
    struct AnyOverTypeVisitor<'db, 'a> {
        query: &'a dyn Fn(Type<'db>) -> bool,
        seen_types: RefCell<FxIndexSet<NonAtomicType<'db>>>,
        found_matching_type: Cell<bool>,
        should_visit_lazy_type_attributes: bool,
    }

    impl<'db> TypeVisitor<'db> for AnyOverTypeVisitor<'db, '_> {
        fn should_visit_lazy_type_attributes(&self) -> bool {
            self.should_visit_lazy_type_attributes
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            let already_found = self.found_matching_type.get();
            if already_found {
                return;
            }
            let found = already_found | (self.query)(ty);
            self.found_matching_type.set(found);
            if found {
                return;
            }
            match TypeKind::from(ty) {
                TypeKind::Atomic => {}
                TypeKind::NonAtomic(non_atomic_type) => {
                    if !self.seen_types.borrow_mut().insert(non_atomic_type) {
                        // If we have already seen this type, we can skip it.
                        return;
                    }
                    walk_non_atomic_type(db, non_atomic_type, self);
                }
            }
        }
    }

    let visitor = AnyOverTypeVisitor {
        query,
        seen_types: RefCell::new(FxIndexSet::default()),
        found_matching_type: Cell::new(false),
        should_visit_lazy_type_attributes,
    };
    visitor.visit_type(db, ty);
    visitor.found_matching_type.get()
}

/// Returns the maximum number of layers of generic specializations for a given type.
///
/// For example, `int` has a depth of `0`, `list[int]` has a depth of `1`, and `list[set[int]]`
/// has a depth of `2`. A set-theoretic type like `list[int] | list[list[int]]` has a maximum
/// depth of `2`.
fn specialization_depth(db: &dyn Db, ty: Type<'_>) -> usize {
    #[derive(Debug, Default)]
    struct SpecializationDepthVisitor<'db> {
        seen_types: RefCell<FxHashMap<NonAtomicType<'db>, Option<usize>>>,
        max_depth: Cell<usize>,
    }

    impl<'db> TypeVisitor<'db> for SpecializationDepthVisitor<'db> {
        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            match TypeKind::from(ty) {
                TypeKind::Atomic => {
                    if ty.is_divergent() {
                        self.max_depth.set(usize::MAX);
                    }
                }
                TypeKind::NonAtomic(non_atomic_type) => {
                    match self.seen_types.borrow_mut().entry(non_atomic_type) {
                        Entry::Occupied(cached_depth) => {
                            self.max_depth
                                .update(|current| current.max(cached_depth.get().unwrap_or(0)));
                            return;
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(None);
                        }
                    }

                    let self_depth: usize =
                        matches!(non_atomic_type, NonAtomicType::GenericAlias(_)).into();

                    let previous_max_depth = self.max_depth.replace(0);
                    walk_non_atomic_type(db, non_atomic_type, self);

                    self.max_depth.update(|max_child_depth| {
                        previous_max_depth.max(max_child_depth.saturating_add(self_depth))
                    });

                    self.seen_types
                        .borrow_mut()
                        .insert(non_atomic_type, Some(self.max_depth.get()));
                }
            }
        }
    }

    let visitor = SpecializationDepthVisitor::default();
    visitor.visit_type(db, ty);
    visitor.max_depth.get()
}

pub(super) fn exceeds_max_specialization_depth(db: &dyn Db, ty: Type<'_>) -> bool {
    // To prevent infinite recursion during type inference for infinite types, we fall back to
    // `C[Divergent]` once a certain amount of levels of specialization have occurred. For
    // example:
    //
    // ```py
    // x = 1
    // while random_bool():
    //     x = [x]
    //
    // reveal_type(x)  # Unknown | Literal[1] | list[Divergent]
    // ```
    const MAX_SPECIALIZATION_DEPTH: usize = 10;

    specialization_depth(db, ty) > MAX_SPECIALIZATION_DEPTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::tests::setup_db, types::KnownClass};

    #[test]
    fn test_generics_layering_depth() {
        let db = setup_db();

        let int = || KnownClass::Int.to_instance(&db);
        let list = |element| KnownClass::List.to_specialized_instance(&db, [element]);
        let dict = |key, value| KnownClass::Dict.to_specialized_instance(&db, [key, value]);
        let set = |element| KnownClass::Set.to_specialized_instance(&db, [element]);
        let str = || KnownClass::Str.to_instance(&db);
        let bytes = || KnownClass::Bytes.to_instance(&db);

        let list_of_int = list(int());
        assert_eq!(specialization_depth(&db, list_of_int), 1);

        let list_of_list_of_int = list(list_of_int);
        assert_eq!(specialization_depth(&db, list_of_list_of_int), 2);

        let list_of_list_of_list_of_int = list(list_of_list_of_int);
        assert_eq!(specialization_depth(&db, list_of_list_of_list_of_int), 3);

        assert_eq!(specialization_depth(&db, set(dict(str(), list_of_int))), 3);

        assert_eq!(
            specialization_depth(
                &db,
                UnionType::from_elements(&db, [list_of_list_of_list_of_int, list_of_list_of_int])
            ),
            3
        );

        assert_eq!(
            specialization_depth(
                &db,
                UnionType::from_elements(&db, [list_of_list_of_int, list_of_list_of_list_of_int])
            ),
            3
        );

        assert_eq!(
            specialization_depth(
                &db,
                Type::heterogeneous_tuple(&db, [Type::heterogeneous_tuple(&db, [int()])])
            ),
            2
        );

        assert_eq!(
            specialization_depth(&db, Type::heterogeneous_tuple(&db, [list_of_int, str()])),
            2
        );

        assert_eq!(
            specialization_depth(
                &db,
                list(UnionType::from_elements(
                    &db,
                    [list(int()), list(str()), list(bytes())]
                ))
            ),
            2
        );
    }
}
