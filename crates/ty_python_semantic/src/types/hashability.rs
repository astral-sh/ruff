use super::{
    ClassBase, ClassType, KnownClass, NominalInstanceType, Type, TypeVarBoundOrConstraints,
};
use crate::Db;

/// Whether every value represented by a type can be hashed at runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::Update)]
pub(super) enum Hashability {
    /// Every represented value is guaranteed to be hashable.
    Always,
    /// No represented value is hashable.
    Never,
    /// The type may contain both hashable and unhashable values, or the answer is unknown.
    Maybe,
}

impl Hashability {
    /// Combine the hashability of alternative types, such as union elements or type-variable
    /// constraints.
    ///
    /// An empty set of alternatives is vacuously [`Hashability::Always`], matching `Never`.
    fn from_alternatives(alternatives: impl IntoIterator<Item = Self>) -> Self {
        let mut alternatives = alternatives.into_iter();
        let Some(first) = alternatives.next() else {
            return Self::Always;
        };
        alternatives.fold(first, |combined, alternative| {
            if combined == alternative {
                combined
            } else {
                Self::Maybe
            }
        })
    }
}

impl<'db> Type<'db> {
    /// Return whether every value represented by this type is hashable at runtime.
    ///
    /// This query is conservative: it returns [`Hashability::Always`] only when hashability is
    /// guaranteed for the full value set. Extensible classes and unsupported type forms therefore
    /// return [`Hashability::Maybe`].
    pub(super) fn hashability(self, db: &'db dyn Db) -> Hashability {
        hashability(db, self)
    }
}

fn hashability<'db>(db: &'db dyn Db, ty: Type<'db>) -> Hashability {
    match ty {
        Type::Never => Hashability::Always,
        Type::FunctionLiteral(_) => Hashability::Always,
        Type::NominalInstance(instance) => instance.hashability(db, false),
        Type::LiteralValue(literal) => literal
            .fallback_instance(db)
            .as_nominal_instance()
            .map_or(Hashability::Maybe, |instance| {
                instance.hashability(db, true)
            }),
        Type::ProtocolInstance(protocol) => {
            if protocol.requires_hash(db) {
                Hashability::Always
            } else {
                Hashability::Maybe
            }
        }
        Type::TypedDict(_) => Hashability::Never,
        Type::TypeVar(typevar) => match typevar.typevar(db).bound_or_constraints(db) {
            None => Hashability::Maybe,
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.hashability(db),
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                Hashability::from_alternatives(
                    constraints.elements(db).iter().map(|ty| ty.hashability(db)),
                )
            }
        },
        Type::Union(union) => {
            Hashability::from_alternatives(union.elements(db).iter().map(|ty| ty.hashability(db)))
        }
        Type::Intersection(intersection) => {
            if intersection
                .iter_positive(db)
                .any(|ty| ty.hashability(db) == Hashability::Always)
            {
                Hashability::Always
            } else {
                Hashability::Maybe
            }
        }
        // Recursive aliases are not fully supported. Any alias that reaches this point was not
        // unpacked by the union builder, so avoid recursively traversing its value type.
        Type::TypeAlias(_) => Hashability::Maybe,
        Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).hashability(db),
        _ => Hashability::Maybe,
    }
}

impl<'db> NominalInstanceType<'db> {
    /// Classify an instance using its effective `__hash__` slot.
    ///
    /// `exact` is used for literal values, whose runtime class is fixed. A non-final nominal type
    /// is otherwise [`Hashability::Maybe`] because a subclass can disable or restore hashing.
    fn hashability(self, db: &'db dyn Db, exact: bool) -> Hashability {
        let class = self.class(db);
        if !exact && !class.is_final(db) {
            return Hashability::Maybe;
        }

        effective_class_hashability(db, class, Type::NominalInstance(self))
    }
}

/// Find the effective `__hash__` slot using Python's class-creation rules.
///
/// Defining `__eq__` without defining `__hash__` implicitly installs
/// `__hash__ = None`, and that cancellation is inherited like an explicit member.
///
/// ```python
/// from typing import final
///
/// @final
/// class Unhashable:
///     def __eq__(self, other: object, /) -> bool: ...
/// ```
fn effective_class_hashability<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
    instance: Type<'db>,
) -> Hashability {
    for base in class.iter_mro(db) {
        if matches!(base, ClassBase::Generic) {
            continue;
        }
        let Some(base) = base.into_class() else {
            return Hashability::Maybe;
        };

        let hash_member = base.own_class_member(db, None, "__hash__");
        if let Some(hash) = hash_member.ignore_possibly_undefined() {
            if !hash_member.inner.place.is_definitely_bound() {
                return Hashability::Maybe;
            }

            if hash.is_none(db) {
                return Hashability::Never;
            }

            return if KnownClass::Hashable
                .to_instance(db)
                .as_protocol_instance()
                .is_some_and(|hashable| hashable.is_satisfied_by(db, instance))
            {
                Hashability::Always
            } else {
                Hashability::Never
            };
        }

        if base
            .own_class_member(db, None, "__eq__")
            .ignore_possibly_undefined()
            .is_some()
        {
            return Hashability::Never;
        }
    }

    Hashability::Maybe
}
