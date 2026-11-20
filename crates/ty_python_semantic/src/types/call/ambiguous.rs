use std::borrow::Cow;

use crate::types::cyclic::{CycleDetector, HasIdentity, TypeIdentity};
use crate::types::generics::Specialization;
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::visitor::any_over_type;
use crate::types::{
    ClassType, DynamicType, GenericAlias, KnownClass, SubclassOfInner, SubclassOfType, Type,
};
use crate::{Db, ProgramEnvironment};

/// Retains the shared structure of ambiguous overload return types.
///
/// For example, `dict[str, list[int]]` and `dict[str, list[bytes]]` become
/// `dict[str, list[Unknown]]`. Shared structure in generic classes, tuples, and `type[...]` can be
/// preserved. Differing unions, intersections, and callable signatures fall back to `Unknown`.
pub(super) fn merge_ambiguous_returns<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    types: &[Type<'db>],
) -> Type<'db> {
    AmbiguousReturnMerger {
        db,
        env,
        visitor: CycleDetector::new(Type::Dynamic(DynamicType::AmbiguousOverload)),
    }
    .merge(types)
}

struct MergeAmbiguousReturns;

#[derive(Clone, PartialEq, Eq, Hash)]
struct ReturnTypes<'db>(Box<[Type<'db>]>);

impl<'db> HasIdentity<'db> for ReturnTypes<'db> {
    type Id = Box<[TypeIdentity<'db>]>;

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(other.0.iter())
                .all(|(left, right)| left.may_share_type_identity(db, *right))
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        self.0.iter().map(|ty| ty.to_type_identity(db)).collect()
    }
}

struct AmbiguousReturnMerger<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    visitor: CycleDetector<'db, MergeAmbiguousReturns, ReturnTypes<'db>, Type<'db>, 1>,
}

