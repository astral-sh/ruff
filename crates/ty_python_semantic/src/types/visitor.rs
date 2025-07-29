use crate::{
    Db, FxIndexSet,
    types::{
        BoundMethodType, BoundSuperType, CallableType, GenericAlias, IntersectionType,
        KnownInstanceType, MethodWrapperKind, NominalInstanceType, PropertyInstanceType,
        ProtocolInstanceType, SubclassOfType, Type, TypeAliasType, TypeIsType, TypeVarInstance,
        UnionType,
        class::walk_generic_alias,
        function::{FunctionType, walk_function_type},
        instance::{walk_nominal_instance_type, walk_protocol_instance_type},
        subclass_of::walk_subclass_of_type,
        tuple::{TupleType, walk_tuple_type},
        walk_bound_method_type, walk_bound_super_type, walk_callable_type, walk_intersection_type,
        walk_known_instance_type, walk_method_wrapper_type, walk_property_instance_type,
        walk_type_alias_type, walk_type_var_type, walk_typeis_type, walk_union,
    },
};

/// The result returned from the [`TypeVisitor`] trait methods. You can abort the visitor by
/// returning `Err(AbortTypeVisitor)`.
pub(crate) type TypeVisitorResult = Result<(), AbortTypeVisitor>;

/// Indicates that you wish to abort visiting a type.
pub(crate) struct AbortTypeVisitor;

