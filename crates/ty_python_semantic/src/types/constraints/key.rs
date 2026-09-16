//! Order-independent keys for the set operations in constraint bounds.

use std::hash::{BuildHasher, Hash, Hasher};

use rustc_hash::{FxBuildHasher, FxHashSet};

use super::variables::{Constraint, ConstraintProvenance};
use crate::Db;
use crate::types::{BoundTypeVarInstance, RecursivelyDefined, Type};

/// Identifies equivalent permutations without changing the bounds retained for inference.
#[derive(Debug, Eq, Hash, PartialEq)]
pub(super) struct ConstraintKey<'db>(ConstraintKind<'db>);

impl<'db> ConstraintKey<'db> {
    pub(super) fn new(db: &'db dyn Db, constraint: Constraint<'db>) -> Self {
        Self(match constraint {
            Constraint::ConcreteLower(bound) => ConstraintKind::Lower(
                bound.typevar,
                bound.provenance,
                TypeKey::new(db, bound.bound),
            ),
            Constraint::ConcreteUpper(bound) => ConstraintKind::Upper(
                bound.typevar,
                bound.provenance,
                TypeKey::new(db, bound.bound),
            ),
            Constraint::ConcreteEquivalence(bound) => ConstraintKind::Equivalence(
                bound.typevar,
                bound.provenance,
                TypeKey::new(db, bound.bound),
            ),
            Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => {
                ConstraintKind::TypeVars(constraint)
            }
        })
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
enum ConstraintKind<'db> {
    Lower(
        BoundTypeVarInstance<'db>,
        ConstraintProvenance,
        TypeKey<'db>,
    ),
    Upper(
        BoundTypeVarInstance<'db>,
        ConstraintProvenance,
        TypeKey<'db>,
    ),
    Equivalence(
        BoundTypeVarInstance<'db>,
        ConstraintProvenance,
        TypeKey<'db>,
    ),
    TypeVars(Constraint<'db>),
}

#[derive(Debug, Eq, Hash, PartialEq)]
enum TypeKey<'db> {
    Atomic(Type<'db>),
    Union(UnorderedTypes<'db>, RecursivelyDefined),
    Intersection(UnorderedTypes<'db>, UnorderedTypes<'db>),
}

impl<'db> TypeKey<'db> {
    fn new(db: &'db dyn Db, ty: Type<'db>) -> Self {
        match ty {
            Type::Union(union) => Self::Union(
                UnorderedTypes::new(db, union.elements(db).iter().copied()),
                union.recursively_defined(db),
            ),
            Type::Intersection(intersection) => Self::Intersection(
                UnorderedTypes::new(db, intersection.iter_positive(db)),
                UnorderedTypes::new(db, intersection.iter_negative(db)),
            ),
            _ => Self::Atomic(ty),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct UnorderedTypes<'db>(FxHashSet<TypeKey<'db>>);

impl<'db> UnorderedTypes<'db> {
    fn new(db: &'db dyn Db, types: impl Iterator<Item = Type<'db>>) -> Self {
        Self(types.map(|ty| TypeKey::new(db, ty)).collect())
    }
}

impl Hash for UnorderedTypes<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.len().hash(state);
        // Hashing must commute with permutation, just like set equality. Equality still checks
        // the elements, so collisions do not equate different bounds.
        let hash = self.0.iter().fold(0u64, |hash, ty| {
            hash.wrapping_add(FxBuildHasher.hash_one(ty))
        });
        state.write_u64(hash);
    }
}
