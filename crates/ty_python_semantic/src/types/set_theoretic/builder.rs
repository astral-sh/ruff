//! Smart builders for union and intersection types.
//!
//! Invariants we maintain here:
//!   * No single-element union types (should just be the contained type instead.)
//!   * No single-positive-element intersection types. Single-negative-element are OK, we don't
//!     have a standalone negation type so there's no other representation for this.
//!   * The same type should never appear more than once in a union or intersection. (This should
//!     be expanded to cover subtyping -- see below -- but for now we only implement it for type
//!     identity.)
//!   * Disjunctive normal form (DNF): the tree of unions and intersections can never be deeper
//!     than a union-of-intersections. Unions cannot contain other unions (the inner union just
//!     flattens into the outer one), intersections cannot contain other intersections (also
//!     flattens), and intersections cannot contain unions (the intersection distributes over the
//!     union, inverting it into a union-of-intersections).
//!   * No type in a union can be a subtype of any other type in the union (just eliminate the
//!     subtype from the union).
//!   * No type in an intersection can be a supertype of any other type in the intersection (just
//!     eliminate the supertype from the intersection).
//!   * An intersection containing two non-overlapping types simplifies to [`Type::Never`].
//!
//! Relation-based intersection simplifications require a non-circular proof. During inference
//! cycles and structural substitution, an intersection can retain redundant or contradictory
//! elements instead. Structural substitution restores DNF without inspecting type definitions.
//! Recursive applications remain atomic during construction. A proof-owned normalizer can unfold
//! their observed expressions while preserving the constructor, arguments, and recursion guard.
//!
//! The implication of these invariants is that a [`UnionBuilder`] does not necessarily build a
//! [`Type::Union`]. For example, if only one type is added to the [`UnionBuilder`], `build()` will
//! just return that type directly. The same is true for [`IntersectionBuilder`]; for example, if a
//! union type is added to the intersection, it will distribute and [`IntersectionBuilder::build`]
//! may end up returning a [`Type::Union`] of intersections.
//!
//! ## Performance
//!
//! In practice, there are two kinds of unions found in the wild: relatively-small unions made up
//! of normal user types (classes, etc), and large unions made up of literals, which can occur via
//! large enums or from string/integer/bytes literals, which can grow due to literal arithmetic or
//! operations on literal strings/bytes. For normal unions, it's most efficient to just store the
//! member types in a vector, and do O(n^2) redundancy checks to maintain the union in simplified
//! form. But literal unions can grow to a size where this becomes a performance problem. For this
//! reason, we group literal types in `UnionBuilder`. Since every different string literal type
//! shares exactly the same possible super-types, and none of them are subtypes of each other
//! (unless exactly the same literal type), we can avoid many unnecessary redundancy checks.

use indexmap::set::MutableValues;
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::hint::cold_path;
use std::ops::ControlFlow;

use super::generic_gradual_intersections::{GenericIntersection, generic_gradual_intersection};
use super::{RecursivelyDefined, TypeNormalization};
use crate::types::enums::EnumComplement;
use crate::types::projection::{ObservationEdge, ObservedType};
use crate::types::relation::RelationContext;
use crate::types::set_theoretic::expand_intersection_typevars_and_newtypes;
use crate::types::visitor::any_over_type;
use crate::types::{
    BytesLiteralType, ClassLiteral, EnumLiteralType, IntersectionType, KnownClass,
    KnownInstanceType, LiteralValueType, LiteralValueTypeKind, NegativeIntersectionElements,
    StringLiteralType, SubclassOfType, Type, TypePair, TypeVarBoundOrConstraints, UnionType,
};
use crate::{Db, FxIndexSet, FxOrderMap, FxOrderSet, ProgramEnvironment};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

/// The explicit inputs of a normalization operation. Scalar rewrite helpers retain every
/// contributor when their result is not a structural child of a single input.
#[derive(Clone, Debug)]
struct NormalizationProof<'db> {
    context: RelationContext<'db>,
    inputs: Vec<ObservedType<'db>>,
    pair: Option<(Vec<ObservedType<'db>>, Vec<ObservedType<'db>>)>,
}

impl<'db> NormalizationProof<'db> {
    fn new(context: RelationContext<'db>, inputs: Vec<ObservedType<'db>>) -> Self {
        Self {
            context,
            inputs,
            pair: None,
        }
    }

    fn with_other_inputs(&self, other: &[ObservedType<'db>]) -> Self {
        let mut inputs = self.inputs.clone();
        inputs.extend_from_slice(other);
        Self {
            context: self.context.clone(),
            inputs,
            pair: Some((self.inputs.clone(), other.to_vec())),
        }
    }

    fn observe(&self, ty: Type<'db>) -> ObservedType<'db> {
        match self.inputs.as_slice() {
            [input] => input.unchanged_or_unresolved(ty),
            inputs => ObservedType::dependent_on(ty, inputs),
        }
    }

    fn operands(
        &self,
        source: Type<'db>,
        target: Type<'db>,
        reversed: bool,
    ) -> (ObservedType<'db>, ObservedType<'db>) {
        let Some((left, right)) = &self.pair else {
            return (self.observe(source), self.observe(target));
        };
        let (left, right) = if reversed {
            (right, left)
        } else {
            (left, right)
        };
        let observe = |ty, inputs: &[ObservedType<'db>]| match inputs {
            [input] => input.unchanged_or_unresolved(ty),
            inputs => ObservedType::dependent_on(ty, inputs),
        };
        (observe(source, left), observe(target, right))
    }
}

/// Relations used while simplifying an observed union. A proof-owned builder must not
/// replace the caller's recursive assumptions with independent cached queries.
#[derive(Clone, Copy)]
struct UnionSimplification<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    session: Option<&'a NormalizationProof<'db>>,
    reversed: bool,
}

impl<'db> UnionSimplification<'_, 'db> {
    fn reversed(self) -> Self {
        Self {
            reversed: !self.reversed,
            ..self
        }
    }
    fn redundant(self, source: Type<'db>, target: Type<'db>) -> bool {
        match self.session {
            Some(session) => {
                let (source, target) = session.operands(source, target, self.reversed);
                session
                    .context
                    .is_redundant(self.db, self.env, source, target)
            }
            None => source.is_redundant_with(self.db, self.env, target),
        }
    }

    fn subtype(self, source: Type<'db>, target: Type<'db>) -> bool {
        match self.session {
            Some(session) => {
                let (source, target) = session.operands(source, target, self.reversed);
                session
                    .context
                    .is_subtype_eager(self.db, self.env, source, target)
            }
            None => source.is_subtype_of(self.db, self.env, target),
        }
    }

    fn equivalent(self, source: Type<'db>, target: Type<'db>) -> bool {
        match self.session {
            Some(session) => {
                let (source, target) = session.operands(source, target, self.reversed);
                session
                    .context
                    .is_equivalent_eager(self.db, self.env, source, target)
            }
            None => source.is_equivalent_to(self.db, self.env, target),
        }
    }

    fn negation_subtype(
        self,
        source: Type<'db>,
        target: Type<'db>,
        cache: &mut Option<Type<'db>>,
    ) -> bool {
        if self.session.is_none() {
            return source.negation_is_subtype_of_cached(self.db, self.env, target, cache);
        }
        let negated = *cache.get_or_insert_with(|| {
            IntersectionBuilder::new(self.db, self.env)
                .normalization(TypeNormalization::Structural)
                .add_negative(source)
                .build()
        });
        self.subtype(negated, target)
    }

    fn intersection(self) -> IntersectionBuilder<'db> {
        let mut builder = IntersectionBuilder::new(self.db, self.env);
        if let Some(session) = self.session {
            // Structural insertion leaves alias observations to the relation checker; the
            // existing-session simplifier still removes positively proved redundancies.
            builder.normalization = TypeNormalization::Structural;
            builder.proof = Some(session.clone());
        }
        builder
    }

    fn union(self) -> UnionBuilder<'db> {
        let mut builder = UnionBuilder::new(self.db, self.env);
        if let Some(session) = self.session {
            builder = builder.with_proof(session.clone());
        }
        builder
    }
}

/// Extract `(core, guard)` from truthiness-guarded intersections.
///
/// e.g.
/// - `A & ~AlwaysTruthy` -> `Some((A, ~AlwaysTruthy))`
/// - `A & ~AlwaysFalsy` -> `Some((A, ~AlwaysFalsy))`
/// - `A` -> `None`
/// - `A & ~AlwaysTruthy & ~AlwaysFalsy` -> `None` (not a single-guard shape)
///
/// This only recognizes the "single truthiness guard" forms used by truthiness narrowing.
fn split_truthiness_guarded_intersection<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    session: Option<&NormalizationProof<'db>>,
) -> Option<(Type<'db>, Type<'db>)> {
    let simplification = UnionSimplification {
        db,
        env,
        session,
        reversed: false,
    };
    let Type::Intersection(intersection) = ty else {
        return None;
    };
    let falsy = Type::AlwaysTruthy.negate(db, env);
    let truthy = Type::AlwaysFalsy.negate(db, env);

    let negative = intersection.negative(db);
    let has_not_truthy = negative.contains(&Type::AlwaysTruthy);
    let has_not_falsy = negative.contains(&Type::AlwaysFalsy);
    let guard = match (has_not_truthy, has_not_falsy) {
        (true, false) => falsy,
        (false, true) => truthy,
        _ => return None,
    };

    let mut core = simplification.intersection();
    for positive in intersection.positive(db) {
        core.add_positive_in_place(*positive);
    }
    for negative in negative {
        if (guard == falsy && *negative == Type::AlwaysTruthy)
            || (guard == truthy && *negative == Type::AlwaysFalsy)
        {
            continue;
        }
        core.add_negative_in_place(*negative);
    }
    Some((core.build(), guard))
}

/// Try to merge a complementary guarded pair into an unguarded core.
///
/// e.g.
/// - `(A & ~AlwaysTruthy, A & ~AlwaysFalsy)` -> `Some(A)`
/// - `(A & ~AlwaysTruthy, B & ~AlwaysFalsy)` -> `Some(A | B)` if reconstruction is exact
/// - `(A & ~AlwaysTruthy, C)` -> `None`
///
/// Safety rule:
/// The candidate merge is accepted only if adding each original guard back reconstructs
/// exactly the original operands (`left` and `right`).
///
/// TODO: This processing is specialized for `AlwaysTruthy/AlwaysFalsy`.
/// It would be nice to generalize this in the future.
/// Discussion: <https://github.com/astral-sh/ty/issues/224>
fn merge_truthiness_guarded_pair<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    session: Option<&NormalizationProof<'db>>,
) -> Option<Type<'db>> {
    let simplification = UnionSimplification {
        db,
        env,
        session,
        reversed: false,
    };
    let (left_core, left_guard) = split_truthiness_guarded_intersection(db, env, left, session)?;
    let (right_core, right_guard) = split_truthiness_guarded_intersection(db, env, right, session)?;
    if left_guard == right_guard {
        return None;
    }

    if simplification.equivalent(left_core, right_core) {
        return Some(left_core);
    }

    let candidate = simplification
        .union()
        .add(left_core)
        .add(right_core)
        .build();
    let left_reconstructed = simplification
        .intersection()
        .add_positive(candidate)
        .add_positive(left_guard)
        .build();
    let right_reconstructed = simplification
        .intersection()
        .add_positive(candidate)
        .add_positive(right_guard)
        .build();
    if left_reconstructed == left && right_reconstructed == right {
        Some(candidate)
    } else {
        None
    }
}

/// Fold `(T & ~A) | (T & ~B)` to `T` when `A` and `B` are disjoint.
///
/// The common part can itself contain exclusions. For example,
/// `(Unknown & ~str & ~A) | (Unknown & ~str & ~B)` simplifies to `Unknown & ~str`.
/// `A` and `B` can each be unions: all exclusions unique to one side must be disjoint
/// from every exclusion unique to the other side.
fn merge_disjoint_exclusions<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    session: Option<&NormalizationProof<'db>>,
) -> Option<Type<'db>> {
    let simplification = UnionSimplification {
        db,
        env,
        session,
        reversed: false,
    };
    let (Type::Intersection(left), Type::Intersection(right)) = (left, right) else {
        return None;
    };
    let left_positive = left.positive(db);
    let left_negative = left.negative(db);
    let right_negative = right.negative(db);

    if !left_positive.set_eq(right.positive(db)) {
        return None;
    }

    let (common_negative, left_only): (SmallVec<[_; 2]>, SmallVec<[_; 2]>) = left_negative
        .iter()
        .copied()
        .partition(|ty| right_negative.contains(ty));

    // Leave trivially redundant operands to the usual union simplification, which preserves
    // their order. This only checks exact containment, not redundancy through subtyping.
    if left_only.is_empty() || common_negative.len() == right_negative.len() {
        return None;
    }

    for right_exclusion in right_negative
        .iter()
        .filter(|ty| !left_negative.contains(ty))
    {
        for left_exclusion in &left_only {
            if match session {
                Some(session) => {
                    let (left, right) = session.operands(*left_exclusion, *right_exclusion, false);
                    simplify_intersection_pair_using(
                        &left,
                        &right,
                        IntersectionPolarity::Positive,
                        |left, right| {
                            session
                                .context
                                .is_redundant(db, env, left.clone(), right.clone())
                        },
                        |left, right| {
                            session
                                .context
                                .is_subtype_eager(db, env, left.clone(), right.clone())
                        },
                        |left, right| {
                            session
                                .context
                                .is_disjoint(db, env, left.clone(), right.clone())
                        },
                    )
                }
                None => simplify_intersection_pair(
                    db,
                    env,
                    *left_exclusion,
                    *right_exclusion,
                    IntersectionPolarity::Positive,
                ),
            } != IntersectionSimplification::Disjoint
            {
                return None;
            }
        }
    }

    let mut common = simplification
        .intersection()
        .positive_elements(left_positive.iter().copied());
    for negative in common_negative {
        common.add_negative_in_place(negative);
    }
    Some(common.build())
}

/// Return `true` if union simplification should preserve this pair because one element is
/// `Hashable` and the other is a non-final nominal instance.
///
/// Hashability does not obey normal inheritance rules: subclasses of hashable classes can be
/// unhashable. Keeping the non-final type allows downstream checks to consider it independently.
fn should_preserve_hashable_union(
    db: &dyn Db,
    env: &ProgramEnvironment<'_>,
    left: Type,
    right: Type,
) -> bool {
    let is_hashable = |ty: Type| {
        ty.as_protocol_instance(db)
            .is_some_and(|protocol| protocol.is_hashable(db))
    };
    let is_non_final_nominal_instance =
        |ty| matches!(ty, Type::NominalInstance(instance) if !instance.class(db, env).is_final(db));

    (is_hashable(left) && is_non_final_nominal_instance(right))
        || (is_hashable(right) && is_non_final_nominal_instance(left))
}

/// Combine union elements that cover more of the same enum class.
///
/// Enum complements are intersections like `Color & ~Literal[Color.RED]`. When a union contains
/// such a complement plus other complements or literals from the same enum, this rewrites the
/// element list to a single complement with the shared exclusions removed.
///
/// ```python
/// from enum import Enum
///
/// class Color(Enum):
///     RED = 1
///     BLUE = 2
///
/// # (Color excluding RED) | Literal[Color.RED] simplifies to Color.
/// ```
fn normalize_enum_complement_unions<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    types: &mut Vec<Type<'db>>,
    mut observations: Option<&mut Vec<ObservedType<'db>>>,
    session: Option<&NormalizationProof<'db>>,
) -> bool {
    let simplification = UnionSimplification {
        db,
        env,
        session,
        reversed: false,
    };
    for complement_index in 0..types.len() {
        let Type::EnumComplement(complement) = types[complement_index] else {
            continue;
        };
        let enum_class = complement.enum_class(db);
        let enum_class_literal = complement.enum_class_literal(db);
        let mut shared_excluded_names: FxHashSet<_> =
            complement.excluded_names(db).iter().cloned().collect();

        let mut remove_indices = Vec::new();
        for (index, ty) in types.iter().enumerate() {
            if index == complement_index {
                continue;
            }

            if let Type::EnumComplement(other_complement) = *ty {
                if other_complement.enum_class(db) == enum_class
                    && other_complement.rest(db) == complement.rest(db)
                {
                    shared_excluded_names
                        .retain(|name| other_complement.excluded_names(db).contains(name));
                    remove_indices.push(index);
                }
                continue;
            }

            if !complement.rest(db).is_empty() {
                continue;
            }

            let Some(enum_literal) = ty.as_enum_literal() else {
                continue;
            };
            if enum_literal.enum_class(db) != enum_class {
                continue;
            }

            let Some(canonical_name) = enum_class_literal.resolve_member(db, enum_literal.name(db))
            else {
                continue;
            };
            shared_excluded_names.remove(canonical_name);
            remove_indices.push(index);
        }

        if !remove_indices.is_empty() {
            let mut builder = simplification
                .intersection()
                .add_positive(enum_class.to_non_generic_instance(db, env));
            for rest in complement.rest(db) {
                builder.add_positive_in_place(*rest);
            }
            for name in enum_class_literal
                .member_names(db)
                .filter(|name| shared_excluded_names.contains(*name))
            {
                builder.add_negative_in_place(Type::enum_literal(EnumLiteralType::new(
                    db,
                    enum_class_literal,
                    name,
                )));
            }
            types[complement_index] = builder.build();
            if let Some(observations) = &mut observations {
                let contributors: Vec<_> = std::iter::once(complement_index)
                    .chain(remove_indices.iter().copied())
                    .map(|index| observations[index].clone())
                    .collect();
                observations[complement_index] =
                    ObservedType::dependent_on(types[complement_index], &contributors);
            }

            remove_indices.sort_unstable();
            for index in remove_indices.into_iter().rev() {
                types.swap_remove(index);
                if let Some(observations) = &mut observations {
                    observations.swap_remove(index);
                }
            }
            return true;
        }
    }

    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralKind<'db> {
    Int,
    String,
    Bytes,
    Enum { enum_class: ClassLiteral<'db> },
}

impl<'db> Type<'db> {
    /// Return `true` if this type can be a supertype of some literals of `kind` and not others.
    fn splits_literals(self, db: &'db dyn Db, kind: LiteralKind) -> bool {
        match (self, kind) {
            // Note that as of 2026-01-04, `AlwaysFalsy` and `AlwaysTruthy` never split
            // enum literals, but that could change in the future. `Literal[Foo.X]` could
            // plausibly be understood by ty as a subtype of `AlwaysFalsy` in the following
            // snippet, because `Foo` is an IntEnum that does not override `__bool__` and
            // `Foo.X` has a falsy value whereas `Foo.Y` does not:
            //
            // ```py
            // class Foo(enum.IntEnum):
            //     X = 0
            //     Y = 1
            // ```
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => true,
            (Type::LiteralValue(literal), _) => match (literal.kind(), kind) {
                (LiteralValueTypeKind::String(_), LiteralKind::String) => true,
                (LiteralValueTypeKind::Bytes(_), LiteralKind::Bytes) => true,
                (LiteralValueTypeKind::Int(_), LiteralKind::Int) => true,
                (LiteralValueTypeKind::Enum(enum_literal), LiteralKind::Enum { enum_class }) => {
                    enum_literal.enum_class(db) == enum_class
                }
                _ => false,
            },
            (Type::Intersection(intersection), _) => {
                intersection
                    .positive(db)
                    .iter()
                    .any(|ty| ty.splits_literals(db, kind))
                    || intersection
                        .negative(db)
                        .iter()
                        .any(|ty| ty.splits_literals(db, kind))
            }
            (Type::Union(union), _) => union
                .elements(db)
                .iter()
                .any(|ty| ty.splits_literals(db, kind)),
            (Type::EnumComplement(complement), LiteralKind::Enum { enum_class }) => {
                complement.enum_class(db) == enum_class
            }
            _ => false,
        }
    }
}

#[derive(Debug)]
struct UnionElement<'db> {
    kind: UnionElementKind<'db>,
    inputs: Option<Vec<ObservedType<'db>>>,
}