impl<'db> AmbiguousReturnMerger<'_, 'db> {
    fn merge(&self, types: &[Type<'db>]) -> Type<'db> {
        // Differing leaves retain this marker so cycle recovery can recognize when overload
        // ambiguity caused a loss of precision.
        let unknown = Type::Dynamic(DynamicType::AmbiguousOverload);
        if types.contains(&unknown) {
            return unknown;
        }

        // Compare all candidates together: equivalent types can have different structures, so
        // replacing an equivalent pair with one representative before considering other
        // candidates can make the result depend on overload order.
        if let Some(equivalent) = self.equivalent_representative(types) {
            return equivalent;
        }

        self.visitor.visit(self.db, ReturnTypes(types.into()), || {
            self.merge_structure(types).unwrap_or(unknown)
        })
    }

    fn equivalent_representative(&self, types: &[Type<'db>]) -> Option<Type<'db>> {
        let (first, rest) = types.split_first()?;
        if !rest
            .iter()
            .all(|ty| first.is_equivalent_to(self.db, self.env, *ty))
        {
            return None;
        }

        // An equivalent `Any` or ordinary `Unknown` must not erase the provenance that allows
        // cycle recovery to recognize a loss of precision from overload ambiguity.
        Some(
            types
                .iter()
                .copied()
                .find(|ty| {
                    any_over_type(self.db, self.env, *ty, false, |ty| {
                        matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
                    })
                })
                .unwrap_or(*first),
        )
    }

    fn merge_column(&self, types: impl Iterator<Item = Type<'db>>) -> Type<'db> {
        self.merge(&types.collect::<Vec<_>>())
    }

    fn merge_structure(&self, types: &[Type<'db>]) -> Option<Type<'db>> {
        let db = self.db;
        let env = self.env;
        let first = *types.first()?;

        if types
            .iter()
            .any(|ty| matches!(ty, Type::TypeAlias(_) | Type::Recursive(_)))
        {
            return Some(self.merge_column(types.iter().map(|ty| match ty {
                Type::TypeAlias(alias) => alias.value_type(db),
                Type::Recursive(recursive) => recursive.unfold(db, env).into_type(),
                _ => *ty,
            })));
        }

        if let Some(tuples) = types
            .iter()
            .map(|ty| ty.exact_tuple_instance_spec(db))
            .collect::<Option<Vec<_>>>()
        {
            return self.merge_tuples(&tuples).map(Type::tuple);
        }

        if let Some(instances) = types
            .iter()
            .map(|ty| self.class_object_instance(*ty))
            .collect::<Option<Vec<_>>>()
        {
            return match self.merge(&instances) {
                Type::Dynamic(dynamic) => Some(SubclassOfType::from(
                    db,
                    env,
                    SubclassOfInner::Dynamic(dynamic),
                )),
                instance => SubclassOfType::try_from_instance(db, env, instance).ok(),
            };
        }

        let aliases = types
            .iter()
            .map(|ty| match (first, ty) {
                (Type::NominalInstance(_), Type::NominalInstance(instance)) => {
                    instance.class(db, env).into_generic_alias()
                }
                (Type::ProtocolInstance(_), Type::ProtocolInstance(protocol))
                    if protocol.materialization_kind(db).is_none() =>
                {
                    protocol.class_origin(db)?.into_generic_alias()
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        self.merge_classes(&aliases)
            .map(|class| Type::instance(db, env, class))
    }

    /// Class literals also occur here because `type[FinalClass]` normalizes to a class literal.
    fn class_object_instance(&self, ty: Type<'db>) -> Option<Type<'db>> {
        let db = self.db;
        let env = self.env;
        match ty {
            Type::SubclassOf(subclass) => Some(subclass.to_instance(db, env)),
            Type::ClassLiteral(class) => {
                Some(Type::instance(db, env, class.default_specialization(db)))
            }
            Type::GenericAlias(alias) => Some(Type::instance(db, env, ClassType::Generic(alias))),
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                Some(Type::object())
            }
            _ => None,
        }
    }

    fn merge_classes(&self, aliases: &[GenericAlias<'db>]) -> Option<ClassType<'db>> {
        let db = self.db;
        let first = *aliases.first()?;
        let context = first.specialization(db).generic_context(db);
        let specializations = aliases
            .iter()
            .map(|alias| {
                let specialization = alias.specialization(db);
                (alias.origin(db) == first.origin(db)
                    && specialization.generic_context(db) == context
                    && specialization.materialization_kind(db).is_none()
                    && specialization.tuple(db).is_none())
                .then_some(specialization)
            })
            .collect::<Option<Vec<_>>>()?;

        let types: Box<[_]> = context
            .variables(db)
            .enumerate()
            .map(|(index, typevar)| {
                let arguments: Vec<_> = specializations
                    .iter()
                    .map(|specialization| specialization.types(db)[index])
                    .collect();
                // Packs have callable or tuple encodings that cannot be replaced by an ordinary
                // dynamic type. Retain equivalent packs while merging other type arguments.
                if typevar.is_paramspec(db) || typevar.is_typevartuple(db) {
                    self.equivalent_representative(&arguments)
                } else {
                    Some(self.merge(&arguments))
                }
            })
            .collect::<Option<_>>()?;
        Some(ClassType::Generic(GenericAlias::new(
            db,
            first.origin(db),
            Specialization::new(db, context, types, None, None),
        )))
    }

    fn merge_tuples(&self, tuples: &[Cow<'db, TupleSpec<'db>>]) -> Option<TupleType<'db>> {
        let db = self.db;
        let env = self.env;
        match tuples.first()?.as_ref() {
            TupleSpec::Fixed(first) => {
                let fixed = tuples
                    .iter()
                    .map(|tuple| tuple.as_fixed_length())
                    .collect::<Option<Vec<_>>>()?;
                if fixed.iter().any(|tuple| tuple.len() != first.len()) {
                    return None;
                }
                Some(TupleType::heterogeneous(
                    db,
                    env,
                    (0..first.len()).map(|index| {
                        self.merge_column(fixed.iter().map(|tuple| tuple.elements_slice()[index]))
                    }),
                ))
            }
            TupleSpec::Variable(first) => {
                let variable = tuples
                    .iter()
                    .map(|tuple| match tuple.as_ref() {
                        TupleSpec::Variable(tuple) => Some(tuple),
                        TupleSpec::Fixed(_) => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                if variable.iter().any(|tuple| {
                    tuple.prefix_elements().len() != first.prefix_elements().len()
                        || tuple.suffix_elements().len() != first.suffix_elements().len()
                }) {
                    return None;
                }
                let segment = if variable
                    .iter()
                    .all(|tuple| tuple.variable() == first.variable())
                {
                    first.variable()
                } else {
                    let elements = variable
                        .iter()
                        .map(|tuple| tuple.variable().homogeneous_type())
                        .collect::<Option<Vec<_>>>()?;
                    VariableSegment::Homogeneous(self.merge(&elements))
                };
                Some(TupleType::mixed_with_segment(
                    db,
                    env,
                    (0..first.prefix_elements().len()).map(|index| {
                        self.merge_column(
                            variable.iter().map(|tuple| tuple.prefix_elements()[index]),
                        )
                    }),
                    segment,
                    (0..first.suffix_elements().len()).map(|index| {
                        self.merge_column(
                            variable.iter().map(|tuple| tuple.suffix_elements()[index]),
                        )
                    }),
                ))
            }
        }
    }
}
