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

use super::RecursivelyDefined;
use crate::types::cyclic::{MAX_RECURSIVE_TYPE_ALIAS_UNFOLDS, TypeIdentity};
use crate::types::enums::EnumComplement;
use crate::types::set_theoretic::expand_intersection_typevars_and_newtypes;
use crate::types::{
    BytesLiteralType, ClassLiteral, EnumLiteralType, IntersectionType, KnownClass,
    KnownInstanceType, LiteralValueType, LiteralValueTypeKind, NegativeIntersectionElements,
    StringLiteralType, SubclassOfType, Type, TypeVarBoundOrConstraints, TypeVarVariance, UnionType,
};
use crate::{Db, FxOrderMap, FxOrderSet};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

/// Controls whether a set-theoretic type builder may ask semantic type-relation questions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NormalizationMode {
    /// Apply both structural and relation-based simplifications.
    #[default]
    RelationAware,
    /// Apply structural simplifications without starting a type-relation query.
    Structural,
}

impl NormalizationMode {
    const fn uses_relations(self) -> bool {
        matches!(self, Self::RelationAware)
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
    ty: Type<'db>,
) -> Option<(Type<'db>, Type<'db>)> {
    let Type::Intersection(intersection) = ty else {
        return None;
    };
    let falsy = Type::AlwaysTruthy.negate(db);
    let truthy = Type::AlwaysFalsy.negate(db);

    let negative = intersection.negative(db);
    let has_not_truthy = negative.contains(&Type::AlwaysTruthy);
    let has_not_falsy = negative.contains(&Type::AlwaysFalsy);
    let guard = match (has_not_truthy, has_not_falsy) {
        (true, false) => falsy,
        (false, true) => truthy,
        _ => return None,
    };

    let mut core = IntersectionBuilder::new(db);
    for positive in intersection.positive(db) {
        core = core.add_positive(*positive);
    }
    for negative in negative {
        if (guard == falsy && *negative == Type::AlwaysTruthy)
            || (guard == truthy && *negative == Type::AlwaysFalsy)
        {
            continue;
        }
        core = core.add_negative(*negative);
    }
    Some((core.build(), guard))
}

/// Return `true` if `general` and `specific` are specializations of the same generic class and
/// `general` only differs by using dynamic types for invariant type variables. For example,
/// `list[Any]` is an invariant-dynamic generalization of `list[int]`.
fn is_invariant_dynamic_generalization_of<'db>(
    db: &'db dyn Db,
    general: Type<'db>,
    specific: Type<'db>,
) -> bool {
    // Fast path to avoid performance regressions.
    if !general.has_dynamic(db) {
        return false;
    }

    if matches!(general, Type::TypeVar(_) | Type::NewTypeInstance(_)) {
        return false;
    }

    let (
        Some((general_class, general_specialization)),
        Some((specific_class, specific_specialization)),
    ) = (
        general.class_specialization(db),
        specific.class_specialization(db),
    )
    else {
        return false;
    };

    // Top and bottom materializations are not gradual types.
    if general_class != specific_class
        || general_specialization.materialization_kind(db).is_some()
        || specific_specialization.materialization_kind(db).is_some()
    {
        return false;
    }

    let mut has_dynamic_replacement = false;
    for ((typevar, general_type), specific_type) in general_specialization
        .generic_context(db)
        .variables(db)
        .zip(general_specialization.types(db))
        .zip(specific_specialization.types(db))
    {
        if general_type == specific_type {
            continue;
        }
        if general_type.is_non_divergent_dynamic()
            && typevar.variance(db) == TypeVarVariance::Invariant
        {
            has_dynamic_replacement = true;
            continue;
        }
        return false;
    }
    has_dynamic_replacement
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
    left: Type<'db>,
    right: Type<'db>,
) -> Option<Type<'db>> {
    let (left_core, left_guard) = split_truthiness_guarded_intersection(db, left)?;
    let (right_core, right_guard) = split_truthiness_guarded_intersection(db, right)?;
    if left_guard == right_guard {
        return None;
    }

    if left_core.is_equivalent_to(db, right_core) {
        return Some(left_core);
    }

    let candidate = UnionType::from_elements(db, [left_core, right_core]);
    let left_reconstructed = IntersectionType::from_two_elements(db, candidate, left_guard);
    let right_reconstructed = IntersectionType::from_two_elements(db, candidate, right_guard);
    if left_reconstructed == left && right_reconstructed == right {
        Some(candidate)
    } else {
        None
    }
}

/// Return `true` if union simplification should preserve this pair because one element is
/// `Hashable` and the other is a non-final nominal instance.
///
/// Hashability does not obey normal inheritance rules: subclasses of hashable classes can be
/// unhashable. Keeping the non-final type allows downstream checks to consider it independently.
fn should_preserve_hashable_union(db: &dyn Db, left: Type, right: Type) -> bool {
    let is_hashable =
        |ty| matches!(ty, Type::ProtocolInstance(protocol) if protocol.is_hashable(db));
    let is_non_final_nominal_instance =
        |ty| matches!(ty, Type::NominalInstance(instance) if !instance.class(db).is_final(db));

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
fn normalize_enum_complement_unions<'db>(db: &'db dyn Db, types: &mut Vec<Type<'db>>) -> bool {
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
            let mut builder = IntersectionBuilder::structural(db)
                .add_positive(enum_class.to_non_generic_instance(db))
                .positive_elements(complement.rest(db).iter().copied());
            for name in enum_class_literal
                .member_names(db)
                .filter(|name| shared_excluded_names.contains(*name))
            {
                builder = builder.add_negative(Type::enum_literal(EnumLiteralType::new(
                    db,
                    enum_class_literal,
                    name,
                )));
            }
            types[complement_index] = builder.build();

            remove_indices.sort_unstable();
            for index in remove_indices.into_iter().rev() {
                types.swap_remove(index);
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

#[derive(Clone, Debug)]
enum UnionElement<'db> {
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

impl<'db> UnionElement<'db> {
    fn type_count(&self) -> usize {
        match self {
            UnionElement::Type(_) => 1,
            UnionElement::IntLiterals(literals) => literals.len(),
            UnionElement::StringLiterals(literals) => literals.len(),
            UnionElement::BytesLiterals(literals) => literals.len(),
            UnionElement::EnumLiterals { literals, .. } => literals.len(),
        }
    }

    /// Try reducing this `UnionElement` given the presence in the same union of `other_type`.
    fn try_reduce(
        &mut self,
        db: &'db dyn Db,
        other_type: Type<'db>,
        cycle_recovery: bool,
        allow_relation_simplification: bool,
    ) -> ReduceResult<'db> {
        if cycle_recovery {
            // A widened literal group must absorb matching literals from later iterations for
            // recovery to converge. Preserve that exact fallback reduction without relation queries.
            return match self {
                UnionElement::Type(existing) => ReduceResult::Type(*existing),
                UnionElement::IntLiterals(_) => {
                    ReduceResult::KeepIf(!other_type.is_instance_of(db, KnownClass::Int))
                }
                UnionElement::StringLiterals(_) => {
                    ReduceResult::KeepIf(!other_type.is_instance_of(db, KnownClass::Str))
                }
                UnionElement::BytesLiterals(_) => {
                    ReduceResult::KeepIf(!other_type.is_instance_of(db, KnownClass::Bytes))
                }
                UnionElement::EnumLiterals { enum_class, .. } => ReduceResult::KeepIf(
                    other_type
                        .as_nominal_instance()
                        .is_none_or(|instance| instance.class_literal(db) != *enum_class),
                ),
            };
        }

        if !allow_relation_simplification {
            return match self {
                UnionElement::Type(existing) => ReduceResult::Type(*existing),
                UnionElement::IntLiterals(_)
                | UnionElement::StringLiterals(_)
                | UnionElement::BytesLiterals(_)
                | UnionElement::EnumLiterals { .. } => ReduceResult::KeepIf(true),
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
            if ignore || other_type.is_redundant_with(db, ty) {
                ignore = true;
                return true;
            }
            if collapse
                || other_type.negation_is_subtype_of_cached(db, ty, &mut other_type_negated_cache)
            {
                collapse = true;
                return true;
            }
            !ty.is_redundant_with(db, other_type)
        };

        let should_keep = match self {
            UnionElement::IntLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::Int) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !Type::from(LiteralValueType::new(*literal, *promotable))
                        .is_redundant_with(db, other_type)
                }
            }
            UnionElement::StringLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::String) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !Type::from(LiteralValueType::new(*literal, *promotable))
                        .is_redundant_with(db, other_type)
                }
            }
            UnionElement::BytesLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::Bytes) {
                    literals.retain(|literal, promotable| {
                        should_retain_type(LiteralValueType::new(*literal, *promotable).into())
                    });
                    !literals.is_empty()
                } else {
                    let (literal, promotable) = literals.first().unwrap();
                    !Type::from(LiteralValueType::new(*literal, *promotable))
                        .is_redundant_with(db, other_type)
                }
            }
            UnionElement::EnumLiterals {
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
                    !Type::from(LiteralValueType::new(*literal, *promotable))
                        .is_redundant_with(db, other_type)
                }
            }
            UnionElement::Type(existing) => return ReduceResult::Type(*existing),
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
    /// Reduction of this `UnionElement` is complete; keep it in the union if the nested
    /// boolean is true, eliminate it from the union if false.
    KeepIf(bool),
    /// Collapse this entire union to `object`.
    CollapseToObject,
    /// The new element is a subtype of an existing part of the `UnionElement`, ignore it.
    Ignore,
    /// The given `Type` can stand-in for the entire `UnionElement` for further union
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
/// Active expansions of specialized recursive aliases and the normalized union that existed
/// immediately before each expansion.
#[derive(Default)]
struct ActiveRecursiveAliasExpansions<'db> {
    entries: Vec<RecursiveAliasExpansion<'db>>,
}