impl<'db> UnionElement<'db> {
    fn new(kind: UnionElementKind<'db>, proof: Option<&NormalizationProof<'db>>) -> Self {
        Self {
            kind,
            inputs: proof.map(|proof| proof.inputs.clone()),
        }
    }

    fn inputs(&self) -> &[ObservedType<'db>] {
        self.inputs.as_deref().unwrap_or_default()
    }

    fn include_inputs(&mut self, inputs: &[ObservedType<'db>]) {
        if inputs.is_empty() {
            return;
        }
        self.inputs
            .get_or_insert_with(Vec::new)
            .extend_from_slice(inputs);
    }

    fn type_count(&self) -> usize {
        self.kind.type_count()
    }
}

#[derive(Debug)]
enum UnionElementKind<'db> {
    Type(Type<'db>),
    // A map from integer literals to their promotability.
    //
    // Note that an unpromotable literal takes higher precedence than the identical literal
    // in its promotable form.
    IntLiterals(FxOrderMap<i64, bool>),
    StringLiterals(FxOrderMap<StringLiteralType<'db>, bool>),
    BytesLiterals(FxOrderMap<BytesLiteralType<'db>, bool>),
    EnumLiterals {
        enum_class: ClassLiteral<'db>,
        literals: FxOrderMap<EnumLiteralType<'db>, bool>,
    },
}

impl<'db> UnionElementKind<'db> {
    fn type_count(&self) -> usize {
        match self {
            UnionElementKind::Type(_) => 1,
            UnionElementKind::IntLiterals(literals) => literals.len(),
            UnionElementKind::StringLiterals(literals) => literals.len(),
            UnionElementKind::BytesLiterals(literals) => literals.len(),
            UnionElementKind::EnumLiterals { literals, .. } => literals.len(),
        }
    }

    /// Try reducing this `UnionElementKind` given the presence in the same union of `other_type`.
    fn try_reduce(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other_type: Type<'db>,
        cycle_recovery: bool,
        session: Option<&NormalizationProof<'db>>,
    ) -> ReduceResult<'db> {
        let simplification = UnionSimplification {
            db,
            env,
            session,
            reversed: false,
        };
        if let UnionElementKind::Type(existing) = self {
            return ReduceResult::Type(*existing);
        }

        if cycle_recovery {
            cold_path();

            // A widened literal group must absorb matching literals from later iterations for
            // recovery to converge. Preserve that exact fallback reduction without relation queries.
            return match self {
                UnionElementKind::IntLiterals(_) => {
                    ReduceResult::KeepIf(!other_type.is_instance_of(db, KnownClass::Int))
                }
                UnionElementKind::StringLiterals(_) => {
                    ReduceResult::KeepIf(!other_type.is_instance_of(db, KnownClass::Str))
                }
                UnionElementKind::BytesLiterals(_) => {
                    ReduceResult::KeepIf(!other_type.is_instance_of(db, KnownClass::Bytes))
                }
                UnionElementKind::EnumLiterals { enum_class, .. } => ReduceResult::KeepIf(
                    other_type
                        .as_nominal_instance()
                        .is_none_or(|instance| instance.class_literal(db, env) != *enum_class),
                ),
                UnionElementKind::Type(_) => {
                    unreachable!("ordinary types are handled before recovery")
                }
            };
        }

        let mut other_type_negated_cache = None;

        let mut collapse = false;
        let mut ignore = false;

        // A closure called for each element in a set of literals
        // to determine whether the element should be retained in the set.
        //
        // If `ignore` or `collapse` is `true` for any element in the set,
        // we no longer need to do any expensive redundancy checks for any
        // further elements in the set:
        //
        // - if `ignore` is `true`, this indicates that `other_type` is
        //   redundant with one of the literals in this set. Given this fact,
        //   it cannot be possible for any other literals in this set to be
        //   redundant with `other_type`.
        // - if `collapse` is `true`, all literals of this kind will be
        //   removed from the union, so it's irrelevant to answer the
        //   question of which literals should remain in this set.
        //
        // We therefore only ask if `ty` is redundant with `other_type` if
        // both `ignore` and `collapse` are `false`. If either is `true`,
        // we skip the expensive redundancy check and return `true`.
        let mut should_retain_type = |ty| {
            if ignore || simplification.redundant(other_type, ty) {
                ignore = true;
                return true;
            }
            if collapse
                || simplification.negation_subtype(other_type, ty, &mut other_type_negated_cache)
            {
                collapse = true;
                return true;
            }
            !simplification.reversed().redundant(ty, other_type)
        };

        let should_keep = match self {
            UnionElementKind::IntLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::Int) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !simplification.reversed().redundant(
                        Type::from(LiteralValueType::new(*literal, *promotable)),
                        other_type,
                    )
                }
            }
            UnionElementKind::StringLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::String) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !simplification.reversed().redundant(
                        Type::from(LiteralValueType::new(*literal, *promotable)),
                        other_type,
                    )
                }
            }
            UnionElementKind::BytesLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::Bytes) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !simplification.reversed().redundant(
                        Type::from(LiteralValueType::new(*literal, *promotable)),
                        other_type,
                    )
                }
            }
            UnionElementKind::EnumLiterals {
                enum_class,
                literals,
            } => {
                let enum_class = LiteralKind::Enum {
                    enum_class: *enum_class,
                };
                if other_type.splits_literals(db, enum_class) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !simplification.reversed().redundant(
                        Type::from(LiteralValueType::new(*literal, *promotable)),
                        other_type,
                    )
                }
            }
            UnionElementKind::Type(_) => {
                unreachable!("ordinary types are handled before reduction")
            }
        };

        if ignore {
            ReduceResult::Ignore
        } else if collapse {
            ReduceResult::CollapseToObject
        } else {
            ReduceResult::KeepIf(should_keep)
        }
    }
}

enum ReduceResult<'db> {
    /// Reduction of this `UnionElementKind` is complete; keep it in the union if the nested
    /// boolean is true, eliminate it from the union if false.
    KeepIf(bool),
    /// Collapse this entire union to `object`.
    CollapseToObject,
    /// The new element is a subtype of an existing part of the `UnionElementKind`, ignore it.
    Ignore,
    /// The given `Type` can stand-in for the entire `UnionElementKind` for further union
    /// simplification checks.
    Type(Type<'db>),
}

/// If the value ​​is defined recursively, widening is performed from fewer literal elements,
/// resulting in faster convergence of the fixed-point iteration.
const MAX_RECURSIVE_UNION_LITERALS: usize = 5;
/// If the value ​​is defined non-recursively, the fixed-point iteration will converge in one go,
/// so in principle we can have as many literal elements as we want.
/// We set a large limit for union and enum literals.
/// Huge enums and string literal sets are not uncommon (especially in generated code), and it's annoying
/// if reachability analysis etc. fails when analysing these enums.
const MAX_NON_RECURSIVE_UNION_LITERALS: usize = 8192;
pub(crate) struct UnionBuilder<'db> {
    elements: Vec<UnionElement<'db>>,
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    unpack_aliases: bool,
    /// This is enabled when joining types in a `cycle_recovery` function. Because recovery cannot
    /// introduce a new cycle, relation-based union simplifications are skipped in this mode.
    cycle_recovery: bool,
    recursively_defined: RecursivelyDefined,
    normalization: TypeNormalization,
    proof: Option<NormalizationProof<'db>>,
}

/// Accumulates types into a union.
///
/// Most real-world type variables only accumulate one or two constraints. We keep those cases as
/// plain `Type`s and only allocate a `UnionBuilder` once we know the accumulation is larger.
pub(crate) enum UnionAccumulator<'db> {
    One(Type<'db>),
    Two(Type<'db>, Type<'db>),
    Deferred(UnionBuilder<'db>),
}

impl<'db> UnionAccumulator<'db> {
    pub(crate) fn new(ty: Type<'db>) -> Self {
        UnionAccumulator::One(ty)
    }

    pub(crate) fn add(&mut self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) {
        match self {
            UnionAccumulator::One(existing) => {
                *self = UnionAccumulator::Two(*existing, ty);
            }
            UnionAccumulator::Two(first, second) => {
                let mut builder = UnionBuilder::new(db, env);
                builder.add_in_place(*first);
                builder.add_in_place(*second);
                builder.add_in_place(ty);
                *self = UnionAccumulator::Deferred(builder);
            }
            UnionAccumulator::Deferred(builder) => builder.add_in_place(ty),
        }
    }

    pub(crate) fn get_or_build(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        match self {
            UnionAccumulator::One(ty) => *ty,
            UnionAccumulator::Two(first, second) => {
                let ty = UnionType::from_two_elements(db, env, *first, *second);
                *self = UnionAccumulator::One(ty);
                ty
            }
            UnionAccumulator::Deferred(_) => {
                let ty =
                    std::mem::replace(self, UnionAccumulator::new(Type::Never)).into_type(db, env);
                *self = UnionAccumulator::new(ty);
                ty
            }
        }
    }

    pub(crate) fn into_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            UnionAccumulator::One(ty) => ty,
            UnionAccumulator::Two(first, second) => {
                UnionType::from_two_elements(db, env, first, second)
            }
            UnionAccumulator::Deferred(builder) => builder.build(),
        }
    }
}

