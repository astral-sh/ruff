//! Rewrite stored recursive references without evaluating type relationships or deferred bodies.

use std::cell::RefCell;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::TypeOperations;
use crate::types::visitor::{
    TypeKind, any_over_type_including_alias_arguments, stored_type_children,
};

use super::{RecursiveBody, RecursiveSubstitution, RecursiveType, RecursiveVar};
use crate::types::{
    ClassType, InternedType, IntersectionType, KnownBoundMethodType, SlotDescriptorType, Type,
    TypeFormType, UnionType,
};
use crate::{Db, FxOrderSet};

#[derive(Clone, Copy)]
enum StoredSubstitution<'db> {
    Recursive(RecursiveSubstitution<'db>),
    Replace {
        union: Option<UnionType<'db>>,
        replacement: Type<'db>,
    },
}

/// A substitution within a recursive binder's stored syntax.
///
/// This mapper has no semantic operation owner. Its helpers can rebuild stored references,
/// but cannot expand definitions, compare types, or normalize receiver constraints.
pub(in crate::types) struct RecursiveReferenceMapper<'db> {
    substitution: StoredSubstitution<'db>,
    mapped: RefCell<FxHashMap<Type<'db>, Type<'db>>>,
}

impl<'db> RecursiveReferenceMapper<'db> {
    pub(super) fn new(substitution: RecursiveSubstitution<'db>) -> Self {
        Self {
            substitution: StoredSubstitution::Recursive(substitution),
            mapped: RefCell::default(),
        }
    }

    /// Replace occurrences in stored type structure, leaving closed recursive bodies opaque.
    fn replace(db: &'db dyn Db, ty: Type<'db>, from: Type<'db>, to: Type<'db>) -> Type<'db> {
        Self {
            substitution: StoredSubstitution::Replace {
                union: from.as_union(),
                replacement: to,
            },
            mapped: RefCell::new(FxHashMap::from_iter([(from, to)])),
        }
        .map_type(db, ty)
    }

    pub(in crate::types) fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        if let Some(mapped) = self.mapped.borrow().get(&ty) {
            return *mapped;
        }
        // A stored union flattens nested unions. In `X | callback`, the alternatives
        // belonging to `X` are siblings of the callback. Keep every sibling outside `X`.
        if let StoredSubstitution::Replace {
            union: Some(pattern),
            replacement,
        } = self.substitution
            && let Type::Union(actual) = ty
            && pattern
                .elements(db)
                .iter()
                .all(|arm| actual.elements(db).contains(arm))
        {
            let mut inserted = false;
            let elements = actual.elements(db).iter().filter_map(|&arm| {
                if pattern.elements(db).contains(&arm) {
                    let first = !inserted;
                    inserted = true;
                    first.then_some(replacement)
                } else {
                    Some(self.map_type(db, arm))
                }
            });
            let mapped =
                UnionType::from_elements_structural(db, elements, actual.recursively_defined(db));
            self.mapped.borrow_mut().insert(ty, mapped);
            return mapped;
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
            StoredSubstitution::Recursive(RecursiveSubstitution::Unfold(recursive))
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
        if let StoredSubstitution::Recursive(RecursiveSubstitution::Bind(cycle)) = self.substitution
            && recursive.cycle(db) == cycle
            && recursive.materialization_kind(db).is_none()
        {
            return Type::RecursiveVar(RecursiveVar::new_internal(db, cycle, arguments));
        }
        // A nested binder shadows references in its body, but not in its arguments.
        let body = match recursive.body(db) {
            RecursiveBody::Type(body) if matches!(self.substitution, StoredSubstitution::Recursive(substitution) if recursive.cycle(db) != substitution.cycle(db)) => {
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

impl<'db> Type<'db> {
    /// A union stored below a constructor can contain every arm of the previous approximation
    /// alongside independent alternatives.
    fn contains_stored_term(self, db: &'db dyn Db, term: Self) -> bool {
        self == term
            || matches!((self, term), (Self::Union(union), Self::Union(pattern)) if
            pattern.elements(db).iter().all(|arm| union.elements(db).contains(arm)))
    }

    /// Widen a constructor that repeatedly embeds the previous approximation of a query cycle.
    ///
    /// For example, if a lookup simplifies `int | C` to `int` before a decorator constructs
    /// `dict[str, int | C]`, each iteration can add another dictionary. Look for an earlier
    /// term `F(X)` and a later term `F(F(X))` in the same field, where `X` has the same
    /// constructor. Substituting `X` back into the later term verifies the whole repeated
    /// layer, including its other arguments. Close that layer with the query's cycle marker.
    pub(in crate::types) fn widen_recursive_growth(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        previous: Self,
        divergent: Self,
    ) -> Self {
        if self == previous || previous.is_divergent() || self.is_divergent() {
            return self;
        }

        let mut pending: smallvec::SmallVec<[_; 4]> = smallvec::smallvec![(self, previous)];
        let mut seen = FxHashSet::default();
        while let Some((current, earlier)) = pending.pop() {
            if current == earlier || !seen.insert((current, earlier)) {
                continue;
            }

            let same_origin = current.may_share_type_identity(db, earlier);
            let arms = current.as_union().map_or_else(
                || std::slice::from_ref(&current),
                |union| union.elements(db),
            );
            // The repeated context can have several branches, e.g. list[X] | dict[str, X].
            // Require an actual constructor between the two occurrences.
            let embeds_earlier = same_origin
                && arms.iter().any(|&arm| {
                    matches!(
                        arm,
                        Self::NominalInstance(_)
                            | Self::GenericAlias(_)
                            | Self::ProtocolInstance(_)
                            | Self::TypedDict(_)
                            | Self::TypeAlias(_)
                    ) && any_over_type_including_alias_arguments(db, env, arm, |ty| {
                        ty.contains_stored_term(db, earlier)
                    })
                });
            if embeds_earlier
                && any_over_type_including_alias_arguments(db, env, earlier, |inner| {
                    inner != earlier
                        && earlier.may_share_type_identity(db, inner)
                        && RecursiveReferenceMapper::replace(db, current, earlier, inner) == earlier
                })
            {
                // A term already present elsewhere in the previous approximation can have come
                // from an independent field. Only widen genuinely new constructed terms.
                if any_over_type_including_alias_arguments(db, env, previous, |ty| ty == current) {
                    continue;
                }
                let widened = RecursiveReferenceMapper::replace(db, current, earlier, divergent);
                return RecursiveReferenceMapper::replace(db, self, current, widened);
            }

            // Unions accumulate alternatives during recovery. Compare new alternatives against
            // the previous ones. A union adds no constructor layer of its own.
            if let Type::Union(union) = current {
                let earlier_arms = earlier.as_union().map_or_else(
                    || std::slice::from_ref(&earlier),
                    |union| union.elements(db),
                );
                for &arm in union.elements(db).iter().rev() {
                    if !earlier_arms.contains(&arm) {
                        pending.extend(earlier_arms.iter().rev().map(|&old| (arm, old)));
                    }
                }
                continue;
            }
            if let Type::Union(union) = earlier {
                if !union.elements(db).contains(&current) {
                    pending.extend(union.elements(db).iter().rev().map(|&arm| (current, arm)));
                }
                continue;
            }

            if !same_origin || matches!(TypeKind::from(current), TypeKind::Atomic) {
                continue;
            }
            let new_children = stored_type_children(db, env, current);
            let old_children = stored_type_children(db, env, earlier);
            if new_children.len() == old_children.len() {
                pending.extend(new_children.into_iter().zip(old_children).rev());
            }
        }
        self
    }
}