/// A visitor trait that recurses into nested types.
///
/// You will typically not call the methods of this trait directly; instead, call the
/// [`visit_type`] function, which handles visiting infinitely recursive types correctly.
pub(crate) trait TypeVisitor<'db> {
    fn visit_type(&mut self, db: &'db dyn Db, ty: Type<'db>) -> TypeVisitorResult;

    fn visit_union_type(&mut self, db: &'db dyn Db, union: UnionType<'db>) -> TypeVisitorResult {
        walk_union(db, union, self)
    }

    fn visit_intersection_type(
        &mut self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> TypeVisitorResult {
        walk_intersection_type(db, intersection, self)
    }

    fn visit_tuple_type(&mut self, db: &'db dyn Db, tuple: TupleType<'db>) -> TypeVisitorResult {
        walk_tuple_type(db, tuple, self)
    }

    fn visit_callable_type(
        &mut self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> TypeVisitorResult {
        walk_callable_type(db, callable, self)
    }

    fn visit_property_instance_type(
        &mut self,
        db: &'db dyn Db,
        property: PropertyInstanceType<'db>,
    ) -> TypeVisitorResult {
        walk_property_instance_type(db, property, self)
    }

    fn visit_typeis_type(
        &mut self,
        db: &'db dyn Db,
        type_is: TypeIsType<'db>,
    ) -> TypeVisitorResult {
        walk_typeis_type(db, type_is, self)
    }

    fn visit_subclass_of_type(
        &mut self,
        db: &'db dyn Db,
        subclass_of: SubclassOfType<'db>,
    ) -> TypeVisitorResult {
        walk_subclass_of_type(db, subclass_of, self)
    }

    fn visit_generic_alias_type(
        &mut self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> TypeVisitorResult {
        walk_generic_alias(db, alias, self)
    }

    fn visit_function_type(
        &mut self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> TypeVisitorResult {
        walk_function_type(db, function, self)
    }

    fn visit_bound_method_type(
        &mut self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> TypeVisitorResult {
        walk_bound_method_type(db, method, self)
    }

    fn visit_bound_super_type(
        &mut self,
        db: &'db dyn Db,
        bound_super: BoundSuperType<'db>,
    ) -> TypeVisitorResult {
        walk_bound_super_type(db, bound_super, self)
    }

    fn visit_nominal_instance_type(
        &mut self,
        db: &'db dyn Db,
        nominal: NominalInstanceType<'db>,
    ) -> TypeVisitorResult {
        walk_nominal_instance_type(db, nominal, self)
    }

    fn visit_type_var_type(
        &mut self,
        db: &'db dyn Db,
        type_var: TypeVarInstance<'db>,
    ) -> TypeVisitorResult {
        walk_type_var_type(db, type_var, self)
    }

    fn visit_protocol_instance_type(
        &mut self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
    ) -> TypeVisitorResult {
        walk_protocol_instance_type(db, protocol, self)
    }

    fn visit_method_wrapper_type(
        &mut self,
        db: &'db dyn Db,
        method_wrapper: MethodWrapperKind<'db>,
    ) -> TypeVisitorResult {
        walk_method_wrapper_type(db, method_wrapper, self)
    }

    fn visit_known_instance_type(
        &mut self,
        db: &'db dyn Db,
        known_instance: KnownInstanceType<'db>,
    ) -> TypeVisitorResult {
        walk_known_instance_type(db, known_instance, self)
    }

    fn visit_type_alias_type(
        &mut self,
        db: &'db dyn Db,
        type_alias: TypeAliasType<'db>,
    ) -> TypeVisitorResult {
        walk_type_alias_type(db, type_alias, self)
    }
}

/// Enumeration of types that may contain other types, such as unions, intersections, and generics.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum NonAtomicType<'db> {
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    Tuple(TupleType<'db>),
    FunctionLiteral(FunctionType<'db>),
    BoundMethod(BoundMethodType<'db>),
    BoundSuper(BoundSuperType<'db>),
    MethodWrapper(MethodWrapperKind<'db>),
    Callable(CallableType<'db>),
    GenericAlias(GenericAlias<'db>),
    KnownInstance(KnownInstanceType<'db>),
    SubclassOf(SubclassOfType<'db>),
    NominalInstance(NominalInstanceType<'db>),
    PropertyInstance(PropertyInstanceType<'db>),
    TypeIs(TypeIsType<'db>),
    TypeVar(TypeVarInstance<'db>),
    ProtocolInstance(ProtocolInstanceType<'db>),
}

enum TypeKind<'db> {
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
            Type::Tuple(tuple) => TypeKind::NonAtomic(NonAtomicType::Tuple(tuple)),
            Type::BoundMethod(method) => TypeKind::NonAtomic(NonAtomicType::BoundMethod(method)),
            Type::BoundSuper(bound_super) => {
                TypeKind::NonAtomic(NonAtomicType::BoundSuper(bound_super))
            }
            Type::MethodWrapper(method_wrapper) => {
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
            Type::TypeVar(type_var) => TypeKind::NonAtomic(NonAtomicType::TypeVar(type_var)),
            Type::TypeIs(type_is) => TypeKind::NonAtomic(NonAtomicType::TypeIs(type_is)),
        }
    }
}

fn walk_non_atomic_type<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    non_atomic_type: NonAtomicType<'db>,
    visitor: &mut V,
) -> TypeVisitorResult {
    match non_atomic_type {
        NonAtomicType::FunctionLiteral(function) => visitor.visit_function_type(db, function),
        NonAtomicType::Intersection(intersection) => {
            visitor.visit_intersection_type(db, intersection)
        }
        NonAtomicType::Union(union) => visitor.visit_union_type(db, union),
        NonAtomicType::Tuple(tuple) => visitor.visit_tuple_type(db, tuple),
        NonAtomicType::BoundMethod(method) => visitor.visit_bound_method_type(db, method),
        NonAtomicType::BoundSuper(bound_super) => visitor.visit_bound_super_type(db, bound_super),
        NonAtomicType::MethodWrapper(method_wrapper) => {
            visitor.visit_method_wrapper_type(db, method_wrapper)
        }
        NonAtomicType::Callable(callable) => visitor.visit_callable_type(db, callable),
        NonAtomicType::GenericAlias(alias) => visitor.visit_generic_alias_type(db, alias),
        NonAtomicType::KnownInstance(known_instance) => {
            visitor.visit_known_instance_type(db, known_instance)
        }
        NonAtomicType::SubclassOf(subclass_of) => visitor.visit_subclass_of_type(db, subclass_of),
        NonAtomicType::NominalInstance(nominal) => visitor.visit_nominal_instance_type(db, nominal),
        NonAtomicType::PropertyInstance(property) => {
            visitor.visit_property_instance_type(db, property)
        }
        NonAtomicType::TypeIs(type_is) => visitor.visit_typeis_type(db, type_is),
        NonAtomicType::TypeVar(type_var) => visitor.visit_type_var_type(db, type_var),
        NonAtomicType::ProtocolInstance(protocol) => {
            visitor.visit_protocol_instance_type(db, protocol)
        }
    }
}

/// Visits a type while guarding against infinite recursion. This lets you write a [`TypeVisitor`]
/// without having to track which types have already been seen. We guarantee that your
/// [`visit_type`][TypeVisitor::visit_type] trait method will only be called once for each distinct
/// non-atomic type that is encountered.
pub(super) fn visit_type<'a, 'db, V>(db: &'db dyn Db, wrapped: &'a mut V, ty: Type<'db>)
where
    V: TypeVisitor<'db>,
{
    struct RecursionGuard<'a, 'db, V> {
        wrapped: &'a mut V,
        seen_types: FxIndexSet<NonAtomicType<'db>>,
    }

    impl<'db, V> TypeVisitor<'db> for RecursionGuard<'_, 'db, V>
    where
        V: TypeVisitor<'db>,
    {
        fn visit_type(&mut self, db: &'db dyn Db, ty: Type<'db>) -> TypeVisitorResult {
            self.wrapped.visit_type(db, ty)?;
            match TypeKind::from(ty) {
                TypeKind::Atomic => {}
                TypeKind::NonAtomic(non_atomic_type) => {
                    if self.seen_types.insert(non_atomic_type) {
                        // If we haven't already seen this type, we should recurse into it.
                        walk_non_atomic_type(db, non_atomic_type, self)?;
                    }
                }
            }
            Ok(())
        }
    }

    let mut visitor = RecursionGuard {
        wrapped,
        seen_types: FxIndexSet::default(),
    };
    let _ = visitor.visit_type(db, ty);
}

/// Return `true` if `ty`, or any of the types contained in `ty`, match the closure passed in.
pub(super) fn any_over_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    query: &'db dyn Fn(Type<'db>) -> bool,
) -> bool {
    struct AnyOverTypeVisitor<'db, 'a> {
        query: &'a dyn Fn(Type<'db>) -> bool,
        found_matching_type: bool,
    }

    impl<'db> TypeVisitor<'db> for AnyOverTypeVisitor<'db, '_> {
        fn visit_type(&mut self, _db: &'db dyn Db, ty: Type<'db>) -> TypeVisitorResult {
            if self.found_matching_type {
                return Err(AbortTypeVisitor);
            }
            self.found_matching_type |= (self.query)(ty);
            if self.found_matching_type {
                return Err(AbortTypeVisitor);
            }
            Ok(())
        }
    }

    let mut visitor = AnyOverTypeVisitor {
        query,
        found_matching_type: false,
    };
    visit_type(db, &mut visitor, ty);
    visitor.found_matching_type
}