impl<'db> UnionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env: env.clone(),
            elements: vec![],
            unpack_aliases: true,
            cycle_recovery: false,
            recursively_defined: RecursivelyDefined::No,
            normalization: TypeNormalization::Semantic,
            proof: None,
        }
    }

    pub(in crate::types) fn normalization(mut self, normalization: TypeNormalization) -> Self {
        self.normalization = normalization;
        self
    }

    /// Normalize observed alternatives within their existing recursive proof.
    pub(in crate::types) fn with_observed_context(mut self, context: RelationContext<'db>) -> Self {
        self.proof = Some(NormalizationProof::new(context, Vec::new()));
        self
    }

    fn with_proof(mut self, proof: NormalizationProof<'db>) -> Self {
        self.proof = Some(proof);
        self.unpack_aliases = false;
        self
    }

    pub(crate) fn unpack_aliases(mut self, val: bool) -> Self {
        self.unpack_aliases = val;
        self
    }

    pub(crate) fn cycle_recovery(mut self, val: bool) -> Self {
        self.cycle_recovery = val;
        if self.cycle_recovery {
            self.unpack_aliases = false;
        }
        self
    }

    /// Preserve recursion from both the source union and any transformed elements already added.
    pub(crate) fn or_recursively_defined(mut self, val: RecursivelyDefined) -> Self {
        self.recursively_defined = self.recursively_defined.or(val);
        self
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Collapse the union to a single type: `object`.
    fn collapse_to_object(&mut self) {
        if let Some(proof) = &mut self.proof {
            for element in &self.elements {
                proof.inputs.extend_from_slice(element.inputs());
            }
        }
        self.elements.clear();
        self.elements.push(UnionElement::new(
            UnionElementKind::Type(Type::object()),
            self.proof.as_ref(),
        ));
    }

    fn widen_literal_types(&mut self, seen_aliases: &mut Vec<Type<'db>>) {
        let db = self.db;
        let mut replace_with = vec![];
        for elem in &self.elements {
            match &elem.kind {
                UnionElementKind::IntLiterals(_) => {
                    replace_with.push((
                        KnownClass::Int.to_instance(db, &self.env),
                        elem.inputs().to_vec(),
                    ));
                }
                UnionElementKind::StringLiterals(_) => {
                    replace_with.push((
                        KnownClass::Str.to_instance(db, &self.env),
                        elem.inputs().to_vec(),
                    ));
                }
                UnionElementKind::BytesLiterals(_) => {
                    replace_with.push((
                        KnownClass::Bytes.to_instance(db, &self.env),
                        elem.inputs().to_vec(),
                    ));
                }
                UnionElementKind::EnumLiterals { literals, .. } => {
                    let (enum_literal, _) = literals.first().unwrap();
                    replace_with.push((
                        enum_literal.enum_class_instance(db, &self.env),
                        elem.inputs().to_vec(),
                    ));
                }
                UnionElementKind::Type(_) => {}
            }
        }
        let original = self.proof.clone();
        for (ty, inputs) in replace_with {
            self.proof = original
                .as_ref()
                .map(|proof| proof.with_other_inputs(&inputs));
            self.add_in_place_impl(ty, seen_aliases);
        }
        self.proof = original;
    }

    pub(in crate::types) fn add_observed_in_place(&mut self, mut observed: ObservedType<'db>) {
        if self.unpack_aliases
            && matches!(observed.ty, Type::TypeAlias(_))
            && let Some(context) = self.proof.as_ref().map(|proof| proof.context.clone())
        {
            let expanded = context.observe(self.db, &observed, || {
                let body = observed.unfold_in_context(self.db, &self.env, &context)?;
                self.add_observed_in_place(body);
                Some(())
            });
            if expanded.is_some() {
                return;
            }
            observed = observed.unresolved();
        }
        if observed.ty.is_union() {
            for child in observed.union_children(self.db, &self.env) {
                self.add_observed_in_place(child);
            }
            return;
        }
        let ty = observed.ty;
        let previous = self
            .proof
            .as_mut()
            .map(|proof| std::mem::replace(&mut proof.inputs, vec![observed]));
        // Observed alias expansion above owns the guard. The raw insertion path must not
        // unfold a retained recursive leaf through a context-free query.
        let unpack_aliases = self.unpack_aliases;
        if self.proof.is_some() {
            self.unpack_aliases = false;
        }
        self.add_in_place(ty);
        self.unpack_aliases = unpack_aliases;
        if let (Some(proof), Some(previous)) = (&mut self.proof, previous) {
            proof.inputs = previous;
        }
    }

    /// Adds a type to this union.
    pub(crate) fn add(mut self, ty: Type<'db>) -> Self {
        self.add_in_place(ty);
        self
    }

    /// Adds a type to this union.
    pub(crate) fn add_in_place(&mut self, ty: Type<'db>) {
        if self.normalization == TypeNormalization::Structural {
            self.add_structural(ty);
            return;
        }
        ty.assert_not_recursive_var();
        self.add_in_place_impl(ty, &mut vec![]);
    }

    /// Restore DNF after substitution without unfolding aliases or querying type relations.
    fn add_structural(&mut self, ty: Type<'db>) {
        match ty {
            Type::Union(union) => {
                self.recursively_defined = self
                    .recursively_defined
                    .or(union.recursively_defined(self.db));
                for element in union.elements(self.db) {
                    self.add_structural(*element);
                }
            }
            Type::Intersection(_) => {
                let normalized = IntersectionBuilder::new(self.db, &self.env)
                    .normalization(TypeNormalization::Structural)
                    .add_positive(ty)
                    .build();
                self.add_structural_dnf(normalized);
            }
            _ => self.add_structural_dnf(ty),
        }
    }

    /// Add an already-normalized branch without normalizing its intersection a second time.
    fn add_structural_dnf(&mut self, ty: Type<'db>) {
        if let Type::LiteralValue(literal) = ty {
            self.recursively_defined = self.recursively_defined.or(literal.recursively_defined());
        }
        match ty {
            Type::Never => {}
            Type::Union(union) => {
                self.recursively_defined = self.recursively_defined.or(union.recursively_defined(self.db));
                for element in union.elements(self.db) {
                    self.add_structural_dnf(*element);
                }
            }
            _ if ty == Type::object() => self.collapse_to_object(),
            _ => {
                if !self.elements.iter().any(|element| {
                    matches!(element.kind, UnionElementKind::Type(existing) if existing == ty || existing == Type::object())
                }) {
                    self.elements.push(UnionElement::new(UnionElementKind::Type(ty), self.proof.as_ref()));
                }
            }
        }
    }

    fn add_in_place_impl(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        let db = self.db;
        let env = self.env.clone();
        let session = self.proof.clone();
        let cycle_recovery = self.cycle_recovery;
        let should_widen = |literals, recursively_defined: RecursivelyDefined| {
            if recursively_defined.is_yes() && cycle_recovery {
                literals >= MAX_RECURSIVE_UNION_LITERALS
            } else {
                literals >= MAX_NON_RECURSIVE_UNION_LITERALS
            }
        };

        let mut ty_negated_cache = None;

        match ty {
            Type::Union(union) => {
                let new_elements = union.elements(db);
                self.elements.reserve(new_elements.len());
                for element in new_elements {
                    self.add_in_place_impl(*element, seen_aliases);
                }
                self.recursively_defined =
                    self.recursively_defined.or(union.recursively_defined(db));
                if self.cycle_recovery && self.recursively_defined.is_yes() {
                    let literals = self.elements.iter().fold(0, |acc, elem| match &elem.kind {
                        UnionElementKind::IntLiterals(literals) => acc + literals.len(),
                        UnionElementKind::StringLiterals(literals) => acc + literals.len(),
                        UnionElementKind::BytesLiterals(literals) => acc + literals.len(),
                        UnionElementKind::EnumLiterals { literals, .. } => acc + literals.len(),
                        UnionElementKind::Type(_) => acc,
                    });
                    if should_widen(literals, self.recursively_defined) {
                        self.widen_literal_types(seen_aliases);
                    }
                }
            }
            // Adding `Never` to a union is a no-op.
            Type::Never => {}
            Type::TypeAlias(_) if self.unpack_aliases => {
                if seen_aliases.contains(&ty) {
                    // Union contains itself recursively via a type alias. This is an error, just
                    // leave out the recursive alias. TODO surface this error.
                } else {
                    seen_aliases.push(ty);
                    self.add_in_place_impl(ty.resolve_type_alias(db), seen_aliases);
                }
            }
            Type::LiteralValue(literal) => {
                self.recursively_defined =
                    self.recursively_defined.or(literal.recursively_defined());
                match literal.kind() {
                    // If adding a string literal, look for an existing `UnionElementKind::StringLiterals` to
                    // add it to, or an existing element that is a super-type of string literals, which
                    // means we shouldn't add it. Otherwise, add a new `UnionElementKind::StringLiterals`
                    // containing it.
                    LiteralValueTypeKind::String(string_literal) => {
                        let mut found = None;
                        let mut found_index = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            let pair = session.as_ref().filter(|_| !matches!(
                                &element.kind, UnionElementKind::StringLiterals(literals)
                                    if !should_widen(literals.len(), self.recursively_defined)
                            )).map(|proof| proof.with_other_inputs(element.inputs()));
                            let simplification = UnionSimplification {
                                db,
                                env: &env,
                                session: pair.as_ref(),
                                reversed: false,
                            };
                            let input = session
                                .as_ref()
                                .map_or(&[][..], |proof| proof.inputs.as_slice());
                            if matches!(&element.kind, UnionElementKind::StringLiterals(_)) {
                                element.include_inputs(input);
                            }
                            match &mut element.kind {
                                UnionElementKind::StringLiterals(literals) => {
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let replace_with =
                                            KnownClass::Str.to_instance(db, &self.env);
                                        self.proof = pair;
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found_index = Some(index);
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElementKind::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(db, &self.env)
                                            == *existing =>
                                {
                                    return;
                                }
                                UnionElementKind::Type(existing) if !cycle_recovery => {
                                    // e.g. `existing` could be `Literal[""] & Any`,
                                    // and `ty` could be `Literal[""]`
                                    if simplification.redundant(ty, *existing) {
                                        element.include_inputs(input);
                                        return;
                                    }
                                    if simplification.reversed().redundant(*existing, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if simplification.negation_subtype(
                                        ty,
                                        *existing,
                                        &mut ty_negated_cache,
                                    ) {
                                        // The type that includes both this new element, and its negation
                                        // (or a supertype of its negation), must be simply `object`.
                                        self.collapse_to_object();
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if let Some(found) = found {
                            let is_promotable = literal.is_promotable();
                            *found.entry(string_literal).or_insert(is_promotable) &= is_promotable;
                        } else {
                            self.elements.push(UnionElement::new(
                                UnionElementKind::StringLiterals(FxOrderMap::from_iter([(
                                    string_literal,
                                    literal.is_promotable(),
                                )])),
                                self.proof.as_ref(),
                            ));
                        }
                        if let Some(index) = to_remove {
                            let inputs = self.elements[index].inputs().to_vec();
                            let output = found_index.unwrap_or(self.elements.len() - 1);
                            self.elements[output].include_inputs(&inputs);
                            self.elements.swap_remove(index);
                        }
                    }
                    // Same for bytes literals as for string literals, above.
                    LiteralValueTypeKind::Bytes(bytes_literal) => {
                        let mut found = None;
                        let mut found_index = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            let pair = session.as_ref().filter(|_| !matches!(
                                &element.kind, UnionElementKind::BytesLiterals(literals)
                                    if !should_widen(literals.len(), self.recursively_defined)
                            )).map(|proof| proof.with_other_inputs(element.inputs()));
                            let simplification = UnionSimplification {
                                db,
                                env: &env,
                                session: pair.as_ref(),
                                reversed: false,
                            };
                            let input = session
                                .as_ref()
                                .map_or(&[][..], |proof| proof.inputs.as_slice());
                            if matches!(&element.kind, UnionElementKind::BytesLiterals(_)) {
                                element.include_inputs(input);
                            }
                            match &mut element.kind {
                                UnionElementKind::BytesLiterals(literals) => {
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let replace_with =
                                            KnownClass::Bytes.to_instance(db, &self.env);
                                        self.proof = pair;
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found_index = Some(index);
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElementKind::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(db, &self.env)
                                            == *existing =>
                                {
                                    return;
                                }
                                UnionElementKind::Type(existing) if !cycle_recovery => {
                                    if simplification.redundant(ty, *existing) {
                                        element.include_inputs(input);
                                        return;
                                    }
                                    // e.g. `existing` could be `Literal[b""] & Any`,
                                    // and `ty` could be `Literal[b""]`
                                    if simplification.reversed().redundant(*existing, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if simplification.negation_subtype(
                                        ty,
                                        *existing,
                                        &mut ty_negated_cache,
                                    ) {
                                        // The type that includes both this new element, and its negation
                                        // (or a supertype of its negation), must be simply `object`.
                                        self.collapse_to_object();
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if let Some(found) = found {
                            let is_promotable = literal.is_promotable();
                            *found.entry(bytes_literal).or_insert(is_promotable) &= is_promotable;
                        } else {
                            self.elements.push(UnionElement::new(
                                UnionElementKind::BytesLiterals(FxOrderMap::from_iter([(
                                    bytes_literal,
                                    literal.is_promotable(),
                                )])),
                                self.proof.as_ref(),
                            ));
                        }
                        if let Some(index) = to_remove {
                            let inputs = self.elements[index].inputs().to_vec();
                            let output = found_index.unwrap_or(self.elements.len() - 1);
                            self.elements[output].include_inputs(&inputs);
                            self.elements.swap_remove(index);
                        }
                    }
                    // And same for int literals as well.
                    LiteralValueTypeKind::Int(int_literal) => {
                        let mut found = None;
                        let mut found_index = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            let pair = session.as_ref().filter(|_| !matches!(
                                &element.kind, UnionElementKind::IntLiterals(literals)
                                    if !should_widen(literals.len(), self.recursively_defined)
                            )).map(|proof| proof.with_other_inputs(element.inputs()));
                            let simplification = UnionSimplification {
                                db,
                                env: &env,
                                session: pair.as_ref(),
                                reversed: false,
                            };
                            let input = session
                                .as_ref()
                                .map_or(&[][..], |proof| proof.inputs.as_slice());
                            if matches!(&element.kind, UnionElementKind::IntLiterals(_)) {
                                element.include_inputs(input);
                            }
                            match &mut element.kind {
                                UnionElementKind::IntLiterals(literals) => {
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let replace_with =
                                            KnownClass::Int.to_instance(db, &self.env);
                                        self.proof = pair;
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found_index = Some(index);
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElementKind::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(db, &self.env)
                                            == *existing =>
                                {
                                    return;
                                }
                                UnionElementKind::Type(existing) if !cycle_recovery => {
                                    if simplification.redundant(ty, *existing) {
                                        element.include_inputs(input);
                                        return;
                                    }
                                    // e.g. `existing` could be `Literal[1] & Any`,
                                    // and `ty` could be `Literal[1]`
                                    if simplification.reversed().redundant(*existing, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if simplification.negation_subtype(
                                        ty,
                                        *existing,
                                        &mut ty_negated_cache,
                                    ) {
                                        // The type that includes both this new element, and its negation
                                        // (or a supertype of its negation), must be simply `object`.
                                        self.collapse_to_object();
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if let Some(found) = found {
                            let is_promotable = literal.is_promotable();
                            *found.entry(int_literal.as_i64()).or_insert(is_promotable) &=
                                is_promotable;
                        } else {
                            self.elements.push(UnionElement::new(
                                UnionElementKind::IntLiterals(FxOrderMap::from_iter([(
                                    int_literal.as_i64(),
                                    literal.is_promotable(),
                                )])),
                                self.proof.as_ref(),
                            ));
                        }
                        if let Some(index) = to_remove {
                            let inputs = self.elements[index].inputs().to_vec();
                            let output = found_index.unwrap_or(self.elements.len() - 1);
                            self.elements[output].include_inputs(&inputs);
                            self.elements.swap_remove(index);
                        }
                    }
                    LiteralValueTypeKind::Enum(enum_member_to_add) => {
                        let enum_class_literal = enum_member_to_add.enum_class_literal(db);
                        let enum_class = enum_class_literal.class_literal(db);
                        let enum_member_count = enum_class_literal.member_count(db);
                        let members_are_exhaustive = enum_class_literal.members_are_exhaustive(db);

                        if members_are_exhaustive && enum_member_count == 1 {
                            self.add_in_place_impl(
                                enum_member_to_add.enum_class_instance(db, &self.env),
                                seen_aliases,
                            );
                            return;
                        }

                        let mut found = None;
                        let mut found_index = None;
                        let mut found_inputs = Vec::new();
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            let pair = session.as_ref().filter(|_| !matches!(
                                &element.kind, UnionElementKind::EnumLiterals { enum_class: existing, literals }
                                    if *existing == enum_class && !should_widen(literals.len(), self.recursively_defined)
                            )).map(|proof| proof.with_other_inputs(element.inputs()));
                            let simplification = UnionSimplification {
                                db,
                                env: &env,
                                session: pair.as_ref(),
                                reversed: false,
                            };
                            let input = session
                                .as_ref()
                                .map_or(&[][..], |proof| proof.inputs.as_slice());
                            if matches!(&element.kind, UnionElementKind::EnumLiterals { enum_class: existing, .. } if *existing == enum_class)
                            {
                                element.include_inputs(input);
                            }
                            let completes_enum = members_are_exhaustive
                                && matches!(
                                    &element.kind, UnionElementKind::EnumLiterals { enum_class: existing, literals }
                                        if *existing == enum_class
                                            && literals.len() + usize::from(!literals.contains_key(&enum_member_to_add)) == enum_member_count
                                );
                            let element_inputs = if completes_enum {
                                element.inputs().to_vec()
                            } else {
                                Vec::new()
                            };
                            match &mut element.kind {
                                UnionElementKind::EnumLiterals {
                                    enum_class: existing_enum_class,
                                    literals,
                                } => {
                                    if *existing_enum_class != enum_class {
                                        continue;
                                    }
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let (literal, _) = literals.first().unwrap();
                                        let replace_with =
                                            literal.enum_class_instance(db, &self.env);
                                        self.proof = pair;
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found_index = Some(index);
                                    found_inputs = element_inputs;
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElementKind::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(db, &self.env)
                                            == *existing =>
                                {
                                    return;
                                }
                                UnionElementKind::Type(existing) if !cycle_recovery => {
                                    if simplification.redundant(ty, *existing) {
                                        element.include_inputs(input);
                                        return;
                                    }
                                    // e.g. `existing` could be `Literal[Foo.X] & Any`,
                                    // and `ty` could be `Literal[Foo.X]`
                                    if simplification.reversed().redundant(*existing, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if simplification.negation_subtype(
                                        ty,
                                        *existing,
                                        &mut ty_negated_cache,
                                    ) {
                                        // The type that includes both this new element, and its negation
                                        // (or a supertype of its negation), must be simply `object`.
                                        self.collapse_to_object();
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if let Some(found) = found {
                            match found.entry(enum_member_to_add) {
                                ordermap::map::Entry::Vacant(entry) => {
                                    entry.insert(literal.is_promotable());

                                    if members_are_exhaustive && found.len() == enum_member_count {
                                        if let Some(proof) = &mut self.proof {
                                            proof.inputs = found_inputs;
                                        }
                                        self.add_in_place_impl(
                                            enum_member_to_add.enum_class_instance(db, &self.env),
                                            seen_aliases,
                                        );
                                        return;
                                    }
                                }
                                ordermap::map::Entry::Occupied(mut entry) => {
                                    *entry.get_mut() &= literal.is_promotable();
                                }
                            }
                        } else {
                            self.elements.push(UnionElement::new(
                                UnionElementKind::EnumLiterals {
                                    enum_class,
                                    literals: FxOrderMap::from_iter([(
                                        enum_member_to_add,
                                        literal.is_promotable(),
                                    )]),
                                },
                                self.proof.as_ref(),
                            ));
                        }
                        if let Some(index) = to_remove {
                            let inputs = self.elements[index].inputs().to_vec();
                            let output = found_index.unwrap_or(self.elements.len() - 1);
                            self.elements[output].include_inputs(&inputs);
                            self.elements.swap_remove(index);
                        }
                    }
                    _ => self.push_type(ty, seen_aliases),
                }
            }
            // Adding `object` to a union results in `object`.
            ty if ty.is_object() && !cycle_recovery => self.collapse_to_object(),
            _ => self.push_type(ty, seen_aliases),
        }
    }

    fn push_type(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        let db = self.db;
        let env = self.env.clone();
        let mut session = self.proof.clone();
        let mut ty = ty;
        let bool_pair = |ty: Type<'db>| {
            if let Some(LiteralValueTypeKind::Bool(b)) = ty.as_literal_value_kind() {
                Some(LiteralValueTypeKind::Bool(!b))
            } else {
                None
            }
        };

        // If an alias gets here, it means we aren't unpacking aliases, and we also
        // shouldn't try to simplify aliases out of the union, because that will require
        // unpacking them.
        let should_simplify_full = !ty.is_alias_like(db) && !self.cycle_recovery;

        let mut ty_negated: Option<Type> = None;
        let mut to_remove = SmallVec::<[usize; 2]>::new();

        for (i, element) in self.elements.iter_mut().enumerate() {
            let pair = session
                .as_ref()
                .map(|proof| proof.with_other_inputs(element.inputs()));
            let simplification = UnionSimplification {
                db,
                env: &env,
                session: pair.as_ref(),
                reversed: false,
            };
            let element_type =
                match element
                    .kind
                    .try_reduce(db, &self.env, ty, self.cycle_recovery, pair.as_ref())
                {
                    ReduceResult::KeepIf(keep) => {
                        element.include_inputs(session.as_ref().map_or(&[], |proof| &proof.inputs));
                        if !keep {
                            to_remove.push(i);
                        }
                        continue;
                    }
                    ReduceResult::Type(ty) => ty,
                    ReduceResult::CollapseToObject => {
                        self.collapse_to_object();
                        return;
                    }
                    ReduceResult::Ignore => {
                        element.include_inputs(session.as_ref().map_or(&[], |proof| &proof.inputs));
                        return;
                    }
                };

            if ty == element_type {
                element.include_inputs(session.as_ref().map_or(&[], |proof| &proof.inputs));
                return;
            }

            // `object` already contains every possible union element.
            if !self.cycle_recovery && element_type == Type::object() {
                element.include_inputs(session.as_ref().map_or(&[], |proof| &proof.inputs));
                return;
            }

            if !self.cycle_recovery
                && should_preserve_hashable_union(db, &self.env, ty, element_type)
            {
                continue;
            }

            // The empty and non-empty range refinements are disjoint, but together they cover
            // the ordinary `range` instance type.
            if let (
                Type::KnownInstance(KnownInstanceType::Range { is_non_empty: left }),
                Type::KnownInstance(KnownInstanceType::Range {
                    is_non_empty: right,
                }),
            ) = (ty, element_type)
                && left != right
            {
                to_remove.push(i);
                ty = KnownClass::Range.to_instance(db, &self.env);
                session = pair;
                continue;
            }

            // Fold `(T & ~AlwaysTruthy) | (T & ~AlwaysFalsy)` to `T`.
            if !self.cycle_recovery
                && let Some(merged_type) =
                    merge_truthiness_guarded_pair(db, &self.env, ty, element_type, pair.as_ref())
            {
                to_remove.push(i);
                ty = merged_type;
                session = pair;
                continue;
            }

            if !self.cycle_recovery
                && element_type
                    .as_literal_value_kind()
                    .zip(bool_pair(ty))
                    .is_some_and(|(element, pair)| element == pair)
            {
                self.proof = pair;
                self.add_in_place_impl(KnownClass::Bool.to_instance(db, &self.env), seen_aliases);
                return;
            }

            // Comparing `TypedDict`s for redundancy requires iterating over their fields, which is
            // problematic if some of those fields point to recursive `Union`s. To avoid cycles,
            // compare `TypedDict`s by name/identity instead of using the `has_relation_to`
            // machinery.
            if element_type.is_typed_dict() && ty.is_typed_dict() {
                continue;
            }

            if should_simplify_full && !element_type.is_alias_like(db) {
                // Preserving aliases also excludes comparisons that expand aliases nested in
                // type arguments. A recursive alias can rebuild this union during specialization.
                if self.proof.is_none()
                    && !self.unpack_aliases
                    && [ty, element_type].into_iter().any(|ty| {
                        any_over_type(db, &self.env, ty, false, |ty| ty.is_alias_like(db))
                    })
                {
                    continue;
                }
                if let Some(merged) =
                    merge_disjoint_exclusions(db, &self.env, ty, element_type, pair.as_ref())
                {
                    to_remove.push(i);
                    let mut pair = pair;
                    if let Some(proof) = &mut pair {
                        for &index in &to_remove {
                            proof
                                .inputs
                                .extend_from_slice(self.elements[index].inputs());
                        }
                        proof.pair = None;
                    }
                    for index in to_remove.into_iter().rev() {
                        self.elements.swap_remove(index);
                    }
                    // The common part can also subsume elements we already visited.
                    self.proof = pair;
                    self.add_in_place_impl(merged, seen_aliases);
                    return;
                }
                if simplification.redundant(ty, element_type) {
                    element.include_inputs(session.as_ref().map_or(&[], |proof| &proof.inputs));
                    return;
                }

                if simplification.reversed().redundant(element_type, ty) {
                    to_remove.push(i);
                    continue;
                }

                if simplification.negation_subtype(ty, element_type, &mut ty_negated) {
                    // We add `ty` to the union. We just checked that `~ty` is a subtype of an
                    // existing `element`. This also means that `~ty | ty` is a subtype of
                    // `element | ty`, because both elements in the first union are subtypes of
                    // the corresponding elements in the second union. But `~ty | ty` is just
                    // `object`. Since `object` is a subtype of `element | ty`, we can only
                    // conclude that `element | ty` must be `object` (object has no other
                    // supertypes). This means we can simplify the whole union to just
                    // `object`, since all other potential elements would also be subtypes of
                    // `object`.
                    self.collapse_to_object();
                    return;
                }
            }
        }

        self.proof = session;
        if let Some(proof) = &mut self.proof {
            for &index in &to_remove {
                proof
                    .inputs
                    .extend_from_slice(self.elements[index].inputs());
            }
        }
        let mut to_remove = to_remove.into_iter();
        if let Some(first) = to_remove.next() {
            self.elements[first] =
                UnionElement::new(UnionElementKind::Type(ty), self.proof.as_ref());
            // We iterate in descending order to keep remaining indices valid after `swap_remove`.
            for index in to_remove.rev() {
                self.elements.swap_remove(index);
            }
        } else {
            self.elements.push(UnionElement::new(
                UnionElementKind::Type(ty),
                self.proof.as_ref(),
            ));
        }
    }

    pub(crate) fn build(self) -> Type<'db> {
        self.try_build().unwrap_or(Type::Never)
    }

    pub(crate) fn try_build(self) -> Option<Type<'db>> {
        self.try_build_with_observations().map(|(ty, _)| ty)
    }

    pub(in crate::types) fn build_observed(self) -> ObservedType<'db> {
        match self.try_build_with_observations() {
            Some((ty, Some(children))) => {
                ObservedType::dependent_on(ty, &children).normalized(ty, children)
            }
            Some((ty, None)) => ObservedType::dependent_on(ty, &[]),
            None => ObservedType::dependent_on(Type::Never, &[]),
        }
    }

    fn try_build_with_observations(
        mut self,
    ) -> Option<(Type<'db>, Option<Vec<ObservedType<'db>>>)> {
        let db = self.db;
        if let Some(proof) = &mut self.proof {
            proof.inputs = self
                .elements
                .iter()
                .flat_map(|element| element.inputs().iter().cloned())
                .collect();
        }
        let mut observations = self.proof.as_ref().map(|_| Vec::new());

        let unpack_aliases = self.unpack_aliases;
        let cycle_recovery = self.cycle_recovery;
        let recursively_defined = self.recursively_defined;

        let type_count = self.elements.iter().map(UnionElement::type_count).sum();
        let mut types = Vec::with_capacity(type_count);
        for element in self.elements {
            let inputs = element.inputs().to_vec();
            let start = types.len();
            match element.kind {
                UnionElementKind::IntLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElementKind::StringLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElementKind::BytesLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElementKind::EnumLiterals { literals, .. } => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElementKind::Type(Type::LiteralValue(literal))
                    if self.normalization == TypeNormalization::Structural =>
                {
                    // Flattening a recursive union must retain the literal's provenance even
                    // when only one element remains. Updating that flag can expose duplicates.
                    let literal =
                        Type::LiteralValue(literal.with_recursively_defined(recursively_defined));
                    if !types.contains(&literal) {
                        types.push(literal);
                    }
                }
                UnionElementKind::Type(ty) => types.push(ty),
            }
            if let Some(observations) = &mut observations {
                observations.extend(types[start..].iter().map(|&ty| match inputs.as_slice() {
                    [input] => input.unchanged_or_unresolved(ty),
                    _ => ObservedType::dependent_on(ty, &inputs),
                }));
            }
        }

        if self.normalization == TypeNormalization::Semantic
            && normalize_enum_complement_unions(
                db,
                &self.env,
                &mut types,
                observations.as_mut(),
                self.proof.as_ref(),
            )
        {
            let mut builder = UnionBuilder::new(db, &self.env)
                .unpack_aliases(unpack_aliases)
                .cycle_recovery(cycle_recovery)
                .or_recursively_defined(recursively_defined);
            if let Some(session) = self.proof {
                builder = builder.with_proof(session);
            }
            if let Some(observations) = observations {
                for observed in observations {
                    builder.add_observed_in_place(observed);
                }
            } else {
                for ty in types {
                    builder.add_in_place(ty);
                }
            }
            return builder.try_build_with_observations();
        }

        let ty = match types.len() {
            0 => return None,
            1 => types[0],
            _ => Type::Union(UnionType::new(
                db,
                types.into_boxed_slice(),
                recursively_defined,
            )),
        };
        Some((ty, observations))
    }
}

/// Controls expansion without making ordinary intersection construction fallible.
trait IntersectionLimits {
    type Break;
    const BOUNDED: bool;

    fn check_terms(terms: usize) -> ControlFlow<Self::Break>;
}

struct UnboundedIntersection;

impl IntersectionLimits for UnboundedIntersection {
    type Break = Infallible;
    const BOUNDED: bool = false;

    fn check_terms(_terms: usize) -> ControlFlow<Self::Break> {
        ControlFlow::Continue(())
    }
}

struct BoundedIntersection;

impl IntersectionLimits for BoundedIntersection {
    type Break = ();
    const BOUNDED: bool = true;

    fn check_terms(terms: usize) -> ControlFlow<Self::Break> {
        const MAX_INTERSECTION_DNF_TERMS: usize = 4;
        if terms > MAX_INTERSECTION_DNF_TERMS {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

#[derive(Clone)]
pub(crate) struct IntersectionBuilder<'db> {
    // Really this builds a union-of-intersections, because we always keep our set-theoretic types
    // in disjunctive normal form (DNF), a union of intersections. In the simplest case there's
    // just a single intersection in this vector, and we are building a single intersection type,
    // but if a union is added to the intersection, we'll distribute ourselves over that union and
    // create a union of intersections.
    intersections: Vec<InnerIntersectionBuilder<'db>>,
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    // One disjunction does not multiply alternatives. Only subsequent distributions consume
    // the bounded constructor's budget, after impossible and redundant branches are removed.
    has_disjunction: bool,
    normalization: TypeNormalization,
    /// Solver normalization reuses its proof instead of starting cached, independent queries.
    proof: Option<NormalizationProof<'db>>,
}

impl<'db> IntersectionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env: env.clone(),
            intersections: vec![InnerIntersectionBuilder::default()],
            has_disjunction: false,
            normalization: TypeNormalization::Semantic,
            proof: None,
        }
    }

    pub(in crate::types) fn normalization(mut self, normalization: TypeNormalization) -> Self {
        self.normalization = normalization;
        self
    }

    /// Build signed observed expressions using the caller's proof rules.
    pub(in crate::types) fn with_observed_context(mut self, context: RelationContext<'db>) -> Self {
        self.normalization = TypeNormalization::Structural;
        self.proof = Some(NormalizationProof::new(context, Vec::new()));
        self
    }

    pub(in crate::types) fn add_positive_observed_in_place(
        &mut self,
        observed: &ObservedType<'db>,
    ) {
        if let Some(proof) = &mut self.proof {
            proof.inputs.push(observed.clone());
        }
        let ControlFlow::Continue(()) = self.add_positive_impl::<UnboundedIntersection>(
            observed.ty,
            &mut vec![],
            Some(observed),
        );
    }

    pub(in crate::types) fn add_negative_observed_in_place(
        &mut self,
        observed: &ObservedType<'db>,
    ) {
        if let Some(proof) = &mut self.proof {
            proof.inputs.push(observed.clone());
        }
        let ControlFlow::Continue(()) = self.add_negative_impl::<UnboundedIntersection>(
            observed.ty,
            &mut vec![],
            Some(observed),
        );
    }

    /// Add DNF branches, dropping `Never` and duplicate branches so later distribution does not
    /// multiply dead or repeated branches.
    fn extend_distributed<L: IntersectionLimits>(
        &self,
        distributed: &mut FxIndexSet<InnerIntersectionBuilder<'db>>,
        other: Self,
        check_budget: bool,
    ) -> ControlFlow<L::Break> {
        // Retain the whole first disjunction: a later factor can eliminate all but a few of its
        // alternatives, including alternatives that occur beyond the budget's position.
        if !L::BOUNDED || !check_budget {
            for candidate in other
                .intersections
                .into_iter()
                .filter(|intersection| !intersection.contains_never())
            {
                if let Some(index) = distributed.get_index_of(&candidate)
                    && let Some(existing) = distributed.get_index_mut2(index)
                {
                    // Only observations change here; Eq and Hash deliberately ignore them.
                    existing.merge_contributors(&candidate);
                } else {
                    distributed.insert(candidate);
                }
            }
            return ControlFlow::Continue(());
        }

        let db = self.db;
        let env = &self.env;
        let built_type = |inner: &InnerIntersectionBuilder<'db>| {
            if self.normalization == TypeNormalization::Structural || self.proof.is_some() {
                inner.clone().build_structural(db)
            } else {
                inner.clone().build(db, env)
            }
        };
        let redundant = |left: &InnerIntersectionBuilder<'db>,
                         right: &InnerIntersectionBuilder<'db>| {
            let left_type = built_type(left);
            let right_type = built_type(right);
            if let Some(proof) = &self.proof {
                proof.context.is_redundant(
                    db,
                    env,
                    left.observe(left_type),
                    right.observe(right_type),
                )
            } else if self.normalization == TypeNormalization::Structural {
                left_type == right_type
            } else {
                left_type.is_redundant_with(db, env, right_type)
            }
        };
        for mut candidate in other.intersections {
            // Some branches only collapse during `build`, for example when a constrained
            // type variable has no remaining constraints. Those do not consume the budget.
            let candidate_type = built_type(&candidate);
            if candidate_type.is_never()
                || self.proof.as_ref().is_some_and(|session| {
                    session.context.is_disjoint(
                        db,
                        env,
                        candidate.observe(candidate_type),
                        candidate.observe(candidate_type),
                    )
                })
            {
                continue;
            }
            if let Some(index) = distributed
                .iter()
                .position(|old| redundant(&candidate, old))
            {
                if let Some(old) = distributed.get_index_mut2(index) {
                    old.merge_contributors(&candidate);
                }
                continue;
            }
            distributed.retain(|old| {
                if redundant(old, &candidate) {
                    candidate.merge_contributors(old);
                    false
                } else {
                    true
                }
            });
            L::check_terms(distributed.len() + 1)?;
            if let Some(index) = distributed.get_index_of(&candidate)
                && let Some(existing) = distributed.get_index_mut2(index)
            {
                existing.merge_contributors(&candidate);
            } else {
                distributed.insert(candidate);
            }
        }
        ControlFlow::Continue(())
    }

    pub(in crate::types) fn bounded_from_elements<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
        normalization: TypeNormalization,
    ) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Clone,
        Type<'db>: From<T>,
    {
        let elements = elements.into_iter().map(Type::from);
        let mut first_elements = elements.clone();
        let Some(first) = first_elements.next() else {
            return Some(Type::object());
        };
        if first_elements.next().is_none() {
            return Some(first);
        }
        Self::bounded_from_type_elements(db, env, elements, normalization, None, None)
            .map(Self::build)
    }

    pub(in crate::types) fn bounded_from_observed_elements<I>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
        normalization: TypeNormalization,
        context: Option<RelationContext<'db>>,
    ) -> Option<ObservedType<'db>>
    where
        I: IntoIterator<Item = ObservedType<'db>>,
    {
        let elements: Vec<_> = elements.into_iter().collect();
        if let [element] = elements.as_slice() {
            return Some(element.clone());
        }
        if normalization == TypeNormalization::Semantic && context.is_none() {
            return None;
        }
        let proof = context.map(|context| NormalizationProof::new(context, elements.clone()));
        // Alias expansion and semantic reductions use observed operands. Raw structural
        // insertion never opens an independent query while this proof is active.
        let builder = Self::bounded_from_type_elements(
            db,
            env,
            elements.iter().map(|element| element.ty),
            TypeNormalization::Structural,
            proof,
            Some(&elements),
        )?;
        Some(builder.build_observed())
    }

    fn bounded_from_type_elements<I>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
        normalization: TypeNormalization,
        proof: Option<NormalizationProof<'db>>,
        observations: Option<&[ObservedType<'db>]>,
    ) -> Option<Self>
    where
        I: IntoIterator<Item = Type<'db>>,
        I::IntoIter: Clone,
    {
        let elements: Vec<_> = elements
            .into_iter()
            .enumerate()
            .map(|(index, ty)| {
                let observed = observations.and_then(|inputs| inputs.get(index));
                let disjunctive = if let (Some(observed), Some(proof)) = (observed, proof.as_ref())
                {
                    Self::observed_is_disjunctive(db, env, observed, false, &proof.context)
                } else {
                    Self::is_disjunctive(db, env, ty, normalization)
                };
                (ty, observed.cloned(), disjunctive)
            })
            .collect();
        // Before distributing multiple disjunctions, apply narrowing factors regardless of their
        // input order. With at most one disjunction, retain the original intersection element order.
        // Classification follows aliases and negations without expanding into DNF; the builder
        // performs that expansion under its budget and recursion guard.
        let multiple_disjunctions = elements
            .iter()
            .filter(|(_, _, disjunctive)| *disjunctive)
            .nth(1)
            .is_some();
        let mut builder = Self::new(db, env).normalization(normalization);
        builder.proof = proof;
        for (element, observed, _) in elements
            .iter()
            .filter(|(_, _, disjunctive)| !multiple_disjunctions || !disjunctive)
            .chain(
                elements
                    .iter()
                    .filter(|(_, _, disjunctive)| multiple_disjunctions && *disjunctive),
            )
        {
            builder
                .add_positive_impl::<BoundedIntersection>(*element, &mut vec![], observed.as_ref())
                .continue_value()?;
        }
        Some(builder)
    }

    /// Whether expanding a factor can introduce alternatives, including through De Morgan's law.
    fn observed_is_disjunctive(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        observed: &ObservedType<'db>,
        negated: bool,
        context: &RelationContext<'db>,
    ) -> bool {
        if matches!(observed.ty, Type::TypeAlias(_)) {
            return context
                .observe(db, observed, || {
                    let body = observed.unfold_in_context(db, env, context)?;
                    Some(Self::observed_is_disjunctive(
                        db, env, &body, negated, context,
                    ))
                })
                .unwrap_or(false);
        }
        match observed.ty {
            Type::Union(_) => {
                !negated
                    || observed
                        .union_children(db, env)
                        .iter()
                        .any(|child| Self::observed_is_disjunctive(db, env, child, true, context))
            }
            Type::Intersection(intersection) => {
                (negated && intersection.positive(db).len() + intersection.negative(db).len() > 1)
                    || intersection
                        .positive(db)
                        .iter()
                        .enumerate()
                        .any(|(index, ty)| {
                            let child = observed.child_at(
                                db,
                                env,
                                *ty,
                                ObservationEdge::IntersectionPositive(index),
                            );
                            Self::observed_is_disjunctive(db, env, &child, negated, context)
                        })
                    || intersection
                        .negative(db)
                        .iter()
                        .enumerate()
                        .any(|(index, ty)| {
                            let child = observed.child_at(
                                db,
                                env,
                                *ty,
                                ObservationEdge::IntersectionNegative(index),
                            );
                            Self::observed_is_disjunctive(db, env, &child, !negated, context)
                        })
            }
            Type::EnumComplement(complement) => {
                let intersection =
                    observed.unchanged_or_unresolved(complement.to_intersection(db, env));
                Self::observed_is_disjunctive(db, env, &intersection, negated, context)
            }
            _ => false,
        }
    }

    fn is_disjunctive(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        normalization: TypeNormalization,
    ) -> bool {
        let mut pending = SmallVec::<[_; 4]>::from_slice(&[(ty, false)]);
        let mut seen_aliases = FxHashSet::default();
        while let Some((ty, negated)) = pending.pop() {
            match ty {
                Type::TypeAlias(_) if normalization == TypeNormalization::Semantic => {
                    if seen_aliases.insert((ty, negated)) {
                        pending.push((ty.resolve_type_alias(db), negated));
                    }
                }
                Type::Union(union) => {
                    if !negated {
                        return true;
                    }
                    pending.extend(union.elements(db).iter().map(|ty| (*ty, true)));
                }
                Type::Intersection(intersection) => {
                    if negated
                        && intersection.positive(db).len() + intersection.negative(db).len() > 1
                    {
                        return true;
                    }
                    pending.extend(intersection.positive(db).iter().map(|ty| (*ty, negated)));
                    pending.extend(intersection.negative(db).iter().map(|ty| (*ty, !negated)));
                }
                Type::EnumComplement(complement) => {
                    pending.push((complement.to_intersection(db, env), negated));
                }
                _ => {}
            }
        }
        false
    }

    pub(crate) fn add_positive(mut self, ty: Type<'db>) -> Self {
        self.add_positive_in_place(ty);
        self
    }

    pub(crate) fn add_positive_in_place(&mut self, ty: Type<'db>) {
        let ControlFlow::Continue(()) =
            self.add_positive_impl::<UnboundedIntersection>(ty, &mut vec![], None);
    }

    fn add_positive_impl<L: IntersectionLimits>(
        &mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
        observed: Option<&ObservedType<'db>>,
    ) -> ControlFlow<L::Break> {
        let db = self.db;
        if matches!(ty, Type::TypeAlias(_))
            && let (Some(observed), Some(context)) = (
                observed,
                self.proof.as_ref().map(|proof| proof.context.clone()),
            )
        {
            if let Some(result) = context.observe(db, observed, || {
                let body = observed.unfold_in_context(db, &self.env, &context)?;
                Some(self.add_positive_impl::<L>(body.ty, seen_aliases, Some(&body)))
            }) {
                return result;
            }
            for inner in &mut self.intersections {
                inner.add_positive_input(ty, Some(observed.unresolved()));
            }
            return ControlFlow::Continue(());
        }
        match ty {
            Type::TypeAlias(_) if self.normalization == TypeNormalization::Semantic => {
                if seen_aliases.contains(&ty) {
                    // Recursive alias, add it without expanding to avoid infinite recursion.
                    for inner in &mut self.intersections {
                        inner.positive.insert(ty);
                    }
                    return ControlFlow::Continue(());
                }
                seen_aliases.push(ty);
                let value_type = ty.resolve_type_alias(db);
                self.add_positive_impl::<L>(
                    value_type,
                    seen_aliases,
                    observed
                        .map(|input| input.unchanged_or_unresolved(value_type))
                        .as_ref(),
                )?;
            }
            Type::Union(union) => {
                // Distribute ourself over this union: for each union element, clone ourself and
                // intersect with that union element, then create a new union-of-intersections with all
                // of those sub-intersections in it. E.g. if `self` is a simple intersection `T1 & T2`
                // and we add `T3 | T4` to the intersection, we don't get `T1 & T2 & (T3 | T4)` (that's
                // not in DNF), we distribute the union and get `(T1 & T3) | (T2 & T3) | (T1 & T4) |
                // (T2 & T4)`. If `self` is already a union-of-intersections `(T1 & T2) | (T3 & T4)`
                // and we add `T5 | T6` to it, that flattens all the way out to `(T1 & T2 & T5) | (T1 &
                // T2 & T6) | (T3 & T4 & T5) ...` -- you get the idea.
                let mut distributed = FxIndexSet::default();
                for (index, elem) in union.elements(db).iter().enumerate() {
                    let mut branch = self.clone();
                    branch.add_positive_impl::<L>(
                        *elem,
                        seen_aliases,
                        observed
                            .map(|input| {
                                input.child_at(
                                    db,
                                    &self.env,
                                    *elem,
                                    ObservationEdge::UnionElement(index),
                                )
                            })
                            .as_ref(),
                    )?;
                    self.extend_distributed::<L>(&mut distributed, branch, self.has_disjunction)?;
                }
                self.intersections = distributed.into_iter().collect();
                self.has_disjunction = true;
            }
            // `(A & B & ~C) & (D & E & ~F)` -> `A & B & D & E & ~C & ~F`
            Type::Intersection(other) => {
                for (index, pos) in other.positive(db).iter().enumerate() {
                    self.add_positive_impl::<L>(
                        *pos,
                        seen_aliases,
                        observed
                            .map(|input| {
                                input.child_at(
                                    db,
                                    &self.env,
                                    *pos,
                                    ObservationEdge::IntersectionPositive(index),
                                )
                            })
                            .as_ref(),
                    )?;
                }
                for (index, neg) in other.negative(db).iter().enumerate() {
                    self.add_negative_impl::<L>(
                        *neg,
                        seen_aliases,
                        observed
                            .map(|input| {
                                input.child_at(
                                    db,
                                    &self.env,
                                    *neg,
                                    ObservationEdge::IntersectionNegative(index),
                                )
                            })
                            .as_ref(),
                    )?;
                }
            }
            Type::EnumComplement(complement)
                if self.normalization == TypeNormalization::Semantic =>
            {
                let intersection = complement.to_intersection(db, &self.env);
                self.add_positive_impl::<L>(
                    intersection,
                    seen_aliases,
                    observed
                        .map(|input| input.unchanged_or_unresolved(intersection))
                        .as_ref(),
                )?;
            }
            _ => {
                // If we are already a union-of-intersections, distribute the new intersected element
                // across all of those intersections.
                for inner in &mut self.intersections {
                    if let Some(session) = &self.proof {
                        inner.add_positive_input(
                            ty,
                            Some(observed.cloned().unwrap_or_else(|| session.observe(ty))),
                        );
                        inner.simplify_in_session(db, &self.env, session);
                        continue;
                    }
                    match self.normalization {
                        TypeNormalization::Semantic => inner.add_positive(db, &self.env, ty),
                        TypeNormalization::Structural => inner.add_positive_structural(ty),
                    }
                }
            }
        }
        ControlFlow::Continue(())
    }

    pub(crate) fn add_negative(mut self, ty: Type<'db>) -> Self {
        self.add_negative_in_place(ty);
        self
    }

    pub(crate) fn add_negative_in_place(&mut self, ty: Type<'db>) {
        let ControlFlow::Continue(()) =
            self.add_negative_impl::<UnboundedIntersection>(ty, &mut vec![], None);
    }

    fn add_negative_impl<L: IntersectionLimits>(
        &mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
        observed: Option<&ObservedType<'db>>,
    ) -> ControlFlow<L::Break> {
        let db = self.db;
        if matches!(ty, Type::TypeAlias(_))
            && let (Some(observed), Some(context)) = (
                observed,
                self.proof.as_ref().map(|proof| proof.context.clone()),
            )
        {
            if let Some(result) = context.observe(db, observed, || {
                let body = observed.unfold_in_context(db, &self.env, &context)?;
                Some(self.add_negative_impl::<L>(body.ty, seen_aliases, Some(&body)))
            }) {
                return result;
            }
            for inner in &mut self.intersections {
                inner.add_negative_input(ty, Some(observed.unresolved()));
            }
            return ControlFlow::Continue(());
        }
        // See comments above in `add_positive`; this is just the negated version.
        match ty {
            Type::TypeAlias(_) if self.normalization == TypeNormalization::Semantic => {
                if seen_aliases.contains(&ty) {
                    // Recursive alias, add it without expanding to avoid infinite recursion.
                    for inner in &mut self.intersections {
                        inner.negative.insert(ty);
                    }
                    return ControlFlow::Continue(());
                }
                seen_aliases.push(ty);
                let value_type = ty.resolve_type_alias(db);
                self.add_negative_impl::<L>(
                    value_type,
                    seen_aliases,
                    observed
                        .map(|input| input.unchanged_or_unresolved(value_type))
                        .as_ref(),
                )?;
            }
            Type::Union(union) => {
                for (index, elem) in union.elements(db).iter().enumerate() {
                    self.add_negative_impl::<L>(
                        *elem,
                        seen_aliases,
                        observed
                            .map(|input| {
                                input.child_at(
                                    db,
                                    &self.env,
                                    *elem,
                                    ObservationEdge::UnionElement(index),
                                )
                            })
                            .as_ref(),
                    )?;
                }
            }
            Type::Intersection(intersection) => {
                // (A | B) & ~(C & ~D)
                // -> (A | B) & (~C | D)
                // -> ((A | B) & ~C) | ((A | B) & D)
                // i.e. if we have an intersection of positive constraints C
                // and negative constraints D, then our new intersection
                // is (existing & ~C) | (existing & D)

                let mut distributed = FxIndexSet::default();
                // A single negative element can encode double negation. It only introduces a
                // disjunction if expanding that element does, for example `~~Alias` for a union.
                let branches = intersection.positive(db).len() + intersection.negative(db).len();
                let check_budget = self.has_disjunction && branches > 1;
                let mut has_disjunction = self.has_disjunction || branches > 1;
                // We negate all the positive constraints while distributing.
                for (index, elem) in intersection.positive(db).iter().enumerate() {
                    let mut branch = self.clone();
                    branch.add_negative_impl::<L>(
                        *elem,
                        &mut seen_aliases.clone(),
                        observed
                            .map(|input| {
                                input.child_at(
                                    db,
                                    &self.env,
                                    *elem,
                                    ObservationEdge::IntersectionPositive(index),
                                )
                            })
                            .as_ref(),
                    )?;
                    has_disjunction |= branch.has_disjunction;
                    self.extend_distributed::<L>(&mut distributed, branch, check_budget)?;
                }
                // All negative constraints end up becoming positive constraints.
                for (index, elem) in intersection.negative(db).iter().enumerate() {
                    let mut branch = self.clone();
                    branch.add_positive_impl::<L>(
                        *elem,
                        &mut seen_aliases.clone(),
                        observed
                            .map(|input| {
                                input.child_at(
                                    db,
                                    &self.env,
                                    *elem,
                                    ObservationEdge::IntersectionNegative(index),
                                )
                            })
                            .as_ref(),
                    )?;
                    has_disjunction |= branch.has_disjunction;
                    self.extend_distributed::<L>(&mut distributed, branch, check_budget)?;
                }
                self.intersections = distributed.into_iter().collect();
                self.has_disjunction = has_disjunction;
            }
            Type::EnumComplement(complement)
                if self.normalization == TypeNormalization::Semantic =>
            {
                let intersection = complement.to_intersection(db, &self.env);
                self.add_negative_impl::<L>(
                    intersection,
                    seen_aliases,
                    observed
                        .map(|input| input.unchanged_or_unresolved(intersection))
                        .as_ref(),
                )?;
            }
            _ => {
                for inner in &mut self.intersections {
                    if let Some(session) = &self.proof {
                        inner.add_negative_input(
                            ty,
                            Some(observed.cloned().unwrap_or_else(|| session.observe(ty))),
                        );
                        inner.simplify_in_session(db, &self.env, session);
                        continue;
                    }
                    match self.normalization {
                        TypeNormalization::Semantic => inner.add_negative(db, &self.env, ty),
                        TypeNormalization::Structural => inner.add_negative_structural(ty),
                    }
                }
            }
        }
        ControlFlow::Continue(())
    }

    pub(crate) fn positive_elements<I, T>(mut self, elements: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        for element in elements {
            self.add_positive_in_place(element.into());
        }
        self
    }

    pub(in crate::types) fn build_observed(self) -> ObservedType<'db> {
        let children: Vec<_> = self
            .intersections
            .into_iter()
            .map(|inner| {
                let ty = inner.clone().build_structural(self.db);
                inner.observe(ty)
            })
            .collect();
        if let Some(proof) = self.proof {
            let mut union =
                UnionBuilder::new(self.db, &self.env).with_observed_context(proof.context);
            for child in children {
                union.add_observed_in_place(child);
            }
            union.build_observed()
        } else {
            let mut union =
                UnionBuilder::new(self.db, &self.env).normalization(TypeNormalization::Structural);
            for child in &children {
                union.add_in_place(child.ty);
            }
            ObservedType::dependent_on(union.build(), &children)
        }
    }

    pub(crate) fn build(self) -> Type<'db> {
        let db = self.db;
        if self.normalization == TypeNormalization::Structural || self.proof.is_some() {
            let mut union =
                UnionBuilder::new(db, &self.env).normalization(TypeNormalization::Structural);
            for inner in self.intersections {
                union.add_structural_dnf(inner.build_structural(db));
            }
            return union.build();
        }
        UnionType::from_elements(
            db,
            &self.env,
            self.intersections
                .into_iter()
                .map(|inner| inner.build(db, &self.env)),
        )
    }
}

/// The signs of a pair of intersection elements. For `Mixed`, the first is positive.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
enum IntersectionPolarity {
    Positive,
    Negative,
    Mixed,
}

/// Describes the signed intersection elements, so `Disjoint` also covers `S & ~T` when `S <: T`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::SalsaValue, get_size2::GetSize)]
enum IntersectionSimplification {
    Unchanged,
    FirstRedundant,
    SecondRedundant,
    Disjoint,
}

fn simplify_intersection_pair<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    first: Type<'db>,
    second: Type<'db>,
    polarity: IntersectionPolarity,
) -> IntersectionSimplification {
    // Built-in literal values have no inference dependencies, so these simplifications cannot
    // participate in a cycle and do not need an interned pair or a tracked relation query.
    if let (Type::LiteralValue(first), Type::LiteralValue(second)) = (first, second)
        && matches!(
            first.kind(),
            LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
        )
        && matches!(
            second.kind(),
            LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
        )
    {
        return match (polarity, first.kind() == second.kind()) {
            (IntersectionPolarity::Positive, true) => {
                // Redundancy depends on promotability and full literal identity, including
                // the recursive-definition flag. Subtyping only compares the literal values.
                if first == second || first.is_promotable() {
                    IntersectionSimplification::SecondRedundant
                } else if second.is_promotable() {
                    IntersectionSimplification::FirstRedundant
                } else {
                    IntersectionSimplification::Unchanged
                }
            }
            (IntersectionPolarity::Positive, false) | (IntersectionPolarity::Mixed, true) => {
                IntersectionSimplification::Disjoint
            }
            (IntersectionPolarity::Negative, true) | (IntersectionPolarity::Mixed, false) => {
                IntersectionSimplification::SecondRedundant
            }
            (IntersectionPolarity::Negative, false) => IntersectionSimplification::Unchanged,
        };
    }

    simplify_intersection_pair_impl(
        db,
        TypePair::new(db, env.program(db), first, second),
        polarity,
    )
}

/// Simplify a pair of intersection elements using non-circular relation checks.
///
/// If this simplification participates in an inference cycle, retain both signed
/// elements. Ordinary type relations keep their usual cycle handling, including for
/// recursive protocols.
///
/// ```python
/// class C:
///     def __init__(self):
///         if not hasattr(self, "x"):
///             self.x = self.__str__
/// ```
///
/// Inferring `C.x` needs the guarded type of `self`. The guard cannot use that unfinished
/// inference to prove that `C` already satisfies the protocol for `x` and erase the branch.
#[salsa::tracked(
    returns(copy),
    cycle_result=|_, _, _, _| IntersectionSimplification::Unchanged,
    heap_size=ruff_memory_usage::heap_size,
)]
fn simplify_intersection_pair_impl<'db>(
    db: &'db dyn Db,
    types: TypePair<'db>,
    polarity: IntersectionPolarity,
) -> IntersectionSimplification {
    let env = ProgramEnvironment::from_program(types.program(db));
    simplify_intersection_pair_using(
        types.first(db),
        types.second(db),
        polarity,
        |left, right| left.is_redundant_with(db, &env, right),
        |left, right| left.is_subtype_of(db, &env, right),
        |left, right| left.is_disjoint_from(db, &env, right),
    )
}

fn simplify_intersection_pair_using<T: Copy>(
    first: T,
    second: T,
    polarity: IntersectionPolarity,
    is_redundant: impl Fn(T, T) -> bool,
    is_subtype: impl Fn(T, T) -> bool,
    is_disjoint: impl Fn(T, T) -> bool,
) -> IntersectionSimplification {
    match polarity {
        IntersectionPolarity::Positive => {
            // S & T = S if S <: T.
            if is_redundant(first, second) {
                return IntersectionSimplification::SecondRedundant;
            }
            let first_redundant = is_redundant(second, first);
            if is_disjoint(second, first) {
                return IntersectionSimplification::Disjoint;
            }
            if first_redundant {
                return IntersectionSimplification::FirstRedundant;
            }
        }
        IntersectionPolarity::Negative => {
            // ~S & ~T = ~T if S <: T; the narrower exclusion is redundant.
            let first_redundant = is_redundant(first, second);
            if is_subtype(second, first) {
                return IntersectionSimplification::SecondRedundant;
            }
            if first_redundant {
                return IntersectionSimplification::FirstRedundant;
            }
        }
        IntersectionPolarity::Mixed => {
            // S & ~T = Never if S <: T, and S & ~T = S if S and T are disjoint.
            if is_subtype(first, second) {
                return IntersectionSimplification::Disjoint;
            }
            if is_disjoint(first, second) {
                return IntersectionSimplification::SecondRedundant;
            }
        }
    }
    IntersectionSimplification::Unchanged
}

#[derive(Debug, Clone, Default)]
struct IntersectionSources<'db> {
    positive: Vec<ObservedType<'db>>,
    negative: Vec<ObservedType<'db>>,
}

#[derive(Debug, Clone, Default)]
struct InnerIntersectionBuilder<'db> {
    positive: FxOrderSet<Type<'db>>,
    negative: NegativeIntersectionElements<'db>,
    // Only proof-owned construction fills these slots. They do not affect structural
    // deduplication; merging equal branches explicitly retains both sets of contributors.
    sources: Option<Box<IntersectionSources<'db>>>,
}

impl PartialEq for InnerIntersectionBuilder<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.positive == other.positive && self.negative == other.negative
    }
}
impl Eq for InnerIntersectionBuilder<'_> {}
impl Hash for InnerIntersectionBuilder<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.positive.hash(state);
        self.negative.hash(state);
    }
}

