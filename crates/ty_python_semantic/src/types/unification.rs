//! Implements the _tallying_ algorithm from [[POPL2015][]], which finds the unification of a
//! set of subtyping constraints.
//!
//! [POPL2015]: https://doi.org/10.1145/2676726.2676991

// XXX
#![allow(dead_code)]

use rustc_hash::FxHashMap;

use crate::Db;
use crate::types::visitor::TypeVisitor;
use crate::types::{
    IntersectionBuilder, IntersectionType, Type, TypeVarInstance, UnionBuilder, UnionType,
};

/// A constraint that the type `s` must be a subtype of the type `t`. Tallying will find all
/// substitutions of any type variables in `s` and `t` that ensure that this subtyping holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Constraint<'db> {
    pub(crate) lower: Type<'db>,
    pub(crate) typevar: TypeVarInstance<'db>,
    pub(crate) upper: Type<'db>,
}

impl<'db> Constraint<'db> {
    /// Returns whether this constraint subsumes `other` — if it constrains the same typevar and
    /// has tighter bounds.
    fn subsumes(self, db: &'db dyn Db, other: Self) -> bool {
        self.typevar == other.typevar
            && other.lower.is_assignable_to(db, self.lower)
            && self.upper.is_assignable_to(db, other.upper)
    }

    /// Merges another constraint into this one. Panics if the two constraints have different
    /// typevars.
    fn merge(&mut self, db: &'db dyn Db, other: Self) {
        debug_assert!(self.typevar == other.typevar);
        self.lower = UnionType::from_elements(db, [self.lower, other.lower]);
        self.upper = IntersectionType::from_elements(db, [self.upper, other.upper]);
    }
}

/// A set of merged constraints. We guarantee that no constraint in the set subsumes another, and
/// that no two constraints in the set have the same typevar.
///
/// This is denoted _C_ in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConstraintSet<'db> {
    constraints: Vec<Constraint<'db>>,
}

impl<'db> ConstraintSet<'db> {
    /// Returns an empty constraint set
    fn empty() -> Self {
        Self {
            constraints: vec![],
        }
    }

    /// Returns a singleton constraint set.
    pub(crate) fn singleton(constraint: Constraint<'db>) -> Self {
        Self {
            constraints: vec![constraint],
        }
    }

    /// Adds a new constraint to this set, ensuring that no constraint in the set subsumes another,
    /// and that no two constraints in the set have the same typevar.
    fn add(&mut self, db: &'db dyn Db, constraint: Constraint<'db>) {
        for existing in &mut self.constraints {
            if constraint.typevar == existing.typevar {
                existing.merge(db, constraint);
                return;
            }
        }
        self.constraints.push(constraint);
    }

    /// Combines two constraint sets, merging any constraints that share the same typevar.
    fn combine(&mut self, db: &'db dyn Db, other: &Self) {
        for constraint in &other.constraints {
            self.add(db, *constraint);
        }
    }

    /// Returns whether this constraint set subsumes `other` — if every constraint in `other` is
    /// subsumed by some constraint in `self`.
    fn subsumes(&self, db: &'db dyn Db, other: &Self) -> bool {
        other.constraints.iter().all(|other_constraint| {
            self.constraints
                .iter()
                .any(|self_constraint| self_constraint.subsumes(db, *other_constraint))
        })
    }
}

/// A set of constraint sets.
///
/// This is denoted _𝒮_ in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConstraintSetSet<'db> {
    sets: Vec<ConstraintSet<'db>>,
}

impl<'db> ConstraintSetSet<'db> {
    /// Returns the set of constraint sets that is unsolvable.
    pub(crate) fn never() -> Self {
        Self { sets: vec![] }
    }

    /// Returns a singleton set of constraint sets.
    pub(crate) fn singleton(constraint_set: ConstraintSet<'db>) -> Self {
        Self {
            sets: vec![constraint_set],
        }
    }

    /// Returns the set of constraint sets that is always satisfied.
    pub(crate) fn always() -> Self {
        Self::singleton(ConstraintSet::empty())
    }

    /// Returns a set of constraint sets that is equivalent to the constraint that `ty` is (a
    /// subtype of) `Never`.
    ///
    /// This is a combination of the "Constraint normalization" and "Constraint merging" steps from
    /// [[POPL2015][]], §3.2.1.
    ///
    /// [POPL2015]: https://doi.org/10.1145/2676726.2676991
    pub(crate) fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Self {
        struct NormVisitor<'db> {
            results: FxHashMap<Type<'db>, ConstraintSetSet<'db>>,
        }

