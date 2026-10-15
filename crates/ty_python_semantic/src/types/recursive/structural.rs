//! Rewrite stored recursive references without evaluating type relationships or deferred bodies.

use std::cell::RefCell;

use rustc_hash::FxHashMap;

use super::{RecursiveBody, RecursiveSubstitution, RecursiveType, RecursiveVar};
use crate::types::{
    ClassType, InternedType, IntersectionType, KnownBoundMethodType, SlotDescriptorType, Type,
    TypeFormType, UnionType,
};
use crate::{Db, FxOrderSet};

/// A substitution within a recursive binder's stored syntax.
///
/// This mapper has no semantic operation owner. Its helpers can rebuild stored references,
/// but cannot expand definitions, compare types, or normalize receiver constraints.
pub(in crate::types) struct RecursiveReferenceMapper<'db> {
    substitution: RecursiveSubstitution<'db>,
    mapped: RefCell<FxHashMap<Type<'db>, Type<'db>>>,
}

impl<'db> RecursiveReferenceMapper<'db> {
    pub(super) fn new(substitution: RecursiveSubstitution<'db>) -> Self {
        Self {
            substitution,
            mapped: RefCell::default(),
        }
    }

    pub(in crate::types) fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        if let Some(mapped) = self.mapped.borrow().get(&ty) {
            return *mapped;
        }
        let mapped = self.map_type_inner(db, ty);
        self.mapped.borrow_mut().insert(ty, mapped);
        mapped
    }

    pub(in crate::types) fn map_class(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> ClassType<'db> {
        match class {
            ClassType::NonGeneric(_) => class,
            ClassType::Generic(alias) => {
                ClassType::Generic(alias.map_recursive_references(db, self))
            }
        }
    }

    fn map_variable(&self, db: &'db dyn Db, variable: RecursiveVar<'db>) -> Type<'db> {
        let arguments = variable
            .arguments(db)
            .map(|arguments| arguments.map_recursive_references(db, self));
        match self.substitution {
            RecursiveSubstitution::Unfold(recursive)
                if variable.cycle(db) == recursive.cycle(db) =>
            {
                Type::Recursive(recursive.with_arguments(db, arguments))
            }
            _ => Type::RecursiveVar(RecursiveVar::new_internal(
                db,
                variable.cycle(db),
                arguments,
            )),
        }
    }

    fn map_recursive(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Type<'db> {
        let arguments = recursive
            .arguments(db)
            .map(|arguments| arguments.map_recursive_references(db, self));
        if let RecursiveSubstitution::Bind(cycle) = self.substitution
            && recursive.cycle(db) == cycle
            && recursive.materialization_kind(db).is_none()
        {
            return Type::RecursiveVar(RecursiveVar::new_internal(db, cycle, arguments));
        }
        // A nested binder shadows references in its body, but not in its arguments.
        let body = match recursive.body(db) {
            RecursiveBody::Type(body) if recursive.cycle(db) != self.substitution.cycle(db) => {
                RecursiveBody::Type(self.map_type(db, body))
            }
            body => body,
        };
        Type::Recursive(RecursiveType::new_internal(
            db,
            recursive.definition(db),
            recursive.cycle(db),
            body,
            arguments,
            recursive.materialization_kind(db),
        ))
    }

    fn map_type_inner(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match ty {
            Type::Recursive(recursive) => self.map_recursive(db, recursive),
            Type::RecursiveVar(variable) => self.map_variable(db, variable),
            Type::FunctionLiteral(function) => {
                Type::FunctionLiteral(function.map_recursive_references(db, self))
            }
            Type::Callable(callable) => Type::Callable(callable.map_recursive_references(db, self)),
            Type::BoundMethod(method) => {
                Type::BoundMethod(method.map_recursive_references(db, self))
            }
            Type::NominalInstance(instance) => {
                Type::NominalInstance(instance.map_recursive_references(db, self))
            }
            Type::NewTypeInstance(newtype) => Type::NewTypeInstance(
                newtype.map_base_class_type(db, |class| self.map_class(db, class)),
            ),
            Type::ProtocolInstance(protocol) => {
                Type::ProtocolInstance(protocol.map_recursive_references(db, self))
            }
            Type::GenericAlias(alias) => {
                Type::GenericAlias(alias.map_recursive_references(db, self))
            }
            Type::TypedDict(typed_dict) => {
                Type::TypedDict(typed_dict.map_recursive_references(db, self))
            }
            Type::SubclassOf(subclass) => subclass.map_recursive_references(db, self),
            Type::PropertyInstance(property) => {
                Type::PropertyInstance(property.map_recursive_references(db, self))
            }
            Type::SlotDescriptor(descriptor) => Type::SlotDescriptor(SlotDescriptorType::new(
                db,
                self.map_type(db, descriptor.value_type(db)),
            )),
            Type::Union(union) => UnionType::from_elements_structural(
                db,
                union.elements(db).iter().map(|ty| self.map_type(db, *ty)),
                union.recursively_defined(db),
            ),
            Type::Intersection(intersection) => Type::Intersection(IntersectionType::new(
                db,
                intersection
                    .positive(db)
                    .iter()
                    .map(|ty| self.map_type(db, *ty))
                    .collect::<FxOrderSet<_>>(),
                intersection.negative(db).map(|ty| self.map_type(db, *ty)),
            )),
            Type::EnumComplement(complement) => {
                Type::EnumComplement(complement.map_recursive_references(db, self))
            }
            Type::TypeIs(type_is) => type_is.map_recursive_references(db, self),
            Type::TypeGuard(type_guard) => {
                type_guard.with_type(db, self.map_type(db, type_guard.return_type(db)))
            }
            Type::TypeForm(type_form) => TypeFormType::from_type_expression(
                db,
                self.map_type(db, type_form.type_argument(db)),
            ),
            Type::TypeAlias(alias) => Type::TypeAlias(alias.map_recursive_references(db, self)),
            Type::KnownInstance(instance) => instance.map_recursive_references(db, self),
            Type::KnownBoundMethod(method) => Type::KnownBoundMethod(match method {
                KnownBoundMethodType::FunctionTypeDunderGet(function) => {
                    KnownBoundMethodType::FunctionTypeDunderGet(InternedType::new(
                        db,
                        self.map_type(db, function.inner(db)),
                    ))
                }
                KnownBoundMethodType::DunderCall(callable) => KnownBoundMethodType::DunderCall(
                    InternedType::new(db, self.map_type(db, callable.inner(db))),
                ),
                KnownBoundMethodType::MethodTypeDunderGet(method) => {
                    KnownBoundMethodType::MethodTypeDunderGet(
                        method.map_recursive_references(db, self),
                    )
                }
                KnownBoundMethodType::PropertyDunderGet(property) => {
                    KnownBoundMethodType::PropertyDunderGet(
                        property.map_recursive_references(db, self),
                    )
                }
                KnownBoundMethodType::PropertyDunderSet(property) => {
                    KnownBoundMethodType::PropertyDunderSet(
                        property.map_recursive_references(db, self),
                    )
                }
                KnownBoundMethodType::PropertyDunderDelete(property) => {
                    KnownBoundMethodType::PropertyDunderDelete(
                        property.map_recursive_references(db, self),
                    )
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
            | Type::SpecialForm(_) => ty,
        }
    }
}