impl<'db> InnerIntersectionBuilder<'db> {
    fn contributors(&self) -> Vec<ObservedType<'db>> {
        self.sources.as_ref().map_or_else(Vec::new, |sources| {
            sources
                .positive
                .iter()
                .chain(&sources.negative)
                .cloned()
                .collect()
        })
    }

    fn observe(&self, ty: Type<'db>) -> ObservedType<'db> {
        let inputs = self.contributors();
        match inputs.as_slice() {
            [input] if self.negative.is_empty() => input.unchanged_or_unresolved(ty),
            _ => ObservedType::dependent_on(ty, &inputs),
        }
    }

    fn merge_contributors(&mut self, other: &Self) {
        let incoming = other.contributors();
        if incoming.is_empty() {
            return;
        }
        if let Some(sources) = self.sources.as_deref_mut() {
            for source in sources.positive.iter_mut().chain(&mut sources.negative) {
                let mut contributors = vec![source.clone()];
                contributors.extend_from_slice(&incoming);
                *source = ObservedType::dependent_on(source.ty, &contributors);
            }
        }
    }

    fn record_input(&mut self, positive: bool, index: usize, input: Option<ObservedType<'db>>) {
        let Some(input) = input else {
            return;
        };
        let sources = self.sources.get_or_insert_with(Box::default);
        let slots = if positive {
            &mut sources.positive
        } else {
            &mut sources.negative
        };
        if let Some(existing) = slots.get_mut(index) {
            *existing = ObservedType::dependent_on(existing.ty, &[existing.clone(), input]);
        } else {
            slots.push(input);
        }
    }

    fn collapse_structural(&mut self, ty: Type<'db>, input: Option<ObservedType<'db>>) {
        let mut contributors = self.contributors();
        contributors.extend(input);
        *self = Self::default();
        self.positive.insert(ty);
        if !contributors.is_empty() {
            self.record_input(true, 0, Some(ObservedType::dependent_on(ty, &contributors)));
        }
    }

    /// Simplify a solver's bounds within its existing proof, before charging the distribution
    /// budget. An unresolved relation retains both elements.
    fn simplify_in_session(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        session: &NormalizationProof<'db>,
    ) {
        'simplify: loop {
            let elements: Vec<_> = self
                .positive
                .iter()
                .copied()
                .enumerate()
                .map(|(index, ty)| (true, index, ty))
                .chain(
                    self.negative
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(index, ty)| (false, index, ty)),
                )
                .collect();
            for (index, &first) in elements.iter().enumerate() {
                for &second in &elements[index + 1..] {
                    let polarity = match (first.0, second.0) {
                        (true, true) => IntersectionPolarity::Positive,
                        (false, false) => IntersectionPolarity::Negative,
                        _ => IntersectionPolarity::Mixed,
                    };
                    let observe = |(positive, index, ty)| {
                        self.sources
                            .as_ref()
                            .and_then(|sources| {
                                if positive {
                                    sources.positive.get(index)
                                } else {
                                    sources.negative.get(index)
                                }
                            })
                            .cloned()
                            .unwrap_or_else(|| session.observe(ty))
                    };
                    let first_observed = observe(first);
                    let second_observed = observe(second);
                    let remove = match simplify_intersection_pair_using(
                        &first_observed,
                        &second_observed,
                        polarity,
                        |left, right| {
                            session
                                .context
                                .is_redundant(db, env, left.clone(), right.clone())
                        },
                        |left, right| {
                            session
                                .context
                                .is_subtype_eager(db, env, left.clone(), right.clone())
                        },
                        |left, right| {
                            session
                                .context
                                .is_disjoint(db, env, left.clone(), right.clone())
                        },
                    ) {
                        IntersectionSimplification::Unchanged => continue,
                        IntersectionSimplification::Disjoint => {
                            self.collapse_structural(
                                Type::Never,
                                Some(ObservedType::dependent_on(
                                    Type::Never,
                                    &[first_observed, second_observed],
                                )),
                            );
                            return;
                        }
                        IntersectionSimplification::FirstRedundant => first,
                        IntersectionSimplification::SecondRedundant => second,
                    };
                    let keep = if remove == first { second } else { first };
                    if let Some(sources) = self.sources.as_mut() {
                        let kept = if keep.0 {
                            sources.positive.get_mut(keep.1)
                        } else {
                            sources.negative.get_mut(keep.1)
                        };
                        if let Some(kept) = kept {
                            *kept = ObservedType::dependent_on(
                                kept.ty,
                                &[first_observed, second_observed],
                            );
                        }
                        let removed = if remove.0 {
                            &mut sources.positive
                        } else {
                            &mut sources.negative
                        };
                        if remove.1 < removed.len() {
                            removed.swap_remove(remove.1);
                        }
                    }
                    if remove.0 {
                        self.positive.swap_remove_index(remove.1);
                    } else {
                        self.negative.swap_remove_index(remove.1);
                    }
                    continue 'simplify;
                }
            }
            return;
        }
    }

    fn add_positive_structural(&mut self, ty: Type<'db>) {
        self.add_positive_input(ty, None);
    }

    fn add_positive_input(&mut self, ty: Type<'db>, input: Option<ObservedType<'db>>) {
        if self.contains_never() {
            return;
        }
        if ty.is_never() {
            self.collapse_structural(ty, input);
            return;
        }
        if self.positive.iter().any(Type::is_pending_narrowing) {
            return;
        }
        if ty.is_divergent() {
            self.collapse_structural(ty, input);
            return;
        }
        if !self.positive.iter().any(Type::is_divergent) && ty != Type::object() {
            let (index, _) = self.positive.insert_full(ty);
            self.record_input(true, index, input);
        }
    }

    fn add_negative_structural(&mut self, ty: Type<'db>) {
        self.add_negative_input(ty, None);
    }

    fn add_negative_input(&mut self, ty: Type<'db>, input: Option<ObservedType<'db>>) {
        if self.contains_never()
            || (self.positive.iter().any(Type::is_divergent) && !ty.is_pending_narrowing())
        {
            return;
        }
        if ty == Type::object() {
            self.collapse_structural(Type::Never, input);
        } else if let Some(negated) = ty.negated_divergent() {
            self.collapse_structural(negated, input);
        } else if matches!(ty, Type::Dynamic(_)) {
            self.add_positive_input(ty, input);
        } else if !ty.is_never() {
            let index = self
                .negative
                .iter()
                .position(|existing| *existing == ty)
                .unwrap_or(self.negative.len());
            self.negative.insert(ty);
            self.record_input(false, index, input);
        }
    }

    fn build_structural(mut self, db: &'db dyn Db) -> Type<'db> {
        match (self.positive.len(), self.negative.len()) {
            (0, 0) => Type::object(),
            (1, 0) => self.positive[0],
            _ => {
                self.positive.shrink_to_fit();
                self.negative.shrink_to_fit();
                Type::Intersection(IntersectionType::new(db, self.positive, self.negative))
            }
        }
    }

    fn contains_never(&self) -> bool {
        self.positive.contains(&Type::Never)
    }

    /// Return `true` when an intersection excludes every member of an enum class.
    ///
    /// This recognizes enum complements that have become empty, such as
    /// `Color & ~Literal[Color.RED] & ~Literal[Color.BLUE]` for a two-member enum.
    ///
    /// ```python
    /// from enum import Enum
    ///
    /// class Color(Enum):
    ///     RED = 1
    ///     BLUE = 2
    ///
    /// def f(color: Color):
    ///     if color is not Color.RED and color is not Color.BLUE:
    ///         reveal_type(color)  # Never
    /// ```
    fn has_empty_enum_complement(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        for positive in &self.positive {
            let Type::NominalInstance(instance) = positive else {
                continue;
            };

            let Some(enum_class_literal) = instance.class_literal(db, env).into_enum_class(db)
            else {
                continue;
            };
            if !enum_class_literal.members_are_exhaustive(db) {
                continue;
            }

            let mut excluded_names = FxHashSet::default();
            for negative in &self.negative {
                let Some(enum_literal) = negative.as_enum_literal() else {
                    continue;
                };
                if enum_literal.enum_class_literal(db) != enum_class_literal {
                    continue;
                }

                let name = enum_literal.name(db);
                let Some(canonical_name) = enum_class_literal.resolve_member(db, name) else {
                    continue;
                };
                excluded_names.insert(canonical_name.clone());
            }

            if excluded_names.is_empty() {
                continue;
            }

            if enum_class_literal
                .member_names(db)
                .all(|name| excluded_names.contains(name))
            {
                return true;
            }
        }

        false
    }

    /// Adds a positive type to this intersection.
    fn add_positive(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut new_positive: Type<'db>,
    ) {
        // `Never & T` -> `Never`
        if self.positive.contains(&Type::Never) {
            return;
        }

        // `T & Never` -> `Never`
        if new_positive.is_never() {
            *self = Self::default();
            self.positive.insert(Type::Never);
            return;
        }

        // `T & Divergent` -> `Divergent`. Conceptually, `Divergent` behaves like `Never` here and
        // dominates intersections. However, `Divergent` is actually a dynamic/gradual type, so
        // `~Divergent` acts like `Divergent` rather than dropping out like `~Never` does.
        // `Divergent` also gets a lot of special handling in cycle recovery.
        // Pending narrowing takes precedence over recursive markers: the predicate must be
        // resolved before its intersection can contribute to the inferred type.
        if self.positive.iter().any(Type::is_pending_narrowing) {
            return;
        }
        if new_positive.is_divergent() {
            *self = Self::default();
            self.positive.insert(new_positive);
            return;
        }
        // `Divergent & T` -> `Divergent`
        if self.positive.iter().any(Type::is_divergent) {
            return;
        }

        // A runtime class value of `TypeForm[T]` has type `type[T]`.
        match new_positive {
            Type::TypeForm(typeform) => {
                if let Ok(narrowed) = SubclassOfType::try_from_instance(
                    db,
                    env,
                    typeform.type_argument(db).resolve_type_alias(db),
                ) && self
                    .positive
                    .swap_remove(&KnownClass::Type.to_instance(db, env))
                {
                    new_positive = narrowed;
                }
            }
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                if let Some((index, narrowed)) =
                    self.positive
                        .iter()
                        .enumerate()
                        .find_map(|(index, positive)| match positive {
                            Type::TypeForm(typeform) => SubclassOfType::try_from_instance(
                                db,
                                env,
                                typeform.type_argument(db).resolve_type_alias(db),
                            )
                            .ok()
                            .map(|narrowed| (index, narrowed)),
                            _ => None,
                        })
                {
                    self.positive.swap_remove_index(index);
                    new_positive = narrowed;
                }
            }
            _ => {}
        }

        match new_positive {
            // `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
            Type::AlwaysTruthy if self.positive.contains(&Type::literal_string()) => {
                self.add_negative(db, env, Type::string_literal(db, ""));
            }
            // `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::AlwaysFalsy if self.positive.swap_remove(&Type::literal_string()) => {
                self.add_positive(db, env, Type::string_literal(db, ""));
            }
            // `AlwaysTruthy & LiteralString` -> `LiteralString & ~Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string()
                    && self.positive.swap_remove(&Type::AlwaysTruthy) =>
            {
                self.add_positive(db, env, Type::literal_string());
                self.add_negative(db, env, Type::string_literal(db, ""));
            }
            // `AlwaysFalsy & LiteralString` -> `Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string() && self.positive.swap_remove(&Type::AlwaysFalsy) =>
            {
                self.add_positive(db, env, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string()
                    && self.negative.swap_remove(&Type::AlwaysTruthy) =>
            {
                self.add_positive(db, env, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string() && self.negative.swap_remove(&Type::AlwaysFalsy) =>
            {
                self.add_positive(db, env, Type::literal_string());
                self.add_negative(db, env, Type::string_literal(db, ""));
            }

            _ => {
                let positive_as_instance = new_positive.as_nominal_instance();

                if let Some(instance) = positive_as_instance
                    && instance.is_object()
                {
                    // `object & T` -> `T`; it is always redundant to add `object` to an intersection
                    return;
                }

                let addition_is_bool_instance = positive_as_instance
                    .is_some_and(|instance| instance.has_known_class(db, KnownClass::Bool));

                for (index, existing_positive) in self.positive.iter().enumerate() {
                    match existing_positive {
                        // `AlwaysTruthy & bool` -> `Literal[True]`
                        Type::AlwaysTruthy if addition_is_bool_instance => {
                            new_positive = Type::bool_literal(true);
                        }
                        // `AlwaysFalsy & bool` -> `Literal[False]`
                        Type::AlwaysFalsy if addition_is_bool_instance => {
                            new_positive = Type::bool_literal(false);
                        }
                        Type::NominalInstance(instance)
                            if instance.has_known_class(db, KnownClass::Bool) =>
                        {
                            match new_positive {
                                // `bool & AlwaysTruthy` -> `Literal[True]`
                                Type::AlwaysTruthy => {
                                    new_positive = Type::bool_literal(true);
                                }
                                // `bool & AlwaysFalsy` -> `Literal[False]`
                                Type::AlwaysFalsy => {
                                    new_positive = Type::bool_literal(false);
                                }
                                _ => continue,
                            }
                        }
                        _ => continue,
                    }
                    self.positive.swap_remove_index(index);
                    break;
                }

                if addition_is_bool_instance {
                    for (index, existing_negative) in self.negative.iter().enumerate() {
                        match existing_negative {
                            // `bool & ~Literal[False]` -> `Literal[True]`
                            // `bool & ~Literal[True]` -> `Literal[False]`
                            Type::LiteralValue(literal) => match literal.kind() {
                                LiteralValueTypeKind::Bool(bool_value) => {
                                    new_positive = Type::bool_literal(!bool_value);
                                }
                                _ => continue,
                            },
                            // `bool & ~AlwaysTruthy` -> `Literal[False]`
                            Type::AlwaysTruthy => {
                                new_positive = Type::bool_literal(false);
                            }
                            // `bool & ~AlwaysFalsy` -> `Literal[True]`
                            Type::AlwaysFalsy => {
                                new_positive = Type::bool_literal(true);
                            }
                            _ => continue,
                        }
                        self.negative.swap_remove_index(index);
                        break;
                    }
                }

                let mut to_remove = SmallVec::<[usize; 1]>::new();
                let mut replacement = None;
                for (index, existing_positive) in self.positive.iter().enumerate() {
                    if let Some(result) =
                        generic_gradual_intersection(db, env, new_positive, *existing_positive)
                    {
                        let GenericIntersection::Simplified(merged) = result else {
                            continue;
                        };
                        if merged == *existing_positive {
                            return;
                        }
                        replacement = Some((index, merged));
                        break;
                    }
                    match simplify_intersection_pair(
                        db,
                        env,
                        *existing_positive,
                        new_positive,
                        IntersectionPolarity::Positive,
                    ) {
                        IntersectionSimplification::Unchanged => {}
                        IntersectionSimplification::SecondRedundant => return,
                        IntersectionSimplification::FirstRedundant => to_remove.push(index),
                        IntersectionSimplification::Disjoint => {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                    }
                }
                if let Some((index, value)) = replacement {
                    self.positive.swap_remove_index(index);
                    self.add_positive(db, env, value);
                    return;
                }
                for index in to_remove.into_iter().rev() {
                    self.positive.swap_remove_index(index);
                }

                let mut to_remove = SmallVec::<[usize; 1]>::new();
                for (index, existing_negative) in self.negative.iter().enumerate() {
                    match simplify_intersection_pair(
                        db,
                        env,
                        new_positive,
                        *existing_negative,
                        IntersectionPolarity::Mixed,
                    ) {
                        IntersectionSimplification::Unchanged => {}
                        IntersectionSimplification::SecondRedundant => to_remove.push(index),
                        IntersectionSimplification::FirstRedundant => return,
                        IntersectionSimplification::Disjoint => {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                    }
                }
                for index in to_remove.into_iter().rev() {
                    self.negative.swap_remove_index(index);
                }

                self.positive.insert(new_positive);
            }
        }
    }

    /// Adds a negative type to this intersection.
    fn add_negative(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        new_negative: Type<'db>,
    ) {
        // `Never & ~T` -> `Never`.
        if self.positive.contains(&Type::Never) {
            return;
        }

        // `Divergent & ~T` -> `Divergent`.
        if self.positive.iter().any(Type::is_divergent) && !new_negative.is_pending_narrowing() {
            debug_assert_eq!(self.positive.len(), 1, "`Divergent` should be alone");
            return;
        }

        if let Some(negated_divergent) = new_negative.negated_divergent() {
            *self = Self::default();
            self.positive.insert(negated_divergent);
            return;
        }

        let contains_bool = || {
            self.positive
                .iter()
                .filter_map(|ty| ty.as_nominal_instance())
                .filter_map(|instance| instance.known_class(db))
                .any(KnownClass::is_bool)
        };

        match new_negative {
            Type::Intersection(inter) => {
                for pos in inter.positive(db) {
                    self.add_negative(db, env, *pos);
                }
                for neg in inter.negative(db) {
                    self.add_positive(db, env, *neg);
                }
            }
            Type::Never => {
                // Adding ~Never to an intersection is a no-op.
            }
            Type::NominalInstance(instance) if instance.is_object() => {
                // Adding ~object to an intersection results in Never.
                *self = Self::default();
                self.positive.insert(Type::Never);
            }
            ty @ Type::Dynamic(_) => {
                // Adding any of these types to the negative side of an intersection
                // is equivalent to adding it to the positive side. We do this to
                // simplify the representation.
                self.add_positive(db, env, ty);
            }
            // `bool & ~AlwaysTruthy` -> `bool & Literal[False]`
            Type::AlwaysTruthy if contains_bool() => {
                self.add_positive(db, env, Type::bool_literal(false));
            }
            // `bool & ~Literal[True]` -> `bool & Literal[False]`
            Type::LiteralValue(literal) if literal.as_bool() == Some(true) && contains_bool() => {
                self.add_positive(db, env, Type::bool_literal(false));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & Literal[""]`
            Type::AlwaysTruthy if self.positive.contains(&Type::literal_string()) => {
                self.add_positive(db, env, Type::string_literal(db, ""));
            }
            // `bool & ~AlwaysFalsy` -> `bool & Literal[True]`
            Type::AlwaysFalsy if contains_bool() => {
                self.add_positive(db, env, Type::bool_literal(true));
            }
            // `bool & ~Literal[False]` -> `bool & Literal[True]`
            Type::LiteralValue(literal) if literal.as_bool() == Some(false) && contains_bool() => {
                self.add_positive(db, env, Type::bool_literal(true));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::AlwaysFalsy if self.positive.contains(&Type::literal_string()) => {
                self.add_negative(db, env, Type::string_literal(db, ""));
            }
            _ => {
                let new_negative_enum = new_negative.as_enum_literal();
                let mut to_remove = SmallVec::<[usize; 1]>::new();
                for (index, existing_negative) in self.negative.iter().enumerate() {
                    if let Some(new_enum) = new_negative_enum
                        && existing_negative
                            .as_enum_literal()
                            .is_some_and(|existing_enum| {
                                existing_enum.enum_class(db) == new_enum.enum_class(db)
                            })
                    {
                        if existing_negative.as_enum_literal() == Some(new_enum) {
                            return;
                        }
                        continue;
                    }

                    match simplify_intersection_pair(
                        db,
                        env,
                        *existing_negative,
                        new_negative,
                        IntersectionPolarity::Negative,
                    ) {
                        IntersectionSimplification::Unchanged => {}
                        IntersectionSimplification::SecondRedundant => return,
                        IntersectionSimplification::FirstRedundant => to_remove.push(index),
                        IntersectionSimplification::Disjoint => {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                    }
                }
                for index in to_remove.into_iter().rev() {
                    self.negative.swap_remove_index(index);
                }

                let mut to_remove = SmallVec::<[usize; 1]>::new();
                for (index, existing_positive) in self.positive.iter().enumerate() {
                    if let Some(new_enum) = new_negative_enum {
                        if let Some(existing_enum) = existing_positive.as_enum_literal()
                            && existing_enum.enum_class(db) == new_enum.enum_class(db)
                        {
                            if existing_enum == new_enum {
                                *self = Self::default();
                                self.positive.insert(Type::Never);
                            }
                            return;
                        }

                        if existing_positive
                            .as_nominal_instance()
                            .is_some_and(|instance| {
                                instance.class_literal(db, env) == new_enum.enum_class(db)
                            })
                        {
                            continue;
                        }
                    }

                    match simplify_intersection_pair(
                        db,
                        env,
                        *existing_positive,
                        new_negative,
                        IntersectionPolarity::Mixed,
                    ) {
                        IntersectionSimplification::Unchanged => {}
                        IntersectionSimplification::SecondRedundant => return,
                        IntersectionSimplification::FirstRedundant => to_remove.push(index),
                        IntersectionSimplification::Disjoint => {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                    }
                }

                for index in to_remove.into_iter().rev() {
                    self.positive.swap_remove_index(index);
                }

                self.negative.insert(new_negative);
            }
        }
    }

    /// Tries to simplify any constrained typevars in the intersection.
    ///
    /// We must preserve the constrained `TypeVar` itself in the result, even if only a single
    /// compatible constraint remains, because other occurrences of the same `TypeVar` still need
    /// to correlate with it (for example, when returning a narrowed value as `T`).
    ///
    /// - If the intersection contains negative entries for all but one of the constraints, we can
    ///   add that remaining constraint as a positive entry.
    ///
    /// - If the intersection contains negative entries for all of the constraints, the overall
    ///   intersection is `Never`.
    fn simplify_constrained_typevars(&mut self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) {
        let mut to_add = SmallVec::<[Type<'db>; 1]>::new();

        for ty in &self.positive {
            let Type::TypeVar(bound_typevar) = ty else {
                continue;
            };
            let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
                bound_typevar.typevar(db).bound_or_constraints(db, env)
            else {
                continue;
            };

            // Determine which constraints appear as negative entries in the intersection.
            let constraints = constraints.elements(db);
            let mut remaining_constraints: Vec<_> = constraints.iter().copied().map(Some).collect();
            for negative in &self.negative {
                // This linear search should be fine as long as we don't encounter typevars with
                // thousands of constraints.
                let matching_constraints = constraints
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.is_subtype_of(db, env, *negative));
                for (constraint_index, _) in matching_constraints {
                    remaining_constraints[constraint_index] = None;
                }
            }

            let mut iter = remaining_constraints.into_iter().flatten();
            let Some(remaining_constraint) = iter.next() else {
                // All of the typevar constraints have been removed, so the entire intersection is
                // `Never`.
                *self = Self::default();
                self.positive.insert(Type::Never);
                return;
            };

            let more_than_one_remaining_constraint = iter.next().is_some();
            if more_than_one_remaining_constraint {
                // This typevar cannot be simplified.
                continue;
            }

            // Only one typevar constraint remains. Adding it as a positive element lets the normal
            // intersection simplification remove any incompatible negatives, while keeping the
            // original typevar in the result.
            to_add.push(remaining_constraint);
        }

        for remaining_constraint in to_add {
            self.add_positive(db, env, remaining_constraint);
        }
    }

    fn build(mut self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        if self.has_empty_enum_complement(db, env) {
            return Type::Never;
        }

        self.simplify_constrained_typevars(db, env);

        // If any typevars are in `self.positive`, speculatively solve all bounded type variables
        // to their upper bound and all constrained type variables to the union of their constraints.
        // If that speculative intersection simplifies to `Never`, this intersection must also simplify
        // to `Never`.
        if self
            .positive
            .iter()
            .any(|ty| matches!(ty, Type::TypeVar(_) | Type::NewTypeInstance(_)))
        {
            let speculative =
                expand_intersection_typevars_and_newtypes(db, env, &self.positive, &self.negative);
            if speculative.is_never() {
                return Type::Never;
            }

            if let Type::EnumComplement(complement) = speculative
                && complement.is_singleton(db)
                && self
                    .positive
                    .iter()
                    .any(|positive| matches!(positive, Type::NewTypeInstance(_)))
            {
                // Preserve the NewType while making its remaining enum member explicit.
                self.add_positive(db, env, complement.remaining_literal_union(db, env));
            }
        }

        if let Some(complement) =
            EnumComplement::from_intersection_parts(db, env, &self.positive, &self.negative)
        {
            return Type::EnumComplement(complement);
        }

        match (self.positive.len(), self.negative.len()) {
            (0, 0) => Type::object(),
            (1, 0) => self.positive[0],
            _ => {
                self.positive.shrink_to_fit();
                self.negative.shrink_to_fit();
                Type::Intersection(IntersectionType::new(db, self.positive, self.negative))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IntersectionBuilder, IntersectionPolarity, MAX_NON_RECURSIVE_UNION_LITERALS,
        MAX_RECURSIVE_UNION_LITERALS, RecursivelyDefined, Type, TypeNormalization, UnionBuilder,
        UnionType, simplify_intersection_pair, simplify_intersection_pair_impl,
    };

    use crate::db::tests::{TestDb, setup_db};
    use crate::place::{global_symbol, known_module_symbol};
    use crate::types::enums::enum_member_literals;
    use crate::types::projection::ObservedType;
    use crate::types::relation::RelationContext;
    use crate::types::tuple::TupleType;
    use crate::types::type_alias::TypeAliasType;
    use crate::types::{
        BytesLiteralType, KnownClass, KnownInstanceType, LiteralValueType, LiteralValueTypeKind,
        Signature, StringLiteralType, Truthiness, TypeContext, TypeMapping, TypePair,
    };

    use ruff_db::system::DbWithWritableSystem as _;
    use ty_module_resolver::KnownModule;
    use ty_python_core::ProgramFile;

    #[test]
    fn build_union_no_elements() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let empty_union = UnionBuilder::new(db, &env).build();
        assert_eq!(empty_union, Type::Never);
    }

    #[test]
    fn observed_union_retains_absorbed_operands() {
        let db = setup_db();
        let env = db.program_environment();
        let integer = KnownClass::Int.to_instance(&db, &env);
        let boolean = KnownClass::Bool.to_instance(&db, &env);
        for types in [[integer, boolean], [boolean, integer]] {
            let mut builder =
                UnionBuilder::new(&db, &env).with_observed_context(RelationContext::default());
            builder.add_observed_in_place(
                ObservedType::root(Type::unknown()).unchanged_or_unresolved(types[0]),
            );
            builder.add_observed_in_place(
                ObservedType::root(Type::object()).unchanged_or_unresolved(types[1]),
            );
            let observed = builder.build_observed().recipe().observe();
            assert_eq!(observed.ty, integer);
            let origins = observed.dependency_origins();
            assert!(
                origins
                    .iter()
                    .any(|origin| origin.application == Type::unknown())
            );
            assert!(
                origins
                    .iter()
                    .any(|origin| origin.application == Type::object())
            );
        }
    }

    #[test]
    fn observed_union_retains_an_absorbed_literal() {
        let db = setup_db();
        let env = db.program_environment();
        let integer = KnownClass::Int.to_instance(&db, &env);
        for types in [
            [integer, Type::int_literal(1)],
            [Type::int_literal(1), integer],
        ] {
            let mut builder =
                UnionBuilder::new(&db, &env).with_observed_context(RelationContext::default());
            builder.add_observed_in_place(
                ObservedType::root(Type::unknown()).unchanged_or_unresolved(types[0]),
            );
            builder.add_observed_in_place(
                ObservedType::root(Type::object()).unchanged_or_unresolved(types[1]),
            );
            let observed = builder.build_observed().recipe().observe();
            assert_eq!(observed.ty, integer);
            let origins = observed.dependency_origins();
            assert!(
                origins
                    .iter()
                    .any(|origin| origin.application == Type::unknown())
            );
            assert!(
                origins
                    .iter()
                    .any(|origin| origin.application == Type::object())
            );
        }
    }

    #[test]
    fn observed_intersection_retains_absorbed_operands() {
        let db = setup_db();
        let env = db.program_environment();
        let integer = KnownClass::Int.to_instance(&db, &env);
        let boolean = KnownClass::Bool.to_instance(&db, &env);
        for types in [[integer, boolean], [boolean, integer]] {
            let observed = IntersectionBuilder::bounded_from_observed_elements(
                &db,
                &env,
                [
                    ObservedType::root(Type::unknown()).unchanged_or_unresolved(types[0]),
                    ObservedType::root(Type::object()).unchanged_or_unresolved(types[1]),
                ],
                TypeNormalization::Semantic,
                Some(RelationContext::default()),
            )
            .unwrap()
            .recipe()
            .observe();
            assert_eq!(observed.ty, boolean);
            let origins = observed.dependency_origins();
            assert!(
                origins
                    .iter()
                    .any(|origin| origin.application == Type::unknown())
            );
            assert!(
                origins
                    .iter()
                    .any(|origin| origin.application == Type::object())
            );
        }
    }

    #[test]
    fn build_union_single_element() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let t0 = Type::int_literal(0);
        let union = UnionType::from_elements(db, &env, [t0]);
        assert_eq!(union, t0);
    }

    #[test]
    fn build_union_two_elements() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let t0 = Type::int_literal(0);
        let t1 = Type::int_literal(1);
        let union = UnionType::from_elements(db, &env, [t0, t1]).expect_union();

        assert_eq!(union.elements(db), &[t0, t1]);
    }

    #[test]
    fn structural_union_preserves_recursive_literals() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let literal_count = MAX_RECURSIVE_UNION_LITERALS + 1;
        let literals = (0..literal_count)
            .map(|value| Type::int_literal(i64::try_from(value).expect("literal fits in i64")));
        let original = literals
            .fold(
                UnionBuilder::new(db, &env)
                    .normalization(TypeNormalization::Structural)
                    .or_recursively_defined(RecursivelyDefined::Yes),
                UnionBuilder::add,
            )
            .build();
        let flattened = UnionBuilder::new(db, &env)
            .normalization(TypeNormalization::Structural)
            .add(original)
            .add(Type::int_literal(0))
            .build()
            .expect_union();

        assert_eq!(flattened.elements(db).len(), literal_count);
        assert!(flattened.recursively_defined(db).is_yes());
        assert!(flattened.elements(db).iter().all(|element| {
            matches!(element, Type::LiteralValue(literal) if literal.recursively_defined().is_yes())
        }));
    }

    #[test]
    fn structural_construction_restores_constants_and_duplicates() {
        let db = setup_db();
        let env = db.program_environment();
        let union = || UnionBuilder::new(&db, &env).normalization(TypeNormalization::Structural);
        let intersection =
            || IntersectionBuilder::new(&db, &env).normalization(TypeNormalization::Structural);
        let atom = Type::any();
        let nested = Type::Union(UnionType::new(
            &db,
            vec![Type::Never, atom, atom].into_boxed_slice(),
            RecursivelyDefined::No,
        ));

        assert_eq!(union().build(), Type::Never);
        assert_eq!(union().add(nested).add(atom).build(), atom);
        assert_eq!(
            nested.expect_union().map_leave_aliases_with_normalization(
                &db,
                &env,
                TypeNormalization::Structural,
                |ty| *ty,
            ),
            atom,
        );
        assert_eq!(intersection().build(), Type::object());
        assert_eq!(intersection().add_positive(nested).build(), atom);
        assert_eq!(
            intersection().add_positive(atom).add_negative(atom).build(),
            atom,
        );
        for (left, right) in [(atom, Type::object()), (Type::object(), atom)] {
            assert_eq!(union().add(left).add(right).build(), Type::object());
            assert_eq!(
                intersection().positive_elements([left, right]).build(),
                atom
            );
        }
        for (left, right) in [(atom, Type::Never), (Type::Never, atom)] {
            assert_eq!(union().add(left).add(right).build(), atom);
            assert_eq!(
                intersection().positive_elements([left, right]).build(),
                Type::Never,
            );
        }
        assert_eq!(
            intersection().add_negative(Type::Never).build(),
            Type::object()
        );
        assert_eq!(
            intersection().add_negative(Type::object()).build(),
            Type::Never
        );

        let literal = LiteralValueType::unpromotable(1_i64);
        let recursive_literal = literal.with_recursively_defined(RecursivelyDefined::Yes);
        let recursive_union = Type::Union(UnionType::new(
            &db,
            vec![
                Type::LiteralValue(literal),
                Type::LiteralValue(recursive_literal),
            ]
            .into_boxed_slice(),
            RecursivelyDefined::Yes,
        ));
        assert_eq!(
            union().add(recursive_union).build(),
            Type::LiteralValue(recursive_literal)
        );
    }

    #[test]
    fn structural_construction_distributes_without_comparing_atoms() {
        let db = setup_db();
        let env = db.program_environment();
        let union = || UnionBuilder::new(&db, &env).normalization(TypeNormalization::Structural);
        let intersection =
            || IntersectionBuilder::new(&db, &env).normalization(TypeNormalization::Structural);
        let [a, b, c] = [1, 2, 3].map(Type::int_literal);

        // `a & (b | c)` retains both branches even though semantic comparison could
        // prove these particular atoms disjoint.
        let distributed = intersection()
            .add_positive(a)
            .add_positive(union().add(b).add(c).build())
            .build();
        let ab = intersection().positive_elements([a, b]).build();
        let ac = intersection().positive_elements([a, c]).build();
        assert_eq!(distributed.expect_union().elements(&db), &[ab, ac]);

        let negated = intersection()
            .add_positive(a)
            .add_negative(intersection().add_positive(b).add_negative(c).build())
            .build();
        let a_not_b = intersection().add_positive(a).add_negative(b).build();
        assert_eq!(negated.expect_union().elements(&db), &[a_not_b, ac]);

        let disjunction = union().add(a).add(b).build();
        let double_negation = intersection()
            .add_negative(intersection().add_negative(disjunction).build())
            .build();
        assert_eq!(double_negation, disjunction);
    }

    #[test]
    fn semantic_observation_normalizes_structural_unions() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let union = UnionBuilder::new(&db, &env)
            .normalization(TypeNormalization::Structural)
            .add(Type::int_literal(1))
            .add(int)
            .build();
        assert!(union.is_union());
        let tuple = Type::heterogeneous_tuple(&db, &env, [union]);
        assert_eq!(
            tuple.apply_type_mapping(&db, &env, &TypeMapping::Normalize, TypeContext::default()),
            Type::heterogeneous_tuple(&db, &env, [int]),
        );
    }

    #[test]
    fn structural_construction_preserves_gradual_complements_without_queries() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "\
            type Alias = int
            Recursive = tuple['Recursive']
            x: Recursive
            ",
        )
        .unwrap();
        let env = db.program_environment();
        let file = ruff_db::files::system_path_to_file(&db, "/src/a.py").unwrap();
        let module = ProgramFile::new(&db, file, env.program(&db));
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
            global_symbol(&db, module, "Alias").place.expect_type()
        else {
            panic!("Expected a type alias");
        };
        let recursive = global_symbol(&db, module, "x").place.expect_type();
        assert!(matches!(recursive, Type::Recursive(_)));
        let tuple = Type::heterogeneous_tuple(&db, &env, [Type::any()]);
        let mut events_db = db.clone();
        events_db.clear_salsa_events();

        for atom in [tuple, Type::TypeAlias(alias), recursive] {
            // Identity is enough to remove duplicates of the same sign. It is not enough
            // to remove opposite signs: an alias or a container can include gradual types.
            let both = IntersectionBuilder::new(&db, &env)
                .normalization(TypeNormalization::Structural)
                .add_positive(atom)
                .add_positive(atom)
                .add_negative(atom)
                .add_negative(atom)
                .build();
            let Type::Intersection(both) = both else {
                panic!("Expected both signs of the same atom to be preserved");
            };
            assert_eq!(
                both.positive(&db).iter().copied().collect::<Vec<_>>(),
                [atom]
            );
            assert_eq!(
                both.negative(&db).iter().copied().collect::<Vec<_>>(),
                [atom]
            );
            let union = UnionBuilder::new(&db, &env)
                .normalization(TypeNormalization::Structural)
                .add(Type::Intersection(both))
                .add(atom)
                .build();
            assert_eq!(
                union.expect_union().elements(&db),
                &[Type::Intersection(both), atom]
            );
        }
        assert!(
            events_db
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }

    #[test]
    fn cycle_recovery_widens_recursive_literal_union() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let literal_limit =
            i64::try_from(MAX_RECURSIVE_UNION_LITERALS).expect("literal limit fits in i64");

        let union = (0..=literal_limit).map(Type::int_literal).fold(
            UnionBuilder::new(db, &env)
                .cycle_recovery(true)
                .or_recursively_defined(RecursivelyDefined::Yes),
            UnionBuilder::add,
        );

        assert_eq!(union.build(), KnownClass::Int.to_instance(db, &env));

        let assert_widens = |literal, instance| {
            for (first, second) in [(literal, instance), (instance, literal)] {
                let union = UnionBuilder::new(db, &env)
                    .cycle_recovery(true)
                    .add(first)
                    .add(second)
                    .build();
                assert_eq!(union, instance);
            }
        };

        assert_widens(Type::int_literal(1), KnownClass::Int.to_instance(db, &env));
        assert_widens(
            Type::string_literal(db, "literal"),
            KnownClass::Str.to_instance(db, &env),
        );
        assert_widens(
            Type::bytes_literal(db, b"literal"),
            KnownClass::Bytes.to_instance(db, &env),
        );

        let safe_uuid_class = known_module_symbol(db, &env, KnownModule::Uuid, "SafeUUID")
            .place
            .expect_type()
            .expect_class_literal();
        let enum_literal = enum_member_literals(db, safe_uuid_class, None)
            .expect("SafeUUID is an enum")
            .next()
            .expect("SafeUUID has members");
        assert_widens(
            enum_literal,
            enum_literal
                .expect_enum_literal()
                .enum_class_instance(db, &env),
        );
    }

    #[test]
    fn cycle_recovery_skips_other_redundancy_simplification() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        for (left, right) in [
            (Type::string_literal(db, "literal"), Type::literal_string()),
            (
                Type::bool_literal(true),
                KnownClass::Bool.to_instance(db, &env),
            ),
            (Type::int_literal(1), Type::object()),
            (Type::bool_literal(true), Type::bool_literal(false)),
        ] {
            for (first, second) in [(left, right), (right, left)] {
                let union = UnionBuilder::new(db, &env)
                    .cycle_recovery(true)
                    .add(first)
                    .add(second)
                    .build()
                    .expect_union();
                assert!(union.elements(db).contains(&left));
                assert!(union.elements(db).contains(&right));
            }
        }
    }

    #[test]
    fn cycle_recovery_preserves_same_length_tuples() {
        let db = setup_db();
        let env = db.program_environment();
        let first = Type::heterogeneous_tuple(&db, &env, [Type::int_literal(1)]);
        let second = Type::heterogeneous_tuple(&db, &env, [Type::int_literal(2)]);

        let union = UnionType::from_elements_cycle_recovery(&db, &env, [first, second]);
        assert_eq!(union.expect_union().elements(&db), &[first, second]);
        assert_eq!(
            UnionType::widen_growing_tuples(&db, &env, first, second),
            None
        );
    }

    #[test]
    fn cycle_recovery_widens_growing_tuple_lengths() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let first = Type::heterogeneous_tuple(&db, &env, [int]);
        let second = Type::heterogeneous_tuple(&db, &env, [int, int]);
        let widened = Type::homogeneous_tuple(&db, &env, int);

        for (left, right) in [(first, second), (second, first)] {
            assert_eq!(
                UnionType::widen_growing_tuples(&db, &env, left, right),
                Some(widened),
            );
        }

        // Appending to the widened result adds a fixed suffix, which recovery must absorb.
        let appended = Type::tuple(TupleType::mixed(&db, &env, [], int, [int]));
        let other = Type::bool_literal(true);
        let previous = UnionType::from_elements_cycle_recovery(&db, &env, [other, widened]);
        let current = UnionType::from_elements_cycle_recovery(&db, &env, [previous, appended]);
        let union = UnionType::widen_growing_tuples(&db, &env, previous, current).unwrap();
        assert_eq!(union.expect_union().elements(&db), &[other, widened]);

        // Initial cycle iterations can discard unrelated alternatives from the previous result.
        assert_eq!(
            UnionType::widen_growing_tuples(&db, &env, previous, appended),
            Some(widened)
        );
    }

    #[test]
    fn cycle_recovery_widens_tuples_without_relation_queries() {
        let db = setup_db();
        let mut events_db = db.clone();
        let env = db.program_environment();
        let literal = LiteralValueType::promotable(1_i64);
        let first = Type::heterogeneous_tuple(&db, &env, [Type::from(literal)]);
        let second = Type::heterogeneous_tuple(&db, &env, [Type::from(literal), Type::object()]);
        events_db.clear_salsa_events();

        let result = UnionType::widen_growing_tuples(&db, &env, first, second).unwrap();
        let tuple = result.exact_tuple_instance_spec(&db).unwrap();
        let elements = tuple.variable_element_type(&db).unwrap().expect_union();
        assert_eq!(
            elements.elements(&db),
            &[
                Type::from(literal.with_recursively_defined(RecursivelyDefined::Yes)),
                Type::object(),
            ]
        );
        assert!(
            events_db
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }

    #[test]
    fn cycle_recovery_widens_never_tuples_to_an_upper_bound() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let first = Type::heterogeneous_tuple(&db, &env, [Type::Never]);

        for (last_element, widened_element) in [(Type::Never, Type::object()), (int, int)] {
            let second = Type::heterogeneous_tuple(&db, &env, [Type::Never, last_element]);
            let result = UnionType::widen_growing_tuples(&db, &env, first, second).unwrap();

            assert_eq!(result, Type::homogeneous_tuple(&db, &env, widened_element));
            assert!(first.is_subtype_of(&db, &env, result));
            assert!(second.is_subtype_of(&db, &env, result));
        }
    }

    #[test]
    fn union_common_literal_supertype() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let str_union = UnionType::from_elements(
            db,
            &env,
            [Type::string_literal(db, "a"), Type::string_literal(db, "b")],
        )
        .expect_union();
        assert_eq!(
            str_union.common_literal_supertype(db, &env),
            Some(Type::literal_string())
        );

        let int_union =
            UnionType::from_elements(db, &env, [Type::int_literal(1), Type::int_literal(2)])
                .expect_union();
        assert_eq!(
            int_union.common_literal_supertype(db, &env),
            Some(KnownClass::Int.to_instance(db, &env))
        );

        let mixed_union = UnionType::from_elements(
            db,
            &env,
            [Type::string_literal(db, "a"), Type::int_literal(1)],
        )
        .expect_union();
        assert_eq!(mixed_union.common_literal_supertype(db, &env), None);
    }

    fn map_marker<'db>(ty: &Type<'db>, marker: Type<'db>, replacement: Type<'db>) -> Type<'db> {
        if *ty == marker { replacement } else { *ty }
    }

    #[test]
    fn map_rebuilds_prefix_for_literal_widening() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let marker = KnownClass::Str.to_instance(db, &env);
        let literal_limit =
            i64::try_from(MAX_NON_RECURSIVE_UNION_LITERALS).expect("literal limit fits in i64");
        let widening_literal = Type::int_literal(literal_limit);
        let expected = KnownClass::Int.to_instance(db, &env);

        let elements = (0..literal_limit).map(Type::int_literal).chain([marker]);
        let union = UnionType::from_elements(db, &env, elements).expect_union();

        assert_eq!(
            union.map(db, &env, |ty| map_marker(ty, marker, widening_literal)),
            expected
        );
        assert_eq!(
            union.map_leave_aliases(db, &env, |ty| map_marker(ty, marker, widening_literal)),
            expected
        );
        assert_eq!(
            union.try_map(db, &env, |ty| Some(map_marker(
                ty,
                marker,
                widening_literal
            ))),
            Some(expected)
        );
    }

    #[test]
    fn map_preserves_alias_unpacking_behavior() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "type Alias = int").unwrap();
        let env = db.program_environment();

        let module = ruff_db::files::system_path_to_file(&db, "/src/a.py").unwrap();
        let module = ProgramFile::new(&db, module, db.program_environment().program(&db));
        let alias_ty = global_symbol(&db, module, "Alias").place.expect_type();
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(alias))) =
            alias_ty
        else {
            panic!("Expected `Alias` to be a type alias");
        };

        let alias = Type::TypeAlias(TypeAliasType::PEP695(alias));
        let str_instance = KnownClass::Str.to_instance(&db, &env);
        let union_ty = UnionType::from_elements_leave_aliases(&db, &env, [alias, str_instance]);
        let union = union_ty.expect_union();
        let unpacked = UnionType::from_elements(
            &db,
            &env,
            [KnownClass::Int.to_instance(&db, &env), str_instance],
        );

        assert_eq!(union.map(&db, &env, |ty| *ty), unpacked);
        assert_eq!(union.try_map(&db, &env, |ty| Some(*ty)), Some(unpacked));
        assert_eq!(union.map_leave_aliases(&db, &env, |ty| *ty), union_ty);
    }

    #[test]
    fn build_intersection_empty_intersection_equals_object() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let intersection = IntersectionBuilder::new(db, &env).build();
        assert_eq!(intersection, Type::object());
    }

    #[test]
    fn literal_intersection_simplification_matches_relations() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let literals: Vec<_> = [
            LiteralValueTypeKind::from(0),
            LiteralValueTypeKind::from(1),
            LiteralValueTypeKind::Bool(false),
            LiteralValueTypeKind::Bool(true),
            LiteralValueTypeKind::String(StringLiteralType::new(db, "a")),
            LiteralValueTypeKind::String(StringLiteralType::new(db, "b")),
            LiteralValueTypeKind::Bytes(BytesLiteralType::new(db, b"a".as_slice())),
            LiteralValueTypeKind::Bytes(BytesLiteralType::new(db, b"b".as_slice())),
        ]
        .into_iter()
        .flat_map(|kind| {
            [false, true].into_iter().flat_map(move |promotable| {
                [RecursivelyDefined::No, RecursivelyDefined::Yes]
                    .into_iter()
                    .map(move |recursive| {
                        Type::LiteralValue(
                            LiteralValueType::new(kind, promotable)
                                .with_recursively_defined(recursive),
                        )
                    })
            })
        })
        .collect();

        for &first in &literals {
            for &second in &literals {
                for polarity in [
                    IntersectionPolarity::Positive,
                    IntersectionPolarity::Negative,
                    IntersectionPolarity::Mixed,
                ] {
                    assert_eq!(
                        simplify_intersection_pair(db, &env, first, second, polarity),
                        simplify_intersection_pair_impl(
                            db,
                            TypePair::new(db, env.program(db), first, second),
                            polarity,
                        ),
                        "{first:?}, {second:?}, {polarity:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn build_intersection_discards_never_dnf_branches() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let bytes = KnownClass::Bytes.to_instance(db, &env);

        let int_or_str = UnionType::from_elements(db, &env, [int, str]);
        let int_or_bytes = UnionType::from_elements(db, &env, [int, bytes]);
        let intersection = IntersectionBuilder::new(db, &env)
            .add_positive(int_or_str)
            .add_positive(int_or_bytes);

        assert_eq!(intersection.intersections.len(), 1);
        assert_eq!(intersection.build(), int);
    }

    #[test]
    fn build_intersection_deduplicates_dnf_branches() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let callable = Type::single_callable(db, Signature::dynamic(Type::object()));
        let intersection = IntersectionBuilder::new(db, &env)
            .add_positive(callable)
            .add_negative(callable)
            .build();
        let negated = intersection.negate(db, &env);

        let mut negative_builder = IntersectionBuilder::new(db, &env);
        let mut positive_builder = IntersectionBuilder::new(db, &env);
        for _ in 0..8 {
            negative_builder.add_negative_in_place(intersection);
            positive_builder.add_positive_in_place(negated);
        }

        // A gradual callable C can overlap its negation, so distribution retains C & ~C
        // alongside C and ~C. Repeating the same clause must not multiply these alternatives.
        assert!(
            negative_builder.intersections.len() <= 3,
            "{:?}",
            negative_builder.intersections,
        );
        assert!(
            positive_builder.intersections.len() <= 3,
            "{:?}",
            positive_builder.intersections,
        );

        assert!(negative_builder.build().is_equivalent_to(db, &env, negated));
        assert!(positive_builder.build().is_equivalent_to(db, &env, negated));
    }

    #[test]
    fn build_intersection_simplify_split_bool() {
        let db = setup_db();

        build_intersection_simplify_split_bool_impl(&db, Type::bool_literal(true));
        build_intersection_simplify_split_bool_impl(&db, Type::bool_literal(false));
        build_intersection_simplify_split_bool_impl(&db, Type::AlwaysTruthy);
        build_intersection_simplify_split_bool_impl(&db, Type::AlwaysFalsy);
    }

    fn build_intersection_simplify_split_bool_impl(db: &TestDb, t_splitter: Type) {
        let env = db.program_environment();
        let bool_value = t_splitter.bool(db, &env) == Truthiness::AlwaysTrue;

        // We add t_object in various orders (in first or second position) in
        // the tests below to ensure that the boolean simplification eliminates
        // everything from the intersection, not just `bool`.
        let t_object = Type::object();
        let t_bool = KnownClass::Bool.to_instance(db, &env);

        let ty = IntersectionBuilder::new(db, &env)
            .add_positive(t_object)
            .add_positive(t_bool)
            .add_negative(t_splitter)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));

        let ty = IntersectionBuilder::new(db, &env)
            .add_positive(t_bool)
            .add_positive(t_object)
            .add_negative(t_splitter)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));

        let ty = IntersectionBuilder::new(db, &env)
            .add_positive(t_object)
            .add_negative(t_splitter)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));

        let ty = IntersectionBuilder::new(db, &env)
            .add_negative(t_splitter)
            .add_positive(t_object)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));
    }

    #[test]
    fn build_intersection_enums() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let safe_uuid_class = known_module_symbol(db, &env, KnownModule::Uuid, "SafeUUID")
            .place
            .ignore_possibly_undefined()
            .unwrap();

        let literals = enum_member_literals(db, safe_uuid_class.expect_class_literal(), None)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(literals.len(), 3);

        // SafeUUID.safe
        let l_safe = literals[0];
        assert_eq!(l_safe.expect_enum_literal().name(db), "safe");
        // SafeUUID.unsafe
        let l_unsafe = literals[1];
        assert_eq!(l_unsafe.expect_enum_literal().name(db), "unsafe");
        // SafeUUID.unknown
        let l_unknown = literals[2];
        assert_eq!(l_unknown.expect_enum_literal().name(db), "unknown");

        // The enum itself: SafeUUID
        let safe_uuid = l_safe.expect_enum_literal().enum_class_instance(db, &env);

        {
            let actual = IntersectionBuilder::new(db, &env)
                .add_positive(safe_uuid)
                .add_negative(l_safe)
                .build();

            assert_eq!(
                actual.display(db, &db.program_environment()).to_string(),
                "Literal[SafeUUID.unsafe, SafeUUID.unknown]"
            );
        }
        {
            // Same as above, but with the order reversed
            let actual = IntersectionBuilder::new(db, &env)
                .add_negative(l_safe)
                .add_positive(safe_uuid)
                .build();

            assert_eq!(
                actual.display(db, &db.program_environment()).to_string(),
                "Literal[SafeUUID.unsafe, SafeUUID.unknown]"
            );
        }
        {
            // Also the same, but now with a nested intersection
            let actual = IntersectionBuilder::new(db, &env)
                .add_positive(safe_uuid)
                .add_positive(
                    IntersectionBuilder::new(db, &env)
                        .add_negative(l_safe)
                        .build(),
                )
                .build();

            assert_eq!(
                actual.display(db, &db.program_environment()).to_string(),
                "Literal[SafeUUID.unsafe, SafeUUID.unknown]"
            );
        }
        {
            let actual = IntersectionBuilder::new(db, &env)
                .add_negative(l_safe)
                .add_positive(safe_uuid)
                .add_negative(l_unsafe)
                .build();

            assert_eq!(
                actual.display(db, &db.program_environment()).to_string(),
                "Literal[SafeUUID.unknown]"
            );
        }
    }
}