        impl<'db> NormVisitor<'db> {
            fn result(&self, ty: Type<'db>) -> &ConstraintSetSet<'db> {
                self.results
                    .get(&ty)
                    .expect("should have computed a result for this type")
            }
        }

        impl<'db> TypeVisitor<'db> for NormVisitor<'db> {
            fn visit_type(&mut self, db: &'db dyn Db, ty: Type<'db>) {
                // Figure 3, step 1, plus a memoization optimization: If we've already visited (or
                // started visiting) this type, return the result that we already calculated for it
                // (or the coinductive base case).
                if self.results.contains_key(&ty) {
                    return;
                };

                // Figure 3, step 1: Insert the coinductive base case for this type into the result
                // set. That ensures that if we encounter this type again recursively while we are
                // in the middle of calculating a result for it, we use the coinductive base case
                // for it at the point of recursion.
                self.results.insert(ty, ConstraintSetSet::always());

                match ty {
                    // These atomic types are always inhabited by at least one value, and can
                    // therefore never be a subtype of `Never`.
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
                    | Type::SpecialForm(_) => {
                        let result = ConstraintSetSet::never();
                        self.results.insert(ty, result);
                    }

                    Type::Union(union) => {
                        // Figure 3, step 6
                        // A union is a subtype of Never only if every element is.
                        self.visit_union_type(db, union);
                        let result = (union.iter(db))
                            .fold(ConstraintSetSet::always(), |sets, element| {
                                self.result(*element).intersect(db, &sets)
                            });
                        self.results.insert(ty, result);
                    }

                    Type::Intersection(intersection) => {
                        // Figure 3, step 2
                        // If an intersection contains any (positive or negative) top-level type
                        // variables, extract out and isolate the smallest one (according to our
                        // arbitrary ordering).
                        let smallest_positive =
                            find_smallest_typevar(intersection.iter_positive(db));
                        let smallest_negative =
                            find_smallest_typevar(intersection.iter_negative(db));
                        let constraint = match (smallest_positive, smallest_negative) {
                            (Some(typevar), None) => Some(Constraint {
                                lower: Type::Never,
                                typevar,
                                upper: remove_positive_typevar(db, intersection, typevar)
                                    .negate(db),
                            }),

                            (Some(typevar), Some(negative)) if typevar < negative => {
                                Some(Constraint {
                                    lower: Type::Never,
                                    typevar,
                                    upper: remove_positive_typevar(db, intersection, typevar)
                                        .negate(db),
                                })
                            }

                            (_, Some(typevar)) => Some(Constraint {
                                lower: remove_negative_typevar(db, intersection, typevar),
                                typevar,
                                upper: Type::object(db),
                            }),

                            (None, None) => None,
                        };
                        if let Some(constraint) = constraint {
                            self.results.insert(ty, constraint.into());
                            return;
                        }

                        // Figure 3, step 3
                        // If all (positive and negative) elements of the intersection are atomic
                        // types (and therefore cannot contain any typevars), fall back on an
                        // assignability check: if the intersection of the positive elements is
                        // assignable to the union of the negative elements, then the overall
                        // intersection is empty.
                        let mut all_atomic = true;
                        let mut positive_atomic = IntersectionBuilder::new(db);
                        let mut negative_atomic = UnionBuilder::new(db);
                        for positive in intersection.iter_positive(db) {
                            if !all_atomic {
                                break;
                            }
                            if !positive.is_atomic() {
                                all_atomic = false;
                                break;
                            }
                            positive_atomic = positive_atomic.add_positive(positive);
                        }
                        for negative in intersection.iter_negative(db) {
                            if !all_atomic {
                                break;
                            }
                            if !negative.is_atomic() {
                                all_atomic = false;
                                break;
                            }
                            negative_atomic = negative_atomic.add(negative);
                        }
                        if all_atomic {
                            let positive_atomic = positive_atomic.build();
                            let negative_atomic = negative_atomic.build();
                            let result = if positive_atomic.is_assignable_to(db, negative_atomic) {
                                ConstraintSetSet::always()
                            } else {
                                ConstraintSetSet::never()
                            };
                            self.results.insert(ty, result);
                            return;
                        }

                        // TODO: other intersection clauses
                    }

                    Type::TypeVar(typevar) => {
                        // Figure 3, step 2
                        // (special case where P' = {typevar}, and P = N = N' = ø)
                        let constraint = Constraint {
                            lower: Type::Never,
                            typevar,
                            upper: Type::object(db),
                        };
                        self.results.insert(ty, constraint.into());
                    }

                    _ => todo!(),
                }
            }
        }

