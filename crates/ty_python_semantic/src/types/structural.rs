//! Transform stored type syntax without expanding definitions or comparing types.

use super::{
    ClassType, InternedType, IntersectionType, KnownBoundMethodType, SlotDescriptorType, Type,
    TypeFormType, UnionType,
};
use crate::{Db, FxOrderSet};

/// Replaces children of a type while preserving its constructor and stored metadata.
/// Implementations decide whether to recurse by calling `Type::map_structure`.
pub(super) trait StructuralTypeMapper<'db>: Sized {
    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db>;

    fn map_class(&self, db: &'db dyn Db, class: ClassType<'db>) -> ClassType<'db> {
        match class {
            ClassType::NonGeneric(_) => class,
            ClassType::Generic(alias) => ClassType::Generic(alias.map_structure(db, self)),
        }
    }
}

impl<'db> Type<'db> {
    /// Maps stored children without evaluating lazy definitions or normalizing open types.
    pub(super) fn map_structure(
        self,
        db: &'db dyn Db,
        mapper: &impl StructuralTypeMapper<'db>,
    ) -> Self {
        match self {
            Type::Recursive(recursive) => Type::Recursive(recursive.map_structure(db, mapper)),
            Type::RecursiveVar(variable) => Type::RecursiveVar(variable.map_structure(db, mapper)),
            Type::FunctionLiteral(function) => {
                Type::FunctionLiteral(function.map_structure(db, mapper))
            }
            Type::Callable(callable) => Type::Callable(callable.map_structure(db, mapper)),
            Type::BoundMethod(method) => Type::BoundMethod(method.map_structure(db, mapper)),
            Type::NominalInstance(instance) => {
                Type::NominalInstance(instance.map_structure(db, mapper))
            }
            Type::NewTypeInstance(newtype) => Type::NewTypeInstance(
                newtype.map_base_class_type(db, |class| mapper.map_class(db, class)),
            ),
            Type::ProtocolInstance(protocol) => {
                Type::ProtocolInstance(protocol.map_structure(db, mapper))
            }
            Type::GenericAlias(alias) => Type::GenericAlias(alias.map_structure(db, mapper)),
            Type::TypedDict(typed_dict) => Type::TypedDict(typed_dict.map_structure(db, mapper)),
            Type::SubclassOf(subclass) => subclass.map_structure(db, mapper),
            Type::PropertyInstance(property) => {
                Type::PropertyInstance(property.map_structure(db, mapper))
            }
            Type::SlotDescriptor(descriptor) => Type::SlotDescriptor(SlotDescriptorType::new(
                db,
                mapper.map_type(db, descriptor.value_type(db)),
            )),
            Type::Union(union) => UnionType::from_elements_structural(
                db,
                union.elements(db).iter().map(|ty| mapper.map_type(db, *ty)),
                union.recursively_defined(db),
            ),
            Type::Intersection(intersection) => Type::Intersection(IntersectionType::new(
                db,
                intersection
                    .positive(db)
                    .iter()
                    .map(|ty| mapper.map_type(db, *ty))
                    .collect::<FxOrderSet<_>>(),
                intersection.negative(db).map(|ty| mapper.map_type(db, *ty)),
            )),
            Type::EnumComplement(complement) => {
                Type::EnumComplement(complement.map_structure(db, mapper))
            }
            Type::TypeIs(type_is) => type_is.map_structure(db, mapper),
            Type::TypeGuard(type_guard) => {
                type_guard.with_type(db, mapper.map_type(db, type_guard.return_type(db)))
            }
            Type::TypeForm(type_form) => TypeFormType::from_type_expression(
                db,
                mapper.map_type(db, type_form.type_argument(db)),
            ),
            Type::TypeAlias(alias) => Type::TypeAlias(alias.map_structure(db, mapper)),
            Type::KnownInstance(instance) => instance.map_structure(db, mapper),
            Type::KnownBoundMethod(method) => Type::KnownBoundMethod(match method {
                KnownBoundMethodType::FunctionTypeDunderGet(function) => {
                    KnownBoundMethodType::FunctionTypeDunderGet(InternedType::new(
                        db,
                        mapper.map_type(db, function.inner(db)),
                    ))
                }
                KnownBoundMethodType::DunderCall(callable) => KnownBoundMethodType::DunderCall(
                    InternedType::new(db, mapper.map_type(db, callable.inner(db))),
                ),
                KnownBoundMethodType::MethodTypeDunderGet(method) => {
                    KnownBoundMethodType::MethodTypeDunderGet(method.map_structure(db, mapper))
                }
                KnownBoundMethodType::PropertyDunderGet(property) => {
                    KnownBoundMethodType::PropertyDunderGet(property.map_structure(db, mapper))
                }
                KnownBoundMethodType::PropertyDunderSet(property) => {
                    KnownBoundMethodType::PropertyDunderSet(property.map_structure(db, mapper))
                }
                KnownBoundMethodType::PropertyDunderDelete(property) => {
                    KnownBoundMethodType::PropertyDunderDelete(property.map_structure(db, mapper))
                }
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
                | KnownBoundMethodType::ConstraintSetIsAlwaysSatisfied(_)
                | KnownBoundMethodType::ConstraintSetIsNeverSatisfied(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => method,
            }),
            Type::Dynamic(_)
            | Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::Divergent(_)
            | Type::TypeVar(_)
            | Type::LiteralValue(_)
            | Type::ClassLiteral(_)
            | Type::WrapperDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_) => self,
        }
    }
}