struct RecursiveAliasExpansion<'db> {
    alias: Type<'db>,
    union_before_expansion: Type<'db>,
}

impl<'db> ActiveRecursiveAliasExpansions<'db> {
    /// Returns `true` if the latest expansion cycle for `alias` left the union unchanged after an
    /// earlier cycle of the same alias grew it.
    fn is_at_fixed_point(&self, alias: Type<'db>, current_union: Type<'db>) -> bool {
        let mut previous_expansions = self
            .entries
            .iter()
            .rev()
            .filter(|expansion| expansion.alias == alias);
        previous_expansions.next().is_some_and(|expansion| {
            expansion.union_before_expansion == current_union
                && previous_expansions.any(|earlier_expansion| {
                    earlier_expansion.union_before_expansion != current_union
                })
        })
    }

    fn enter(&mut self, alias: Type<'db>, union_before_expansion: Type<'db>) {
        self.entries.push(RecursiveAliasExpansion {
            alias,
            union_before_expansion,
        });
    }

    fn exit(&mut self, alias: Type<'db>) {
        let exited = self.entries.pop();
        debug_assert_eq!(exited.map(|expansion| expansion.alias), Some(alias));
    }
}

pub(crate) struct UnionBuilder<'db> {
    elements: Vec<UnionElement<'db>>,
    recursive_alias_remainder: Option<Type<'db>>,
    db: &'db dyn Db,
    unpack_aliases: bool,
    /// This is enabled when joining types in a `cycle_recovery` function. Because recovery cannot
    /// introduce a new cycle, relation-based union simplifications are skipped in this mode.
    cycle_recovery: bool,
    normalization: NormalizationMode,
    recursively_defined: RecursivelyDefined,
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

    pub(crate) fn add(&mut self, db: &'db dyn Db, ty: Type<'db>) {
        match self {
            UnionAccumulator::One(existing) => {
                *self = UnionAccumulator::Two(*existing, ty);
            }
            UnionAccumulator::Two(first, second) => {
                let mut builder = UnionBuilder::new(db);
                builder.add_in_place(*first);
                builder.add_in_place(*second);
                builder.add_in_place(ty);
                *self = UnionAccumulator::Deferred(builder);
            }
            UnionAccumulator::Deferred(builder) => builder.add_in_place(ty),
        }
    }

    pub(crate) fn get_or_build(&mut self, db: &'db dyn Db) -> Type<'db> {
        match self {
            UnionAccumulator::One(ty) => *ty,
            UnionAccumulator::Two(first, second) => {
                let ty = UnionType::from_two_elements(db, *first, *second);
                *self = UnionAccumulator::One(ty);
                ty
            }
            UnionAccumulator::Deferred(_) => {
                let ty = std::mem::replace(self, UnionAccumulator::new(Type::Never)).into_type(db);
                *self = UnionAccumulator::new(ty);
                ty
            }
        }
    }

    pub(crate) fn into_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            UnionAccumulator::One(ty) => ty,
            UnionAccumulator::Two(first, second) => UnionType::from_two_elements(db, first, second),
            UnionAccumulator::Deferred(builder) => builder.build(),
        }
    }
}