        let mut visitor = NormVisitor {
            results: FxHashMap::default(),
        };
        visitor.visit_type(db, ty);
        visitor
            .results
            .remove(&ty)
            .expect("should have computed a result for the input type")
    }

    /// Adds a new constraint set to this set, ensuring that no constraint set in the set subsumes
    /// another.
    fn add(&mut self, db: &'db dyn Db, constraint_set: ConstraintSet<'db>) {
        for existing in &mut self.sets {
            // If there is an existing constraint set that subsumes (or is subsumed by) the new
            // one, we want to keep the _subsumed_ constraint set.
            if constraint_set.subsumes(db, existing) {
                return;
            } else if existing.subsumes(db, &constraint_set) {
                *existing = constraint_set;
                return;
            }
        }
        self.sets.push(constraint_set);
    }

    /// Intersects two sets of constraint sets.
    ///
    /// This is the ⊓ operator from [[POPL2015][]], Definition 3.5.
    ///
    /// [POPL2015]: https://doi.org/10.1145/2676726.2676991
    fn intersect(&self, db: &'db dyn Db, other: &Self) -> Self {
        let mut result = Self::never();
        for self_set in &self.sets {
            for other_set in &other.sets {
                let mut new_set = self_set.clone();
                new_set.combine(db, other_set);
                result.add(db, new_set);
            }
        }
        result
    }

    /// union two sets of constraint sets.
    ///
    /// This is the ⊔ operator from [[POPL2015][]], Definition 3.5.
    ///
    /// [POPL2015]: https://doi.org/10.1145/2676726.2676991
    fn union(&mut self, db: &'db dyn Db, other: Self) {
        for set in other.sets {
            self.add(db, set);
        }
    }
}

impl<'db> From<Constraint<'db>> for ConstraintSetSet<'db> {
    fn from(constraint: Constraint<'db>) -> ConstraintSetSet<'db> {
        ConstraintSetSet::singleton(ConstraintSet::singleton(constraint))
    }
}

impl<'db> From<ConstraintSet<'db>> for ConstraintSetSet<'db> {
    fn from(constraint_set: ConstraintSet<'db>) -> ConstraintSetSet<'db> {
        ConstraintSetSet::singleton(constraint_set)
    }
}

/// Returns the “smallest” top-level typevar in an iterator of types.
///
/// “Smallest” is with respect to the ≼ total order mentioned in [[POPL2015][]], §3.2.1. “Any will
/// do”, so we just compare Salsa IDs.
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
fn find_smallest_typevar<'db>(
    types: impl IntoIterator<Item = Type<'db>>,
) -> Option<TypeVarInstance<'db>> {
    types
        .into_iter()
        .filter_map(|ty| match ty {
            Type::TypeVar(typevar) => Some(typevar),
            _ => None,
        })
        .min()
}

/// Removes a top-level positive typevar from an intersection.
///
/// This is the `single` function from [[POPL2015][]], §3.2.1, for the `k ∈ P'` case.
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
fn remove_positive_typevar<'db>(
    db: &'db dyn Db,
    intersection: IntersectionType<'db>,
    typevar: TypeVarInstance<'db>,
) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db);
    for positive in intersection.iter_positive(db) {
        if positive != Type::TypeVar(typevar) {
            builder = builder.add_positive(positive);
        }
    }
    for negative in intersection.iter_negative(db) {
        builder = builder.add_negative(negative);
    }
    builder.build()
}

/// Removes a top-level negative typevar from an intersection.
///
/// This is the `single` function from [[POPL2015][]], §3.2.1, for the `k ∈ N'` case.
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
fn remove_negative_typevar<'db>(
    db: &'db dyn Db,
    intersection: IntersectionType<'db>,
    typevar: TypeVarInstance<'db>,
) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db);
    for positive in intersection.iter_positive(db) {
        builder = builder.add_positive(positive);
    }
    for negative in intersection.iter_negative(db) {
        if negative != Type::TypeVar(typevar) {
            builder = builder.add_negative(negative);
        }
    }
    builder.build()
}