impl<'db> UnionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self::with_normalization(db, NormalizationMode::default())
    }

    fn with_normalization(db: &'db dyn Db, normalization: NormalizationMode) -> Self {
        Self {
            db,
            elements: vec![],
            recursive_alias_remainder: None,
            unpack_aliases: true,
            cycle_recovery: false,
            normalization,
            recursively_defined: RecursivelyDefined::No,
        }
    }

    /// Creates a union builder that never starts a type-relation query.
    pub(crate) fn structural(db: &'db dyn Db) -> Self {
        Self::with_normalization(db, NormalizationMode::Structural)
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

    pub(crate) fn recursively_defined(mut self, val: RecursivelyDefined) -> Self {
        self.recursively_defined = val;
        self
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Collapse the union to a single type: `object`.
    fn collapse_to_object(&mut self) {
        self.elements.clear();
        self.elements.push(UnionElement::Type(Type::object()));
    }

    fn widen_literal_types(&mut self, seen_aliases: &mut Vec<TypeIdentity<'db>>) {
        let mut replace_with = vec![];
        for elem in &self.elements {
            match elem {
                UnionElement::IntLiterals(_) => {
                    replace_with.push(KnownClass::Int.to_instance(self.db));
                }
                UnionElement::StringLiterals(_) => {
                    replace_with.push(KnownClass::Str.to_instance(self.db));
                }
                UnionElement::BytesLiterals(_) => {
                    replace_with.push(KnownClass::Bytes.to_instance(self.db));
                }
                UnionElement::EnumLiterals { literals, .. } => {
                    let (enum_literal, _) = literals.first().unwrap();
                    replace_with.push(enum_literal.enum_class_instance(self.db));
                }
                UnionElement::Type(_) => {}
            }
        }
        for ty in replace_with {
            self.add_in_place_impl(ty, seen_aliases);
        }
    }

    /// Returns the normalized union elements accumulated so far, excluding any alias remainder
    /// retained after reaching the recursive-unfold limit.
    fn current_type(&self) -> Type<'db> {
        Self {
            elements: self.elements.clone(),
            recursive_alias_remainder: None,
            db: self.db,
            unpack_aliases: self.unpack_aliases,
            cycle_recovery: self.cycle_recovery,
            normalization: self.normalization,
            recursively_defined: self.recursively_defined,
        }
        .try_build_resolved()
        .unwrap_or(Type::Never)
    }

    /// Adds a type to this union.
    pub(crate) fn add(mut self, ty: Type<'db>) -> Self {
        self.add_in_place(ty);
        self
    }

    /// Adds a type to this union.
    pub(crate) fn add_in_place(&mut self, ty: Type<'db>) {
        self.add_in_place_impl(ty, &mut vec![]);
    }

    pub(crate) fn add_in_place_impl(
        &mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<TypeIdentity<'db>>,
    ) {
        self.add_in_place_recursive(
            ty,
            seen_aliases,
            &mut ActiveRecursiveAliasExpansions::default(),
        );
    }

    fn add_in_place_recursive(
        &mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<TypeIdentity<'db>>,
        active_recursive_alias_expansions: &mut ActiveRecursiveAliasExpansions<'db>,
    ) {
        let cycle_recovery = self.cycle_recovery;
        let uses_relations = self.normalization.uses_relations() && !cycle_recovery;
        let should_widen = |literals, recursively_defined: RecursivelyDefined| {
            if recursively_defined.is_yes() && cycle_recovery {
                literals >= MAX_RECURSIVE_UNION_LITERALS
            } else {
                literals >= MAX_NON_RECURSIVE_UNION_LITERALS
            }
        };

        let mut ty_negated_cache = None;
        let mut ty_negated = || *ty_negated_cache.get_or_insert_with(|| ty.negate(self.db));

        match ty {
            Type::Union(union) => {
                let new_elements = union.elements(self.db);
                self.elements.reserve(new_elements.len());
                for element in new_elements {
                    self.add_in_place_recursive(
                        *element,
                        seen_aliases,
                        active_recursive_alias_expansions,
                    );
                }
                self.recursively_defined = self
                    .recursively_defined
                    .or(union.recursively_defined(self.db));
                if self.cycle_recovery && self.recursively_defined.is_yes() {
                    let literals = self.elements.iter().fold(0, |acc, elem| match elem {
                        UnionElement::IntLiterals(literals) => acc + literals.len(),
                        UnionElement::StringLiterals(literals) => acc + literals.len(),
                        UnionElement::BytesLiterals(literals) => acc + literals.len(),
                        UnionElement::EnumLiterals { literals, .. } => acc + literals.len(),
                        UnionElement::Type(_) => acc,
                    });
                    if should_widen(literals, self.recursively_defined) {
                        self.widen_literal_types(seen_aliases);
                    }
                }
            }
            // Adding `Never` to a union is a no-op.
            Type::Never => {}
            Type::TypeAlias(alias) if self.unpack_aliases => {
                let identity = ty.to_type_identity(self.db);
                let active_occurrences = seen_aliases
                    .iter()
                    .filter(|active| **active == identity)
                    .count();
                let current_type = matches!(identity, TypeIdentity::RecursiveTypeAlias(_))
                    .then(|| self.current_type());
                // An unchanged union alone is not a fixed point: later substitutions can still
                // expose new members, and union members after the recursive reference have not
                // been visited yet. Stop only after this exact specialization first grew the
                // union and then completed a subsequent cycle without changing it.
                if let Some(current_type) = current_type
                    && active_recursive_alias_expansions.is_at_fixed_point(ty, current_type)
                {
                    return;
                }
                // Whether a recursive union alias has a finite complete expansion is decidable in
                // principle for the standard type-alias calculus: alias definitions form a
                // restricted macro grammar without type-level conditionals or arbitrary
                // computation. A general decision procedure, however, requires substantially more
                // expensive grammar or higher-order pushdown analysis than is suitable for normal
                // union construction. Instead, unfold up to the shared operational limit and ask
                // the terminating relation checker whether the remainder is already covered by the
                // accumulated union. Preserve an unproved remainder so that it can be replaced with
                // `Unknown`; discarding it would make the result unsound.
                let should_stop = match identity {
                    TypeIdentity::RecursiveTypeAlias(_) => {
                        active_occurrences > MAX_RECURSIVE_TYPE_ALIAS_UNFOLDS
                    }
                    _ => active_occurrences > 0,
                };
                if should_stop {
                    // Defer the fallback until all sibling union elements have been processed.
                    // Otherwise `A | Alias` and `Alias | A` can normalize differently.
                    self.recursive_alias_remainder =
                        Some(self.recursive_alias_remainder.map_or(ty, |remainder| {
                            UnionType::from_elements_leave_aliases(self.db, [remainder, ty])
                        }));
                } else {
                    seen_aliases.push(identity);
                    if let Some(current_type) = current_type {
                        active_recursive_alias_expansions.enter(ty, current_type);
                    }
                    self.add_in_place_recursive(
                        alias.value_type(self.db),
                        seen_aliases,
                        active_recursive_alias_expansions,
                    );
                    if current_type.is_some() {
                        active_recursive_alias_expansions.exit(ty);
                    }
                    let popped = seen_aliases.pop();
                    debug_assert_eq!(popped, Some(identity));
                }
            }
            Type::LiteralValue(literal) => {
                self.recursively_defined =
                    self.recursively_defined.or(literal.recursively_defined());
                match literal.kind() {
                    // If adding a string literal, look for an existing `UnionElement::StringLiterals` to
                    // add it to, or an existing element that is a super-type of string literals, which
                    // means we shouldn't add it. Otherwise, add a new `UnionElement::StringLiterals`
                    // containing it.
                    LiteralValueTypeKind::String(string_literal) => {
                        let mut found = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            match element {
                                UnionElement::StringLiterals(literals) => {
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let replace_with = KnownClass::Str.to_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(self.db) == *existing =>
                                {
                                    return;
                                }
                                UnionElement::Type(existing)
                                    if uses_relations
                                        && !matches!(*existing, Type::TypeAlias(_)) =>
                                {
                                    // e.g. `existing` could be `Literal[""] & Any`,
                                    // and `ty` could be `Literal[""]`
                                    if ty.is_redundant_with(self.db, *existing) {
                                        return;
                                    }
                                    if existing.is_redundant_with(self.db, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if ty_negated().is_subtype_of(self.db, *existing) {
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
                            self.elements.push(UnionElement::StringLiterals(
                                FxOrderMap::from_iter([(string_literal, literal.is_promotable())]),
                            ));
                        }
                        if let Some(index) = to_remove {
                            self.elements.swap_remove(index);
                        }
                    }
                    // Same for bytes literals as for string literals, above.
                    LiteralValueTypeKind::Bytes(bytes_literal) => {
                        let mut found = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            match element {
                                UnionElement::BytesLiterals(literals) => {
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let replace_with = KnownClass::Bytes.to_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(self.db) == *existing =>
                                {
                                    return;
                                }
                                UnionElement::Type(existing)
                                    if uses_relations
                                        && !matches!(*existing, Type::TypeAlias(_)) =>
                                {
                                    if ty.is_redundant_with(self.db, *existing) {
                                        return;
                                    }
                                    // e.g. `existing` could be `Literal[b""] & Any`,
                                    // and `ty` could be `Literal[b""]`
                                    if existing.is_redundant_with(self.db, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if ty_negated().is_subtype_of(self.db, *existing) {
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
                            self.elements
                                .push(UnionElement::BytesLiterals(FxOrderMap::from_iter([(
                                    bytes_literal,
                                    literal.is_promotable(),
                                )])));
                        }
                        if let Some(index) = to_remove {
                            self.elements.swap_remove(index);
                        }
                    }
                    // And same for int literals as well.
                    LiteralValueTypeKind::Int(int_literal) => {
                        let mut found = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            match element {
                                UnionElement::IntLiterals(literals) => {
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let replace_with = KnownClass::Int.to_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(self.db) == *existing =>
                                {
                                    return;
                                }
                                UnionElement::Type(existing)
                                    if uses_relations
                                        && !matches!(*existing, Type::TypeAlias(_)) =>
                                {
                                    if ty.is_redundant_with(self.db, *existing) {
                                        return;
                                    }
                                    // e.g. `existing` could be `Literal[1] & Any`,
                                    // and `ty` could be `Literal[1]`
                                    if existing.is_redundant_with(self.db, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if ty_negated().is_subtype_of(self.db, *existing) {
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
                            self.elements
                                .push(UnionElement::IntLiterals(FxOrderMap::from_iter([(
                                    int_literal.as_i64(),
                                    literal.is_promotable(),
                                )])));
                        }
                        if let Some(index) = to_remove {
                            self.elements.swap_remove(index);
                        }
                    }
                    LiteralValueTypeKind::Enum(enum_member_to_add) => {
                        let enum_class_literal = enum_member_to_add.enum_class_literal(self.db);
                        let enum_class = enum_class_literal.class_literal(self.db);
                        let enum_member_count = enum_class_literal.member_count(self.db);
                        let members_are_exhaustive =
                            enum_class_literal.members_are_exhaustive(self.db);

                        if members_are_exhaustive && enum_member_count == 1 {
                            self.add_in_place_impl(
                                enum_member_to_add.enum_class_instance(self.db),
                                seen_aliases,
                            );
                            return;
                        }

                        let mut found = None;
                        let mut to_remove = None;
                        for (index, element) in self.elements.iter_mut().enumerate() {
                            match element {
                                UnionElement::EnumLiterals {
                                    enum_class: existing_enum_class,
                                    literals,
                                } => {
                                    if *existing_enum_class != enum_class {
                                        continue;
                                    }
                                    if should_widen(literals.len(), self.recursively_defined) {
                                        let (literal, _) = literals.first().unwrap();
                                        let replace_with = literal.enum_class_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing)
                                    if cycle_recovery
                                        && literal.fallback_instance(self.db) == *existing =>
                                {
                                    return;
                                }
                                UnionElement::Type(existing)
                                    if uses_relations
                                        && !matches!(*existing, Type::TypeAlias(_)) =>
                                {
                                    if ty.is_redundant_with(self.db, *existing) {
                                        return;
                                    }
                                    // e.g. `existing` could be `Literal[Foo.X] & Any`,
                                    // and `ty` could be `Literal[Foo.X]`
                                    if existing.is_redundant_with(self.db, ty) {
                                        to_remove = Some(index);
                                        continue;
                                    }
                                    if ty_negated().is_subtype_of(self.db, *existing) {
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
                                        self.add_in_place_impl(
                                            enum_member_to_add.enum_class_instance(self.db),
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
                            self.elements.push(UnionElement::EnumLiterals {
                                enum_class,
                                literals: FxOrderMap::from_iter([(
                                    enum_member_to_add,
                                    literal.is_promotable(),
                                )]),
                            });
                        }
                        if let Some(index) = to_remove {
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

    fn push_type(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<TypeIdentity<'db>>) {
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
        let uses_relations = self.normalization.uses_relations() && !self.cycle_recovery;
        let should_simplify_full = !matches!(ty, Type::TypeAlias(_)) && uses_relations;

        let mut ty_negated: Option<Type> = None;
        let mut to_remove = SmallVec::<[usize; 2]>::new();
        for (i, element) in self.elements.iter_mut().enumerate() {
            let element_type =
                match element.try_reduce(self.db, ty, self.cycle_recovery, should_simplify_full) {
                    ReduceResult::KeepIf(keep) => {
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
                        return;
                    }
                };

            if ty == element_type {
                return;
            }

            // `object` already contains every possible union element.
            if !self.cycle_recovery && element_type == Type::object() {
                return;
            }

            if uses_relations && should_preserve_hashable_union(self.db, ty, element_type) {
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
                ty = KnownClass::Range.to_instance(self.db);
                continue;
            }

            // Fold `(T & ~AlwaysTruthy) | (T & ~AlwaysFalsy)` to `T`.
            if uses_relations
                && let Some(merged_type) = merge_truthiness_guarded_pair(self.db, ty, element_type)
            {
                to_remove.push(i);
                ty = merged_type;
                continue;
            }

            if !self.cycle_recovery
                && element_type
                    .as_literal_value_kind()
                    .zip(bool_pair(ty))
                    .is_some_and(|(element, pair)| element == pair)
            {
                self.add_in_place_impl(KnownClass::Bool.to_instance(self.db), seen_aliases);
                return;
            }

            // Comparing `TypedDict`s for redundancy requires iterating over their fields, which is
            // problematic if some of those fields point to recursive `Union`s. To avoid cycles,
            // compare `TypedDict`s by name/identity instead of using the `has_relation_to`
            // machinery.
            if element_type.is_typed_dict() && ty.is_typed_dict() {
                continue;
            }

            if should_simplify_full && !matches!(element_type, Type::TypeAlias(_)) {
                if ty.is_redundant_with(self.db, element_type) {
                    return;
                }

                if element_type.is_redundant_with(self.db, ty) {
                    to_remove.push(i);
                    continue;
                }

                if ty.negation_is_subtype_of_cached(self.db, element_type, &mut ty_negated) {
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

        let mut to_remove = to_remove.into_iter();
        if let Some(first) = to_remove.next() {
            self.elements[first] = UnionElement::Type(ty);
            // We iterate in descending order to keep remaining indices valid after `swap_remove`.
            for index in to_remove.rev() {
                self.elements.swap_remove(index);
            }
        } else {
            self.elements.push(UnionElement::Type(ty));
        }
    }

    pub(crate) fn build(self) -> Type<'db> {
        self.try_build().unwrap_or(Type::Never)
    }

    pub(crate) fn try_build(self) -> Option<Type<'db>> {
        if self.normalization.uses_relations() && !self.cycle_recovery {
            let db = self.db;
            self.try_build_with_recursive_alias_remainder_check(|remainder, current| {
                remainder.is_redundant_with(db, current)
            })
        } else {
            self.try_build_with_recursive_alias_remainder_check(|_, _| false)
        }
    }

    /// Builds the union using a caller-provided semantic check for deferred recursive aliases.
    ///
    /// This lets a caller that already owns a type-relation checker avoid starting a nested
    /// relation query while preserving the same recursive-alias fallback semantics.
    pub(crate) fn build_with_recursive_alias_remainder_check(
        self,
        is_redundant: impl FnMut(Type<'db>, Type<'db>) -> bool,
    ) -> Type<'db> {
        self.try_build_with_recursive_alias_remainder_check(is_redundant)
            .unwrap_or(Type::Never)
    }

    fn try_build_with_recursive_alias_remainder_check(
        mut self,
        mut is_redundant: impl FnMut(Type<'db>, Type<'db>) -> bool,
    ) -> Option<Type<'db>> {
        let remainder = self.recursive_alias_remainder.take();
        let db = self.db;
        let cycle_recovery = self.cycle_recovery;
        let normalization = self.normalization;
        let current = self.try_build_resolved();
        let Some(remainder) = remainder else {
            return current;
        };
        if current.is_some_and(|current| is_redundant(remainder, current)) {
            return current;
        }

        // The unexpanded remainder may contribute union members not seen within the operational
        // limit. Dropping it would under-approximate the alias, so retain soundness by covering
        // those unknown members with `Unknown`.
        let mut fallback = Self::with_normalization(db, normalization)
            .unpack_aliases(false)
            .cycle_recovery(cycle_recovery);
        if let Some(current) = current {
            fallback.add_in_place(current);
        }
        fallback.add_in_place(Type::unknown());
        fallback.try_build_resolved()
    }

    fn try_build_resolved(self) -> Option<Type<'db>> {
        let db = self.db;
        let unpack_aliases = self.unpack_aliases;
        let cycle_recovery = self.cycle_recovery;
        let normalization = self.normalization;
        let recursively_defined = self.recursively_defined;

        let type_count = self.elements.iter().map(UnionElement::type_count).sum();
        let mut types = Vec::with_capacity(type_count);
        for element in self.elements {
            match element {
                UnionElement::IntLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElement::StringLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElement::BytesLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElement::EnumLiterals { literals, .. } => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(
                            LiteralValueType::new(literal, promotable)
                                .with_recursively_defined(recursively_defined),
                        )
                    }));
                }
                UnionElement::Type(ty) => types.push(ty),
            }
        }

        if !cycle_recovery && normalize_enum_complement_unions(db, &mut types) {
            let builder = UnionBuilder::with_normalization(db, normalization)
                .unpack_aliases(unpack_aliases)
                .cycle_recovery(cycle_recovery)
                .recursively_defined(recursively_defined);
            return types
                .into_iter()
                .fold(builder, UnionBuilder::add)
                .try_build();
        }

        match types.len() {
            0 => None,
            1 => Some(types[0]),
            _ => Some(Type::Union(UnionType::new(
                db,
                types.into_boxed_slice(),
                recursively_defined,
            ))),
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
    normalization: NormalizationMode,
}

impl<'db> IntersectionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self::with_normalization(db, NormalizationMode::default())
    }

    fn with_normalization(db: &'db dyn Db, normalization: NormalizationMode) -> Self {
        Self {
            db,
            intersections: vec![InnerIntersectionBuilder::default()],
            normalization,
        }
    }

    /// Creates an intersection builder that never starts a type-relation query.
    pub(crate) fn structural(db: &'db dyn Db) -> Self {
        Self::with_normalization(db, NormalizationMode::Structural)
    }

    fn empty(db: &'db dyn Db, normalization: NormalizationMode) -> Self {
        Self {
            db,
            intersections: vec![],
            normalization,
        }
    }

    /// Add DNF branches, dropping those that have already collapsed to `Never` so that later
    /// union distribution does not multiply dead branches.
    fn extend(&mut self, other: Self) {
        debug_assert_eq!(self.normalization, other.normalization);
        self.intersections.extend(
            other
                .intersections
                .into_iter()
                .filter(|intersection| !intersection.contains_never()),
        );
    }

    pub(crate) fn add_positive(self, ty: Type<'db>) -> Self {
        self.add_positive_impl(ty, &mut vec![])
    }

    pub(crate) fn add_positive_impl(
        mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<TypeIdentity<'db>>,
    ) -> Self {
        match ty {
            Type::TypeAlias(alias) => {
                let identity = ty.to_type_identity(self.db);
                if seen_aliases.contains(&identity) {
                    // Recursive alias, add it without expanding to avoid infinite recursion.
                    for inner in &mut self.intersections {
                        inner.positive.insert(ty);
                    }
                    return self;
                }
                seen_aliases.push(identity);
                let value_type = alias.value_type(self.db);
                let result = self.add_positive_impl(value_type, seen_aliases);
                let popped = seen_aliases.pop();
                debug_assert_eq!(popped, Some(identity));
                result
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
                union
                    .elements(self.db)
                    .iter()
                    .map(|elem| self.clone().add_positive_impl(*elem, seen_aliases))
                    .fold(
                        IntersectionBuilder::empty(self.db, self.normalization),
                        |mut builder, sub| {
                            builder.extend(sub);
                            builder
                        },
                    )
            }
            // `(A & B & ~C) & (D & E & ~F)` -> `A & B & D & E & ~C & ~F`
            Type::Intersection(other) => {
                let db = self.db;
                for pos in other.positive(db) {
                    self = self.add_positive_impl(*pos, seen_aliases);
                }
                for neg in other.negative(db) {
                    self = self.add_negative_impl(*neg, seen_aliases);
                }
                self
            }
            Type::EnumComplement(complement) => {
                let db = self.db;
                self.add_positive_impl(complement.to_intersection(db), seen_aliases)
            }
            _ => {
                // If we are already a union-of-intersections, distribute the new intersected element
                // across all of those intersections.
                for inner in &mut self.intersections {
                    inner.add_positive(self.db, self.normalization, ty);
                }
                self
            }
        }
    }

    pub(crate) fn add_negative(self, ty: Type<'db>) -> Self {
        self.add_negative_impl(ty, &mut vec![])
    }

    pub(crate) fn add_negative_impl(
        mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<TypeIdentity<'db>>,
    ) -> Self {
        // See comments above in `add_positive`; this is just the negated version.
        match ty {
            Type::TypeAlias(alias) => {
                let identity = ty.to_type_identity(self.db);
                if seen_aliases.contains(&identity) {
                    // Recursive alias, add it without expanding to avoid infinite recursion.
                    for inner in &mut self.intersections {
                        inner.negative.insert(ty);
                    }
                    return self;
                }
                seen_aliases.push(identity);
                let value_type = alias.value_type(self.db);
                let result = self.add_negative_impl(value_type, seen_aliases);
                let popped = seen_aliases.pop();
                debug_assert_eq!(popped, Some(identity));
                result
            }
            Type::Union(union) => {
                for elem in union.elements(self.db) {
                    self = self.add_negative_impl(*elem, seen_aliases);
                }
                self
            }
            Type::Intersection(intersection) => {
                // (A | B) & ~(C & ~D)
                // -> (A | B) & (~C | D)
                // -> ((A | B) & ~C) | ((A | B) & D)
                // i.e. if we have an intersection of positive constraints C
                // and negative constraints D, then our new intersection
                // is (existing & ~C) | (existing & D)

                let positive_side = intersection
                    .positive(self.db)
                    .iter()
                    // we negate all the positive constraints while distributing
                    .map(|elem| {
                        self.clone()
                            .add_negative_impl(*elem, &mut seen_aliases.clone())
                    });

                let negative_side = intersection
                    .negative(self.db)
                    .iter()
                    // all negative constraints end up becoming positive constraints
                    .map(|elem| {
                        self.clone()
                            .add_positive_impl(*elem, &mut seen_aliases.clone())
                    });

                positive_side.chain(negative_side).fold(
                    IntersectionBuilder::empty(self.db, self.normalization),
                    |mut builder, sub| {
                        builder.extend(sub);
                        builder
                    },
                )
            }
            Type::EnumComplement(complement) => {
                let db = self.db;
                self.add_negative_impl(complement.to_intersection(db), seen_aliases)
            }
            _ => {
                for inner in &mut self.intersections {
                    inner.add_negative(self.db, self.normalization, ty);
                }
                self
            }
        }
    }

    pub(crate) fn positive_elements<I, T>(mut self, elements: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        for element in elements {
            self = self.add_positive(element.into());
        }
        self
    }

    pub(crate) fn build(self) -> Type<'db> {
        let mut intersections = self
            .intersections
            .into_iter()
            .map(|intersection| intersection.build(self.db, self.normalization));
        let Some(first) = intersections.next() else {
            return Type::Never;
        };
        let Some(second) = intersections.next() else {
            return first;
        };

        intersections
            .fold(
                UnionBuilder::with_normalization(self.db, self.normalization)
                    .add(first)
                    .add(second),
                UnionBuilder::add,
            )
            .build()
    }
}

#[derive(Debug, Clone, Default)]
struct InnerIntersectionBuilder<'db> {
    positive: FxOrderSet<Type<'db>>,
    negative: NegativeIntersectionElements<'db>,
}

impl<'db> InnerIntersectionBuilder<'db> {
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
    fn has_empty_enum_complement(&self, db: &'db dyn Db) -> bool {
        for positive in &self.positive {
            let Type::NominalInstance(instance) = positive else {
                continue;
            };

            let Some(enum_class_literal) = instance.class_literal(db).into_enum_class(db) else {
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
        normalization: NormalizationMode,
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
                if let Some(narrowed) = SubclassOfType::try_from_instance(
                    db,
                    typeform.type_argument(db).resolve_type_alias(db),
                ) && self.positive.swap_remove(&KnownClass::Type.to_instance(db))
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
                                typeform.type_argument(db).resolve_type_alias(db),
                            )
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
                self.add_negative(db, normalization, Type::string_literal(db, ""));
            }
            // `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::AlwaysFalsy if self.positive.swap_remove(&Type::literal_string()) => {
                self.add_positive(db, normalization, Type::string_literal(db, ""));
            }
            // `AlwaysTruthy & LiteralString` -> `LiteralString & ~Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string()
                    && self.positive.swap_remove(&Type::AlwaysTruthy) =>
            {
                self.add_positive(db, normalization, Type::literal_string());
                self.add_negative(db, normalization, Type::string_literal(db, ""));
            }
            // `AlwaysFalsy & LiteralString` -> `Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string() && self.positive.swap_remove(&Type::AlwaysFalsy) =>
            {
                self.add_positive(db, normalization, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string()
                    && self.negative.swap_remove(&Type::AlwaysTruthy) =>
            {
                self.add_positive(db, normalization, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string() && self.negative.swap_remove(&Type::AlwaysFalsy) =>
            {
                self.add_positive(db, normalization, Type::literal_string());
                self.add_negative(db, normalization, Type::string_literal(db, ""));
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

                if normalization.uses_relations() {
                    let mut to_remove = SmallVec::<[usize; 1]>::new();
                    for (index, existing_positive) in self.positive.iter().enumerate() {
                        // S & T = S if S <: T or T is an invariant-dynamic generalization of S.
                        if existing_positive.is_redundant_with(db, new_positive)
                            || is_invariant_dynamic_generalization_of(
                                db,
                                new_positive,
                                *existing_positive,
                            )
                        {
                            return;
                        }
                        // same rule, reverse order
                        if new_positive.is_redundant_with(db, *existing_positive)
                            || is_invariant_dynamic_generalization_of(
                                db,
                                *existing_positive,
                                new_positive,
                            )
                        {
                            to_remove.push(index);
                        }
                        // A & B = Never    if A and B are disjoint
                        if new_positive.is_disjoint_from(db, *existing_positive) {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                    }
                    for index in to_remove.into_iter().rev() {
                        self.positive.swap_remove_index(index);
                    }

                    let mut to_remove = SmallVec::<[usize; 1]>::new();
                    for (index, existing_negative) in self.negative.iter().enumerate() {
                        // S & ~T = Never    if S <: T
                        if new_positive.is_subtype_of(db, *existing_negative) {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                        // A & ~B = A    if A and B are disjoint
                        if existing_negative.is_disjoint_from(db, new_positive) {
                            to_remove.push(index);
                        }
                    }
                    for index in to_remove.into_iter().rev() {
                        self.negative.swap_remove_index(index);
                    }
                }

                self.positive.insert(new_positive);
            }
        }
    }

    /// Adds a negative type to this intersection.
    fn add_negative(
        &mut self,
        db: &'db dyn Db,
        normalization: NormalizationMode,
        new_negative: Type<'db>,
    ) {
        // `Never & ~T` -> `Never`.
        if self.positive.contains(&Type::Never) {
            return;
        }

        // `Divergent & ~T` -> `Divergent`.
        if self.positive.iter().any(Type::is_divergent) {
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
                    self.add_negative(db, normalization, *pos);
                }
                for neg in inter.negative(db) {
                    self.add_positive(db, normalization, *neg);
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
                self.add_positive(db, normalization, ty);
            }
            // `bool & ~AlwaysTruthy` -> `bool & Literal[False]`
            Type::AlwaysTruthy if contains_bool() => {
                self.add_positive(db, normalization, Type::bool_literal(false));
            }
            // `bool & ~Literal[True]` -> `bool & Literal[False]`
            Type::LiteralValue(literal) if literal.as_bool() == Some(true) && contains_bool() => {
                self.add_positive(db, normalization, Type::bool_literal(false));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & Literal[""]`
            Type::AlwaysTruthy if self.positive.contains(&Type::literal_string()) => {
                self.add_positive(db, normalization, Type::string_literal(db, ""));
            }
            // `bool & ~AlwaysFalsy` -> `bool & Literal[True]`
            Type::AlwaysFalsy if contains_bool() => {
                self.add_positive(db, normalization, Type::bool_literal(true));
            }
            // `bool & ~Literal[False]` -> `bool & Literal[True]`
            Type::LiteralValue(literal) if literal.as_bool() == Some(false) && contains_bool() => {
                self.add_positive(db, normalization, Type::bool_literal(true));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::AlwaysFalsy if self.positive.contains(&Type::literal_string()) => {
                self.add_negative(db, normalization, Type::string_literal(db, ""));
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

                    if normalization.uses_relations() {
                        // ~S & ~T = ~T    if S <: T
                        if existing_negative.is_redundant_with(db, new_negative) {
                            to_remove.push(index);
                        }
                        // same rule, reverse order
                        if new_negative.is_subtype_of(db, *existing_negative) {
                            return;
                        }
                    }
                }
                for index in to_remove.into_iter().rev() {
                    self.negative.swap_remove_index(index);
                }

                for existing_positive in &self.positive {
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
                                instance.class_literal(db) == new_enum.enum_class(db)
                            })
                        {
                            continue;
                        }
                    }

                    if normalization.uses_relations() {
                        // S & ~T = Never    if S <: T
                        if existing_positive.is_subtype_of(db, new_negative) {
                            *self = Self::default();
                            self.positive.insert(Type::Never);
                            return;
                        }
                        // A & ~B = A    if A and B are disjoint
                        if existing_positive.is_disjoint_from(db, new_negative) {
                            return;
                        }
                    }
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
    fn simplify_constrained_typevars(&mut self, db: &'db dyn Db) {
        let mut to_add = SmallVec::<[Type<'db>; 1]>::new();

        for ty in &self.positive {
            let Type::TypeVar(bound_typevar) = ty else {
                continue;
            };
            let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
                bound_typevar.typevar(db).bound_or_constraints(db)
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
                    .filter(|(_, c)| c.is_subtype_of(db, *negative));
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
            self.add_positive(db, NormalizationMode::RelationAware, remaining_constraint);
        }
    }

    fn build(mut self, db: &'db dyn Db, normalization: NormalizationMode) -> Type<'db> {
        if self.has_empty_enum_complement(db) {
            return Type::Never;
        }

        if normalization.uses_relations() {
            self.simplify_constrained_typevars(db);
        }

        // If any typevars are in `self.positive`, speculatively solve all bounded type variables
        // to their upper bound and all constrained type variables to the union of their constraints.
        // If that speculative intersection simplifies to `Never`, this intersection must also simplify
        // to `Never`.
        if normalization.uses_relations()
            && self
                .positive
                .iter()
                .any(|ty| matches!(ty, Type::TypeVar(_) | Type::NewTypeInstance(_)))
        {
            let speculative =
                expand_intersection_typevars_and_newtypes(db, &self.positive, &self.negative);
            if speculative.is_never() {
                return Type::Never;
            }
        }

        if let Some(complement) =
            EnumComplement::from_intersection_parts(db, &self.positive, &self.negative)
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
        IntersectionBuilder, IntersectionType, MAX_NON_RECURSIVE_UNION_LITERALS,
        MAX_RECURSIVE_UNION_LITERALS, NegativeIntersectionElements, RecursivelyDefined, Type,
        UnionBuilder, UnionType,
    };

    use crate::FxOrderSet;
    use crate::db::tests::{TestDb, setup_db};
    use crate::place::{global_symbol, known_module_symbol};
    use crate::types::enums::{EnumComplement, enum_member_literals};
    use crate::types::type_alias::TypeAliasType;
    use crate::types::typevar::TypeVarConstraints;
    use crate::types::{
        BoundTypeVarInstance, DynamicType, KnownClass, KnownInstanceType, Truthiness,
        TypeVarBoundOrConstraints, TypeVarVariance,
    };

    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_db::testing::find_will_execute_event_by_name;
    use ruff_python_ast::name::Name;
    use ty_module_resolver::KnownModule;

    #[test]
    fn build_union_no_elements() {
        let db = setup_db();

        let empty_union = UnionBuilder::new(&db).build();
        assert_eq!(empty_union, Type::Never);
    }

    #[test]
    fn build_union_single_element() {
        let db = setup_db();

        let t0 = Type::int_literal(0);
        let union = UnionType::from_elements(&db, [t0]);
        assert_eq!(union, t0);
    }

    #[test]
    fn build_union_two_elements() {
        let db = setup_db();

        let t0 = Type::int_literal(0);
        let t1 = Type::int_literal(1);
        let union = UnionType::from_elements(&db, [t0, t1]).expect_union();

        assert_eq!(union.elements(&db), &[t0, t1]);
    }

    #[test]
    fn structural_union_preserves_relation_redundancies_in_both_orders() {
        let db = setup_db();
        let int_instance = KnownClass::Int.to_instance(&db);
        let int_literal = Type::int_literal(1);

        for [first, second] in [[int_instance, int_literal], [int_literal, int_instance]] {
            assert_eq!(
                UnionBuilder::new(&db).add(first).add(second).build(),
                int_instance
            );

            let union = UnionBuilder::structural(&db)
                .add(first)
                .add(second)
                .build()
                .expect_union();
            assert_eq!(union.elements(&db).len(), 2);
        }
    }

    #[test]
    fn structural_intersection_preserves_relation_redundancies() {
        let db = setup_db();
        let int_instance = KnownClass::Int.to_instance(&db);
        let int_literal = Type::int_literal(1);

        let mut event_db = db.clone();
        event_db.clear_salsa_events();
        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(int_instance)
            .add_positive(int_literal)
            .build()
        else {
            panic!("structural normalization should preserve both positive elements");
        };
        assert_eq!(intersection.positive(&db).len(), 2);
        assert!(intersection.positive(&db).contains(&int_instance));
        assert!(intersection.positive(&db).contains(&int_literal));

        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(int_literal)
            .add_negative(int_instance)
            .build()
        else {
            panic!("structural normalization should preserve a related negative element");
        };
        assert_eq!(intersection.positive(&db).as_slice(), &[int_literal]);
        assert_eq!(intersection.negative(&db).len(), 1);
        assert!(intersection.negative(&db).contains(&int_instance));

        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_negative(int_instance)
            .add_negative(int_literal)
            .build()
        else {
            panic!("structural normalization should preserve both negative elements");
        };
        assert_eq!(intersection.negative(&db).len(), 2);
        assert!(intersection.negative(&db).contains(&int_instance));
        assert!(intersection.negative(&db).contains(&int_literal));

        let events = event_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "is_redundant_with_impl", None, &events).is_none(),
            "structural intersection normalization must not start a relation query"
        );

        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(int_instance)
                .add_positive(int_literal)
                .build(),
            int_literal
        );
        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(int_literal)
                .add_negative(int_instance)
                .build(),
            Type::Never
        );
        let Type::Intersection(intersection) = IntersectionBuilder::new(&db)
            .add_negative(int_instance)
            .add_negative(int_literal)
            .build()
        else {
            panic!("the relation-aware result should retain ~int");
        };
        assert_eq!(intersection.negative(&db).len(), 1);
        assert!(intersection.negative(&db).contains(&int_instance));
    }

    #[test]
    fn structural_intersection_skips_other_relation_simplifications() {
        let db = setup_db();
        let int_instance = KnownClass::Int.to_instance(&db);
        let str_instance = KnownClass::Str.to_instance(&db);

        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(int_instance)
                .add_positive(str_instance)
                .build(),
            Type::Never
        );
        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(int_instance)
            .add_positive(str_instance)
            .build()
        else {
            panic!("structural normalization should skip disjointness checks");
        };
        assert_eq!(intersection.positive(&db).len(), 2);

        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_negative(str_instance)
                .add_positive(int_instance)
                .build(),
            int_instance
        );
        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_negative(str_instance)
            .add_positive(int_instance)
            .build()
        else {
            panic!("structural normalization should preserve disjoint negative elements");
        };
        assert!(intersection.positive(&db).contains(&int_instance));
        assert!(intersection.negative(&db).contains(&str_instance));

        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(int_instance)
                .add_negative(str_instance)
                .build(),
            int_instance
        );
        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(int_instance)
            .add_negative(str_instance)
            .build()
        else {
            panic!("structural normalization should preserve disjoint negative elements");
        };
        assert!(intersection.positive(&db).contains(&int_instance));
        assert!(intersection.negative(&db).contains(&str_instance));

        let list_of_any = KnownClass::List.to_specialized_instance(&db, &[Type::any()]);
        let list_of_int = KnownClass::List.to_specialized_instance(&db, &[int_instance]);
        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(list_of_any)
                .add_positive(list_of_int)
                .build(),
            list_of_int
        );
        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(list_of_any)
            .add_positive(list_of_int)
            .build()
        else {
            panic!("structural normalization should skip invariant dynamic generalization");
        };
        assert_eq!(intersection.positive(&db).len(), 2);
    }

    #[test]
    fn structural_intersection_skips_typevar_and_newtype_expansion() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/newtype.py",
            r#"
            from typing import NewType

            UserId = NewType("UserId", int)
            "#,
        )
        .unwrap();

        let int_instance = KnownClass::Int.to_instance(&db);
        let str_instance = KnownClass::Str.to_instance(&db);

        let constrained =
            BoundTypeVarInstance::synthetic(&db, Name::new_static("T"), TypeVarVariance::Invariant)
                .map_bound_or_constraints(&db, |_| {
                    Some(TypeVarBoundOrConstraints::Constraints(
                        TypeVarConstraints::new(
                            &db,
                            vec![int_instance, str_instance].into_boxed_slice(),
                        ),
                    ))
                });
        let constrained = Type::TypeVar(constrained);

        let Type::Intersection(intersection) = IntersectionBuilder::new(&db)
            .add_positive(constrained)
            .add_negative(int_instance)
            .build()
        else {
            panic!("relation-aware normalization should narrow the constrained typevar");
        };
        assert!(intersection.positive(&db).contains(&str_instance));
        assert!(intersection.negative(&db).is_empty());

        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(constrained)
            .add_negative(int_instance)
            .build()
        else {
            panic!("structural normalization should preserve the constrained typevar shape");
        };
        assert_eq!(intersection.positive(&db).as_slice(), &[constrained]);
        assert!(intersection.negative(&db).contains(&int_instance));

        let module = ruff_db::files::system_path_to_file(&db, "/src/newtype.py").unwrap();
        let Type::KnownInstance(KnownInstanceType::NewType(newtype)) =
            global_symbol(&db, module, "UserId").place.expect_type()
        else {
            panic!("UserId should be a NewType");
        };
        let user_id = Type::NewTypeInstance(newtype);

        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(user_id)
                .add_positive(str_instance)
                .build(),
            Type::Never
        );
        let Type::Intersection(intersection) = IntersectionBuilder::structural(&db)
            .add_positive(user_id)
            .add_positive(str_instance)
            .build()
        else {
            panic!("structural normalization should not expand a NewType to its base");
        };
        assert_eq!(intersection.positive(&db).len(), 2);
    }

    #[test]
    fn structural_intersection_preserves_normalization_through_dnf() {
        let db = setup_db();
        let int_instance = KnownClass::Int.to_instance(&db);
        let int_literal = Type::int_literal(1);
        let source = Type::Union(UnionType::new(
            &db,
            vec![int_instance, int_literal].into_boxed_slice(),
            RecursivelyDefined::No,
        ));

        assert_eq!(
            IntersectionBuilder::new(&db).add_positive(source).build(),
            int_instance
        );

        let union = IntersectionBuilder::structural(&db)
            .add_positive(source)
            .build()
            .expect_union();
        assert_eq!(union.elements(&db), &[int_instance, int_literal]);

        let mut negative = NegativeIntersectionElements::default();
        negative.insert(int_instance);
        let source = Type::Intersection(IntersectionType::new(
            &db,
            FxOrderSet::from_iter([int_literal]),
            negative,
        ));

        assert_eq!(
            IntersectionBuilder::new(&db).add_negative(source).build(),
            Type::object()
        );
        let union = IntersectionBuilder::structural(&db)
            .add_negative(source)
            .build()
            .expect_union();
        assert!(union.elements(&db).contains(&int_instance));
        assert!(union.elements(&db).contains(&int_literal.negate(&db)));
    }

    #[test]
    fn structural_union_normalizes_enum_complements() {
        let db = setup_db();
        let enum_class = known_module_symbol(&db, KnownModule::Uuid, "SafeUUID")
            .place
            .expect_type()
            .expect_class_literal();
        let mut literals =
            enum_member_literals(&db, enum_class, None).expect("SafeUUID is an enum");
        let literal = literals.next().expect("SafeUUID has members");
        let other_literal = literals.next().expect("SafeUUID has multiple members");
        let third_literal = literals.next().expect("SafeUUID has three members");
        drop(literals);
        let enum_instance = literal.expect_enum_literal().enum_class_instance(&db);
        let complement = IntersectionBuilder::new(&db)
            .add_positive(enum_instance)
            .add_negative(literal)
            .build();

        assert_eq!(
            UnionBuilder::new(&db).add(complement).add(literal).build(),
            enum_instance
        );

        assert_eq!(
            UnionBuilder::structural(&db)
                .add(complement)
                .add(literal)
                .build(),
            enum_instance
        );

        let int_instance = KnownClass::Int.to_instance(&db);
        let int_literal = Type::int_literal(1);
        let union = UnionBuilder::structural(&db)
            .add(complement)
            .add(literal)
            .add(int_instance)
            .add(int_literal)
            .build()
            .expect_union();
        assert_eq!(union.elements(&db).len(), 3);
        assert!(union.elements(&db).contains(&enum_instance));
        assert!(union.elements(&db).contains(&int_instance));
        assert!(union.elements(&db).contains(&int_literal));

        let literal = literal.expect_enum_literal();
        let other_literal = other_literal.expect_enum_literal();
        let third_literal = third_literal.expect_enum_literal();
        let enum_class_literal = literal.enum_class_literal(&db);
        let dynamic = Type::Dynamic(DynamicType::Any);
        let rest = FxOrderSet::from_iter([dynamic]);
        let complement = Type::EnumComplement(EnumComplement::new(
            &db,
            enum_class_literal,
            FxOrderSet::from_iter([literal.name(&db).clone()]),
            rest.clone(),
        ));
        let other_complement = Type::EnumComplement(EnumComplement::new(
            &db,
            enum_class_literal,
            FxOrderSet::from_iter([other_literal.name(&db).clone()]),
            rest.clone(),
        ));

        let mut event_db = db.clone();
        event_db.clear_salsa_events();
        let Type::Intersection(intersection) = UnionBuilder::structural(&db)
            .add(complement)
            .add(other_complement)
            .build()
        else {
            panic!("enum complements with rest should merge to an intersection");
        };
        assert_eq!(intersection.positive(&db).len(), 2);
        assert!(intersection.positive(&db).contains(&enum_instance));
        assert!(intersection.positive(&db).contains(&dynamic));
        assert!(intersection.negative(&db).is_empty());

        let complement = Type::EnumComplement(EnumComplement::new(
            &db,
            enum_class_literal,
            FxOrderSet::from_iter([literal.name(&db).clone(), other_literal.name(&db).clone()]),
            rest.clone(),
        ));
        let other_complement = Type::EnumComplement(EnumComplement::new(
            &db,
            enum_class_literal,
            FxOrderSet::from_iter([literal.name(&db).clone(), third_literal.name(&db).clone()]),
            rest.clone(),
        ));
        let Type::EnumComplement(complement) = UnionBuilder::structural(&db)
            .add(complement)
            .add(other_complement)
            .build()
        else {
            panic!("shared enum exclusions should remain a complement");
        };
        assert_eq!(
            complement.excluded_names(&db),
            &FxOrderSet::from_iter([literal.name(&db).clone()])
        );
        assert_eq!(complement.rest(&db), &rest);

        let events = event_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "is_redundant_with_impl", None, &events).is_none(),
            "structural enum normalization must not start a relation query"
        );
    }

    #[test]
    fn cycle_recovery_widens_recursive_literal_union() {
        let db = setup_db();
        let literal_limit =
            i64::try_from(MAX_RECURSIVE_UNION_LITERALS).expect("literal limit fits in i64");

        let union = (0..=literal_limit).map(Type::int_literal).fold(
            UnionBuilder::new(&db)
                .cycle_recovery(true)
                .recursively_defined(RecursivelyDefined::Yes),
            UnionBuilder::add,
        );

        assert_eq!(union.build(), KnownClass::Int.to_instance(&db));

        let assert_widens = |literal, instance| {
            for (first, second) in [(literal, instance), (instance, literal)] {
                let union = UnionBuilder::new(&db)
                    .cycle_recovery(true)
                    .add(first)
                    .add(second)
                    .build();
                assert_eq!(union, instance);
            }
        };

        assert_widens(Type::int_literal(1), KnownClass::Int.to_instance(&db));
        assert_widens(
            Type::string_literal(&db, "literal"),
            KnownClass::Str.to_instance(&db),
        );
        assert_widens(
            Type::bytes_literal(&db, b"literal"),
            KnownClass::Bytes.to_instance(&db),
        );

        let safe_uuid_class = known_module_symbol(&db, KnownModule::Uuid, "SafeUUID")
            .place
            .expect_type()
            .expect_class_literal();
        let enum_literal = enum_member_literals(&db, safe_uuid_class, None)
            .expect("SafeUUID is an enum")
            .next()
            .expect("SafeUUID has members");
        assert_widens(
            enum_literal,
            enum_literal.expect_enum_literal().enum_class_instance(&db),
        );
    }

    #[test]
    fn cycle_recovery_skips_other_redundancy_simplification() {
        let db = setup_db();

        for (left, right) in [
            (Type::string_literal(&db, "literal"), Type::literal_string()),
            (Type::bool_literal(true), KnownClass::Bool.to_instance(&db)),
            (Type::int_literal(1), Type::object()),
            (Type::bool_literal(true), Type::bool_literal(false)),
        ] {
            for (first, second) in [(left, right), (right, left)] {
                let union = UnionBuilder::new(&db)
                    .cycle_recovery(true)
                    .add(first)
                    .add(second)
                    .build()
                    .expect_union();
                assert!(union.elements(&db).contains(&left));
                assert!(union.elements(&db).contains(&right));
            }
        }
    }

    #[test]
    fn union_common_literal_supertype() {
        let db = setup_db();

        let str_union = UnionType::from_elements(
            &db,
            [
                Type::string_literal(&db, "a"),
                Type::string_literal(&db, "b"),
            ],
        )
        .expect_union();
        assert_eq!(
            str_union.common_literal_supertype(&db),
            Some(Type::literal_string())
        );

        let int_union = UnionType::from_elements(&db, [Type::int_literal(1), Type::int_literal(2)])
            .expect_union();
        assert_eq!(
            int_union.common_literal_supertype(&db),
            Some(KnownClass::Int.to_instance(&db))
        );

        let mixed_union =
            UnionType::from_elements(&db, [Type::string_literal(&db, "a"), Type::int_literal(1)])
                .expect_union();
        assert_eq!(mixed_union.common_literal_supertype(&db), None);
    }

    fn map_marker<'db>(ty: &Type<'db>, marker: Type<'db>, replacement: Type<'db>) -> Type<'db> {
        if *ty == marker { replacement } else { *ty }
    }

    #[test]
    fn map_rebuilds_prefix_for_literal_widening() {
        let db = setup_db();

        let marker = KnownClass::Str.to_instance(&db);
        let literal_limit =
            i64::try_from(MAX_NON_RECURSIVE_UNION_LITERALS).expect("literal limit fits in i64");
        let widening_literal = Type::int_literal(literal_limit);
        let expected = KnownClass::Int.to_instance(&db);

        let elements = (0..literal_limit).map(Type::int_literal).chain([marker]);
        let union = UnionType::from_elements(&db, elements).expect_union();

        assert_eq!(
            union.map(&db, |ty| map_marker(ty, marker, widening_literal)),
            expected
        );
        assert_eq!(
            union.map_leave_aliases(&db, |ty| map_marker(ty, marker, widening_literal)),
            expected
        );
        assert_eq!(
            union.try_map(&db, |ty| Some(map_marker(ty, marker, widening_literal))),
            Some(expected)
        );
    }

    #[test]
    fn map_preserves_alias_unpacking_behavior() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "type Alias = int").unwrap();

        let module = ruff_db::files::system_path_to_file(&db, "/src/a.py").unwrap();
        let alias_ty = global_symbol(&db, module, "Alias").place.expect_type();
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(alias))) =
            alias_ty
        else {
            panic!("Expected `Alias` to be a type alias");
        };

        let alias = Type::TypeAlias(TypeAliasType::PEP695(alias));
        let str_instance = KnownClass::Str.to_instance(&db);
        let union_ty = UnionType::from_elements_leave_aliases(&db, [alias, str_instance]);
        let union = union_ty.expect_union();
        let unpacked =
            UnionType::from_elements(&db, [KnownClass::Int.to_instance(&db), str_instance]);

        assert_eq!(union.map(&db, |ty| *ty), unpacked);
        assert_eq!(union.try_map(&db, |ty| Some(*ty)), Some(unpacked));
        assert_eq!(union.map_leave_aliases(&db, |ty| *ty), union_ty);
    }

    #[test]
    fn build_intersection_empty_intersection_equals_object() {
        let db = setup_db();

        let intersection = IntersectionBuilder::new(&db).build();
        assert_eq!(intersection, Type::object());
    }

    #[test]
    fn build_intersection_discards_never_dnf_branches() {
        let db = setup_db();
        let int = KnownClass::Int.to_instance(&db);
        let str = KnownClass::Str.to_instance(&db);
        let bytes = KnownClass::Bytes.to_instance(&db);

        let int_or_str = UnionType::from_elements(&db, [int, str]);
        let int_or_bytes = UnionType::from_elements(&db, [int, bytes]);
        let intersection = IntersectionBuilder::new(&db)
            .add_positive(int_or_str)
            .add_positive(int_or_bytes);

        assert_eq!(intersection.intersections.len(), 1);
        assert_eq!(intersection.build(), int);
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
        let bool_value = t_splitter.bool(db) == Truthiness::AlwaysTrue;

        // We add t_object in various orders (in first or second position) in
        // the tests below to ensure that the boolean simplification eliminates
        // everything from the intersection, not just `bool`.
        let t_object = Type::object();
        let t_bool = KnownClass::Bool.to_instance(db);

        let ty = IntersectionBuilder::new(db)
            .add_positive(t_object)
            .add_positive(t_bool)
            .add_negative(t_splitter)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));

        let ty = IntersectionBuilder::new(db)
            .add_positive(t_bool)
            .add_positive(t_object)
            .add_negative(t_splitter)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));

        let ty = IntersectionBuilder::new(db)
            .add_positive(t_object)
            .add_negative(t_splitter)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));

        let ty = IntersectionBuilder::new(db)
            .add_negative(t_splitter)
            .add_positive(t_object)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::bool_literal(!bool_value));
    }

    #[test]
    fn build_intersection_enums() {
        let db = setup_db();

        let safe_uuid_class = known_module_symbol(&db, KnownModule::Uuid, "SafeUUID")
            .place
            .ignore_possibly_undefined()
            .unwrap();

        let literals = enum_member_literals(&db, safe_uuid_class.expect_class_literal(), None)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(literals.len(), 3);

        // SafeUUID.safe
        let l_safe = literals[0];
        assert_eq!(l_safe.expect_enum_literal().name(&db), "safe");
        // SafeUUID.unsafe
        let l_unsafe = literals[1];
        assert_eq!(l_unsafe.expect_enum_literal().name(&db), "unsafe");
        // SafeUUID.unknown
        let l_unknown = literals[2];
        assert_eq!(l_unknown.expect_enum_literal().name(&db), "unknown");

        // The enum itself: SafeUUID
        let safe_uuid = l_safe.expect_enum_literal().enum_class_instance(&db);

        {
            let actual = IntersectionBuilder::new(&db)
                .add_positive(safe_uuid)
                .add_negative(l_safe)
                .build();

            assert_eq!(
                actual.display(&db).to_string(),
                "Literal[SafeUUID.unsafe, SafeUUID.unknown]"
            );
        }
        {
            // Same as above, but with the order reversed
            let actual = IntersectionBuilder::new(&db)
                .add_negative(l_safe)
                .add_positive(safe_uuid)
                .build();

            assert_eq!(
                actual.display(&db).to_string(),
                "Literal[SafeUUID.unsafe, SafeUUID.unknown]"
            );
        }
        {
            // Also the same, but now with a nested intersection
            let actual = IntersectionBuilder::new(&db)
                .add_positive(safe_uuid)
                .add_positive(IntersectionBuilder::new(&db).add_negative(l_safe).build())
                .build();

            assert_eq!(
                actual.display(&db).to_string(),
                "Literal[SafeUUID.unsafe, SafeUUID.unknown]"
            );
        }
        {
            let actual = IntersectionBuilder::new(&db)
                .add_negative(l_safe)
                .add_positive(safe_uuid)
                .add_negative(l_unsafe)
                .build();

            assert_eq!(actual.display(&db).to_string(), "Literal[SafeUUID.unknown]");
        }
    }
}
