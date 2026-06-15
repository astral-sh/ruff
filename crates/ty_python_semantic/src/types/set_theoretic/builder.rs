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

use crate::types::enums::{EnumComplement, enum_metadata};
use crate::types::generics::Specialization;
use crate::types::set_theoretic::expand_intersection_typevars_and_newtypes;
use crate::types::tuple::Tuple;
use crate::types::visitor::{self, TypeVisitor};
use crate::types::{
    BytesLiteralType, ClassLiteral, ClassType, DivergentType, EnumLiteralType, GenericAlias,
    IntersectionType, KnownClass, LiteralValueType, LiteralValueTypeKind,
    NegativeIntersectionElements, NominalInstanceType, StringLiteralType, SubclassOfType, Type,
    TypeVarBoundOrConstraints, UnionType,
    class::{
        DynamicClassAnchor, DynamicClassLiteral, DynamicEnumAnchor, DynamicEnumLiteral,
        DynamicNamedTupleAnchor, DynamicNamedTupleLiteral, DynamicTypedDictAnchor,
        DynamicTypedDictLiteral, EnumSpec, NamedTupleField, NamedTupleSpec,
    },
    typed_dict::{TypedDictOpenness, TypedDictSchema},
};
use crate::{Db, FxOrderMap, FxOrderSet};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use std::cell::Cell;

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

    let has_not_truthy = intersection.negative(db).contains(&Type::AlwaysTruthy);
    let has_not_falsy = intersection.negative(db).contains(&Type::AlwaysFalsy);
    let guard = match (has_not_truthy, has_not_falsy) {
        (true, false) => falsy,
        (false, true) => truthy,
        _ => return None,
    };

    let mut core = IntersectionBuilder::new(db);
    for positive in intersection.positive(db) {
        core = core.add_positive(*positive);
    }
    for negative in intersection.negative(db) {
        if (guard == falsy && *negative == Type::AlwaysTruthy)
            || (guard == truthy && *negative == Type::AlwaysFalsy)
        {
            continue;
        }
        core = core.add_negative(*negative);
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
        let metadata = enum_metadata(db, enum_class).expect("Enum complement class is an enum");
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

            let Some(canonical_name) = metadata.resolve_member(enum_literal.name(db)) else {
                continue;
            };
            shared_excluded_names.remove(canonical_name);
            remove_indices.push(index);
        }

        if !remove_indices.is_empty() {
            let mut builder =
                IntersectionBuilder::new(db).add_positive(enum_class.to_non_generic_instance(db));
            for rest in complement.rest(db) {
                builder = builder.add_positive(*rest);
            }
            for name in metadata
                .members
                .keys()
                .filter(|name| shared_excluded_names.contains(*name))
            {
                builder = builder.add_negative(Type::enum_literal(EnumLiteralType::new(
                    db,
                    enum_class,
                    name.clone(),
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

struct CycleFusionSummary<'db> {
    recursion_guard: visitor::TypeCollector<'db>,
    has_divergent: Cell<bool>,
    has_recursive: Cell<bool>,
    has_type_alias: Cell<bool>,
}

impl<'db> CycleFusionSummary<'db> {
    fn collect(db: &'db dyn Db, ty: Type<'db>) -> Self {
        let summary = Self {
            recursion_guard: visitor::TypeCollector::default(),
            has_divergent: Cell::new(false),
            has_recursive: Cell::new(false),
            has_type_alias: Cell::new(false),
        };
        summary.visit_type(db, ty);
        summary
    }

    fn is_finite_cycle_fusion_target(&self) -> bool {
        // Lazy attributes can trigger queries, so this summary is intentionally
        // conservative: aliases and recursive types are not expanded in cycle recovery.
        !self.has_divergent.get() && !self.has_recursive.get() && !self.has_type_alias.get()
    }
}

impl<'db> TypeVisitor<'db> for CycleFusionSummary<'db> {
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        match ty {
            Type::Divergent(_) => self.has_divergent.set(true),
            Type::Recursive(_) => self.has_recursive.set(true),
            Type::TypeAlias(_) => self.has_type_alias.set(true),
            _ => visitor::walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard),
        }
    }
}

/// Overlay a finite candidate onto a marker-tainted candidate during cycle recovery.
///
/// For example, `tuple[Divergent(a), int]` overlaid with `tuple[str, int]` becomes
/// `Divergent(a, tuple[str, int])`. This is intentionally limited to the
/// cycle-recovery path: all markers must belong to one binder, and the finite side
/// must already be available without expanding aliases or recursive types.
struct CycleFusionOverlay {
    divergent_id: Cell<Option<salsa::Id>>,
    has_multiple_divergent_ids: Cell<bool>,
}

impl CycleFusionOverlay {
    fn build<'db>(
        db: &'db dyn Db,
        marker_candidate: Type<'db>,
        finite_candidate: Type<'db>,
    ) -> Option<Type<'db>> {
        let overlay = Self {
            divergent_id: Cell::new(None),
            has_multiple_divergent_ids: Cell::new(false),
        };
        let overlaid = overlay.overlay_type(db, marker_candidate, finite_candidate);
        let overlaid = overlaid?;
        let marker_id = overlay.single_divergent_id()?;
        Some(Type::cycle_marked(db, marker_id, overlaid))
    }

    fn add_divergent_id(&self, id: salsa::Id) {
        match self.divergent_id.get() {
            Some(existing) if existing != id => self.has_multiple_divergent_ids.set(true),
            Some(_) => {}
            None => self.divergent_id.set(Some(id)),
        }
    }

    fn single_divergent_id(&self) -> Option<salsa::Id> {
        if self.has_multiple_divergent_ids.get() {
            return None;
        }
        self.divergent_id.get()
    }

    fn overlay_type<'db>(
        &self,
        db: &'db dyn Db,
        marker: Type<'db>,
        finite: Type<'db>,
    ) -> Option<Type<'db>> {
        if let Type::Divergent(marked) = marker
            && let Some(body) = marked.body(db)
        {
            self.add_divergent_id(marked.binder_id(db));
            if let Type::Divergent(finite_marked) = finite
                && finite_marked.binder_id(db) == marked.binder_id(db)
                && finite_marked.body(db).is_some()
            {
                return Some(body);
            }
            return self.overlay_type(db, body, finite);
        }
        if let Type::Divergent(marked) = finite
            && let Some(body) = marked.body(db)
        {
            self.add_divergent_id(marked.binder_id(db));
            return self.overlay_type(db, marker, body);
        }

        if marker == finite {
            return Some(marker);
        }

        match marker {
            Type::Divergent(divergent) => {
                self.add_divergent_id(divergent.id(db));
                // A marker leaf can be replaced only by a finite shape that is
                // already available without expanding aliases or recursive types.
                CycleFusionSummary::collect(db, finite)
                    .is_finite_cycle_fusion_target()
                    .then_some(finite)
            }
            Type::Union(union) => {
                let mut elements = Vec::with_capacity(union.elements(db).len() + 1);
                for element in union.elements(db) {
                    let summary = CycleFusionSummary::collect(db, *element);
                    if summary.has_divergent.get() {
                        elements.push(self.overlay_type(db, *element, finite)?);
                    } else if summary.is_finite_cycle_fusion_target() {
                        elements.push(*element);
                    } else {
                        return None;
                    }
                }
                elements.push(finite);
                Some(UnionType::from_elements(db, elements))
            }
            Type::Recursive(_) | Type::TypeAlias(_) => None,
            _ => {
                if let Some(overlaid_tuple) = self.overlay_tuple(db, marker, finite) {
                    return Some(overlaid_tuple);
                }

                // If both sides are finite, keep both possibilities. The enclosing
                // `CycleMarked` carries the cycle information rather than either branch.
                let marker_summary = CycleFusionSummary::collect(db, marker);
                let finite_summary = CycleFusionSummary::collect(db, finite);
                if marker_summary.is_finite_cycle_fusion_target()
                    && finite_summary.is_finite_cycle_fusion_target()
                {
                    Some(UnionType::from_elements(db, [marker, finite]))
                } else {
                    None
                }
            }
        }
    }

    fn overlay_tuple<'db>(
        &self,
        db: &'db dyn Db,
        marker: Type<'db>,
        finite: Type<'db>,
    ) -> Option<Type<'db>> {
        let marker_tuple = marker.tuple_instance_spec(db)?;
        let finite_tuple = finite.tuple_instance_spec(db)?;

        let (Tuple::Fixed(marker), Tuple::Fixed(finite)) =
            (marker_tuple.as_ref(), finite_tuple.as_ref())
        else {
            return None;
        };

        if marker.len() != finite.len() {
            return None;
        }

        let elements = marker
            .iter_all_elements()
            .zip(finite.iter_all_elements())
            .map(|(marker, finite)| self.overlay_type(db, marker, finite))
            .collect::<Option<Vec<_>>>()?;
        Some(Type::heterogeneous_tuple(db, elements))
    }
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
    fn try_reduce(&mut self, db: &'db dyn Db, other_type: Type<'db>) -> ReduceResult<'db> {
        let mut other_type_negated_cache = None;
        let mut other_type_negated =
            || *other_type_negated_cache.get_or_insert_with(|| other_type.negate(db));

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
            if collapse || other_type_negated().is_subtype_of(db, ty) {
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

/// During cycle recovery, widening is performed from fewer literal elements,
/// resulting in faster convergence of the fixed-point iteration.
const MAX_CYCLE_RECOVERY_UNION_LITERALS: usize = 5;
/// Outside cycle recovery, we avoid eagerly widening smaller literal unions.
/// To avoid unintended huge computational loads, we still limit it to 256.
const MAX_NON_RECURSIVE_UNION_LITERALS: usize = 256;
/// However, we set a much larger limit for enum literals than for other kinds of literals.
/// Huge enums are not uncommon (especially in generated code), and it's annoying
/// if reachability analysis etc. fails when analysing these enums.
const MAX_NON_RECURSIVE_UNION_ENUM_LITERALS: usize = 8192;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RelationSimplification {
    /// Union construction may ask relation queries to remove redundant members.
    Full,
    /// Skip relation checks whose direct operands include a protocol instance.
    ///
    /// Alias-preserving annotation unions can be built while recovering a protocol
    /// interface; opening another protocol interface from that path can form a Salsa
    /// cycle inside the recovery function.
    NoProtocolRelations,
    /// Skip all relation checks.
    ///
    /// Cycle recovery must not introduce fresh query dependencies.
    NoRelations,
}

impl RelationSimplification {
    fn allows_relation<'db>(self, db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> bool {
        match self {
            RelationSimplification::Full => true,
            RelationSimplification::NoProtocolRelations => {
                !Self::relation_may_query_protocol_interface(left, right)
                    && !left.contains_cycle_sensitive_type(db)
                    && !right.contains_cycle_sensitive_type(db)
            }
            RelationSimplification::NoRelations => false,
        }
    }

    fn allows_try_reduce<'db>(self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        match self {
            RelationSimplification::Full => true,
            RelationSimplification::NoProtocolRelations => {
                !matches!(ty, Type::ProtocolInstance(_)) && !ty.contains_cycle_sensitive_type(db)
            }
            RelationSimplification::NoRelations => false,
        }
    }

    fn relation_may_query_protocol_interface<'db>(left: Type<'db>, right: Type<'db>) -> bool {
        matches!(
            (left, right),
            (Type::ProtocolInstance(_), _) | (_, Type::ProtocolInstance(_))
        )
    }
}

pub(crate) struct UnionBuilder<'db> {
    elements: Vec<UnionElement<'db>>,
    db: &'db dyn Db,
    unpack_aliases: bool,
    relation_simplification: RelationSimplification,
    /// This is enabled when joining types in a `cycle_recovery` function.
    cycle_recovery: bool,
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
        Self {
            db,
            elements: vec![],
            unpack_aliases: true,
            relation_simplification: RelationSimplification::Full,
            cycle_recovery: false,
        }
    }

    pub(crate) fn unpack_aliases(mut self, val: bool) -> Self {
        self.unpack_aliases = val;
        self
    }

    fn with_relation_simplification(
        mut self,
        relation_simplification: RelationSimplification,
    ) -> Self {
        self.relation_simplification = relation_simplification;
        self
    }

    pub(crate) fn without_protocol_relation_simplification(mut self) -> Self {
        if self.relation_simplification != RelationSimplification::NoRelations {
            self.relation_simplification = RelationSimplification::NoProtocolRelations;
        }
        self
    }

    pub(crate) fn cycle_recovery(mut self, val: bool) -> Self {
        self.cycle_recovery = val;
        if self.cycle_recovery {
            self.unpack_aliases = false;
            self.relation_simplification = RelationSimplification::NoProtocolRelations;
        }
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

    fn widen_literal_types(&mut self, seen_aliases: &mut Vec<Type<'db>>) {
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

    /// Adds a type to this union.
    pub(crate) fn add(mut self, ty: Type<'db>) -> Self {
        self.add_in_place(ty);
        self
    }

    /// Adds a type to this union.
    pub(crate) fn add_in_place(&mut self, ty: Type<'db>) {
        self.add_in_place_impl(ty, &mut vec![]);
    }

    pub(crate) fn add_in_place_impl(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        let cycle_recovery = self.cycle_recovery;
        let relation_simplification = self.relation_simplification;
        let should_widen = |literals| {
            if cycle_recovery {
                literals >= MAX_CYCLE_RECOVERY_UNION_LITERALS
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
                    self.add_in_place_impl(*element, seen_aliases);
                }
                if self.cycle_recovery {
                    let literals = self.elements.iter().fold(0, |acc, elem| match elem {
                        UnionElement::IntLiterals(literals) => acc + literals.len(),
                        UnionElement::StringLiterals(literals) => acc + literals.len(),
                        UnionElement::BytesLiterals(literals) => acc + literals.len(),
                        UnionElement::EnumLiterals { literals, .. } => acc + literals.len(),
                        UnionElement::Type(_) => acc,
                    });
                    if should_widen(literals) {
                        self.widen_literal_types(seen_aliases);
                    }
                }
            }
            // Adding `Never` to a union is a no-op.
            Type::Never => {}
            Type::TypeAlias(alias) if self.unpack_aliases => {
                self.add_in_place_impl(alias.value_type(self.db), seen_aliases);
            }
            Type::LiteralValue(literal) => {
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
                                    if should_widen(literals.len()) {
                                        let replace_with = KnownClass::Str.to_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing) => {
                                    if !relation_simplification
                                        .allows_relation(self.db, ty, *existing)
                                    {
                                        continue;
                                    }
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
                                    if should_widen(literals.len()) {
                                        let replace_with = KnownClass::Bytes.to_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing) => {
                                    if !relation_simplification
                                        .allows_relation(self.db, ty, *existing)
                                    {
                                        continue;
                                    }
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
                                    if should_widen(literals.len()) {
                                        let replace_with = KnownClass::Int.to_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing) => {
                                    if !relation_simplification
                                        .allows_relation(self.db, ty, *existing)
                                    {
                                        continue;
                                    }
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
                        let enum_class = enum_member_to_add.enum_class(self.db);

                        // We generally expect that a `Type::LiteralValue(LiteralValueTypeKind::Enum)`
                        // value is in fact in enum, i.e., that `enum_metadata` returns `Some(...)`.
                        // However, during cycle recovery, it's possible (empirically) to end up
                        // in an inconsistent state. The metadata is only required for simplification
                        // and not for correctness, so we treat it as optional here.
                        // TODO: Come up with a design, either to enum metadata or the cycle
                        // handling more broadly, that avoids this inconsistency.
                        let metadata = enum_metadata(self.db, enum_class);

                        if metadata.is_some_and(|metadata| metadata.members.len() == 1) {
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
                                    // See the doc-comment above `MAX_NON_RECURSIVE_UNION_ENUM_LITERALS`
                                    // for why we avoid using the `should_widen` closure here.
                                    let enum_literals_limit = if cycle_recovery {
                                        MAX_CYCLE_RECOVERY_UNION_LITERALS
                                    } else {
                                        MAX_NON_RECURSIVE_UNION_ENUM_LITERALS
                                    };
                                    if literals.len() >= enum_literals_limit {
                                        let (literal, _) = literals.first().unwrap();
                                        let replace_with = literal.enum_class_instance(self.db);
                                        self.add_in_place_impl(replace_with, seen_aliases);
                                        return;
                                    }
                                    found = Some(literals);
                                    continue;
                                }
                                UnionElement::Type(existing) => {
                                    if !relation_simplification
                                        .allows_relation(self.db, ty, *existing)
                                    {
                                        continue;
                                    }
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

                                    if metadata.is_some_and(|metadata| {
                                        found.len() == metadata.members.len()
                                    }) {
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
            ty if ty.is_object() => self.collapse_to_object(),
            _ => self.push_type(ty, seen_aliases),
        }
    }

    fn cycle_marked_represents_same_type(
        db: &'db dyn Db,
        relation_simplification: RelationSimplification,
        preferred: Type<'db>,
        other: Type<'db>,
    ) -> bool {
        // `Divergent(id, Some(T))` and `T` are equivalent, but the marked representative
        // must survive so later cycle recovery can still see the binder.
        if !preferred.contains_cycle_marked(db) {
            return false;
        }
        let other_contains_cycle_marked = other.contains_cycle_marked(db);
        let preferred = preferred.erase_cycle_marks(db);
        let other = other.erase_cycle_marks(db);
        if preferred == other {
            return true;
        }
        !other_contains_cycle_marked
            && relation_simplification.allows_relation(db, preferred, other)
            && (preferred.is_equivalent_to(db, other)
                || preferred.is_assignable_to(db, other) && other.is_assignable_to(db, preferred))
    }

    fn cycle_fusion_candidate(
        db: &'db dyn Db,
        cycle_recovery: bool,
        marker_candidate: Type<'db>,
        finite_candidate: Type<'db>,
    ) -> Option<Type<'db>> {
        if !cycle_recovery {
            return None;
        }

        CycleFusionOverlay::build(db, marker_candidate, finite_candidate)
    }

    fn merge_matching_cycle_marked(
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        let (Type::Divergent(left), Type::Divergent(right)) = (left, right) else {
            return None;
        };
        if left.binder_id(db) != right.binder_id(db) {
            return None;
        }

        match (left.body(db), right.body(db)) {
            (Some(left_body), Some(right_body)) => Some(Type::cycle_marked(
                db,
                left.binder_id(db),
                UnionType::from_elements_cycle_recovery(db, [left_body, right_body]),
            )),
            (Some(_), None) => Some(Type::Divergent(left)),
            (None, Some(_)) => Some(Type::Divergent(right)),
            (None, None) => Some(Type::Divergent(left)),
        }
    }

    fn merge_cycle_recovery_type(db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> Type<'db> {
        UnionType::from_elements_cycle_recovery(db, [left, right])
    }

    fn merge_cycle_recovery_type_slices(
        db: &'db dyn Db,
        left: &[Type<'db>],
        right: &[Type<'db>],
    ) -> Option<Box<[Type<'db>]>> {
        (left.len() == right.len()).then(|| {
            left.iter()
                .zip(right)
                .map(|(left, right)| Self::merge_cycle_recovery_type(db, *left, *right))
                .collect()
        })
    }

    fn merge_dynamic_class_members(
        db: &'db dyn Db,
        left: &[(ruff_python_ast::name::Name, Type<'db>)],
        right: &[(ruff_python_ast::name::Name, Type<'db>)],
    ) -> Box<[(ruff_python_ast::name::Name, Type<'db>)]> {
        let mut merged = left.to_vec();
        for (right_name, right_ty) in right {
            if let Some((_, left_ty)) = merged
                .iter_mut()
                .find(|(left_name, _)| left_name == right_name)
            {
                *left_ty = Self::merge_cycle_recovery_type(db, *left_ty, *right_ty);
            } else {
                merged.push((right_name.clone(), *right_ty));
            }
        }
        merged.into_boxed_slice()
    }

    fn merge_dynamic_class_anchor(
        db: &'db dyn Db,
        left: &DynamicClassAnchor<'db>,
        right: &DynamicClassAnchor<'db>,
    ) -> Option<DynamicClassAnchor<'db>> {
        match (left, right) {
            (DynamicClassAnchor::Definition(left), DynamicClassAnchor::Definition(right))
                if left == right =>
            {
                Some(DynamicClassAnchor::Definition(*left))
            }
            (
                DynamicClassAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    explicit_bases: left_bases,
                },
                DynamicClassAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    explicit_bases: right_bases,
                },
            ) if left_scope == right_scope && left_offset == right_offset => {
                Some(DynamicClassAnchor::ScopeOffset {
                    scope: *left_scope,
                    offset: *left_offset,
                    explicit_bases: Self::merge_cycle_recovery_type_slices(
                        db,
                        left_bases,
                        right_bases,
                    )?,
                })
            }
            _ => None,
        }
    }

    fn same_dynamic_class_anchor_origin(
        left: &DynamicClassAnchor<'db>,
        right: &DynamicClassAnchor<'db>,
    ) -> bool {
        match (left, right) {
            (DynamicClassAnchor::Definition(left), DynamicClassAnchor::Definition(right)) => {
                left == right
            }
            (
                DynamicClassAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    ..
                },
                DynamicClassAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    ..
                },
            ) => left_scope == right_scope && left_offset == right_offset,
            _ => false,
        }
    }

    fn same_dynamic_named_tuple_anchor_origin(
        left: &DynamicNamedTupleAnchor<'db>,
        right: &DynamicNamedTupleAnchor<'db>,
    ) -> bool {
        match (left, right) {
            (
                DynamicNamedTupleAnchor::CollectionsDefinition {
                    definition: left, ..
                },
                DynamicNamedTupleAnchor::CollectionsDefinition {
                    definition: right, ..
                },
            ) => left == right,
            (
                DynamicNamedTupleAnchor::TypingDefinition(left),
                DynamicNamedTupleAnchor::TypingDefinition(right),
            ) => left == right,
            (
                DynamicNamedTupleAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    ..
                },
                DynamicNamedTupleAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    ..
                },
            ) => left_scope == right_scope && left_offset == right_offset,
            _ => false,
        }
    }

    fn same_dynamic_typed_dict_anchor_origin(
        left: &DynamicTypedDictAnchor<'db>,
        right: &DynamicTypedDictAnchor<'db>,
    ) -> bool {
        match (left, right) {
            (
                DynamicTypedDictAnchor::Definition(left),
                DynamicTypedDictAnchor::Definition(right),
            ) => left == right,
            (
                DynamicTypedDictAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    ..
                },
                DynamicTypedDictAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    ..
                },
            ) => left_scope == right_scope && left_offset == right_offset,
            _ => false,
        }
    }

    fn same_dynamic_enum_anchor_origin(
        left: &DynamicEnumAnchor<'db>,
        right: &DynamicEnumAnchor<'db>,
    ) -> bool {
        match (left, right) {
            (
                DynamicEnumAnchor::Definition {
                    definition: left, ..
                },
                DynamicEnumAnchor::Definition {
                    definition: right, ..
                },
            ) => left == right,
            (
                DynamicEnumAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    ..
                },
                DynamicEnumAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    ..
                },
            ) => left_scope == right_scope && left_offset == right_offset,
            _ => false,
        }
    }

    fn same_dynamic_class_literal_origin(
        db: &'db dyn Db,
        left: ClassLiteral<'db>,
        right: ClassLiteral<'db>,
    ) -> bool {
        match (left, right) {
            (ClassLiteral::Dynamic(left), ClassLiteral::Dynamic(right)) => {
                left.name(db) == right.name(db)
                    && Self::same_dynamic_class_anchor_origin(left.anchor(db), right.anchor(db))
            }
            (ClassLiteral::DynamicNamedTuple(left), ClassLiteral::DynamicNamedTuple(right)) => {
                left.name(db) == right.name(db)
                    && Self::same_dynamic_named_tuple_anchor_origin(
                        left.anchor(db),
                        right.anchor(db),
                    )
            }
            (ClassLiteral::DynamicTypedDict(left), ClassLiteral::DynamicTypedDict(right)) => {
                left.name(db) == right.name(db)
                    && Self::same_dynamic_typed_dict_anchor_origin(
                        left.anchor(db),
                        right.anchor(db),
                    )
            }
            (ClassLiteral::DynamicEnum(left), ClassLiteral::DynamicEnum(right)) => {
                left.name(db) == right.name(db)
                    && left.base_class(db) == right.base_class(db)
                    && Self::same_dynamic_enum_anchor_origin(left.anchor(db), right.anchor(db))
            }
            _ => false,
        }
    }

    fn widen_dynamic_origin_in_type(
        db: &'db dyn Db,
        ty: Type<'db>,
        origin: ClassLiteral<'db>,
    ) -> Type<'db> {
        match ty {
            Type::ClassLiteral(class) => {
                if Self::same_dynamic_class_literal_origin(db, class, origin) {
                    return KnownClass::Type.to_instance(db);
                }
                Type::ClassLiteral(Self::widen_dynamic_origin_in_class_literal(
                    db, class, origin,
                ))
            }
            Type::GenericAlias(alias) => {
                let origin_class = alias.origin(db);
                if origin_class.is_tuple(db) {
                    return ty;
                }
                let specialization = Self::widen_dynamic_origin_in_specialization(
                    db,
                    alias.specialization(db),
                    origin,
                );
                Type::GenericAlias(GenericAlias::new(db, origin_class, specialization))
            }
            Type::NominalInstance(instance) => {
                let class = instance.class(db);
                if class.known(db) == Some(KnownClass::Tuple) {
                    return ty;
                }
                let widened = Self::widen_dynamic_origin_in_class_type(db, class, origin);
                if widened == class {
                    ty
                } else {
                    Type::instance(db, widened)
                }
            }
            Type::Union(union) => UnionType::from_elements_cycle_recovery(
                db,
                union
                    .elements(db)
                    .iter()
                    .map(|element| Self::widen_dynamic_origin_in_type(db, *element, origin)),
            ),
            Type::Divergent(divergent) if let Some(body) = divergent.body(db) => {
                Type::cycle_marked(
                    db,
                    divergent.binder_id(db),
                    Self::widen_dynamic_origin_in_type(db, body, origin),
                )
            }
            _ => ty,
        }
    }

    fn widen_dynamic_origin_in_class_type(
        db: &'db dyn Db,
        class: ClassType<'db>,
        origin: ClassLiteral<'db>,
    ) -> ClassType<'db> {
        match class {
            ClassType::NonGeneric(class) => ClassType::NonGeneric(
                Self::widen_dynamic_origin_in_class_literal(db, class, origin),
            ),
            ClassType::Generic(alias) => {
                let origin_class = alias.origin(db);
                if origin_class.is_tuple(db) {
                    class
                } else {
                    ClassType::Generic(GenericAlias::new(
                        db,
                        origin_class,
                        Self::widen_dynamic_origin_in_specialization(
                            db,
                            alias.specialization(db),
                            origin,
                        ),
                    ))
                }
            }
        }
    }

    fn widen_dynamic_origin_in_specialization(
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        origin: ClassLiteral<'db>,
    ) -> Specialization<'db> {
        let types: Box<_> = specialization
            .types(db)
            .iter()
            .map(|ty| Self::widen_dynamic_origin_in_type(db, *ty, origin))
            .collect();
        if types.as_ref() == specialization.types(db) {
            specialization
        } else {
            Specialization::new(
                db,
                specialization.generic_context(db),
                types,
                specialization.materialization_kind(db),
                None,
            )
        }
    }

    fn widen_dynamic_origin_in_class_literal(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        origin: ClassLiteral<'db>,
    ) -> ClassLiteral<'db> {
        match class {
            ClassLiteral::Dynamic(dynamic) => ClassLiteral::Dynamic(
                Self::widen_dynamic_origin_in_dynamic_class(db, dynamic, origin),
            ),
            ClassLiteral::DynamicNamedTuple(named_tuple) => ClassLiteral::DynamicNamedTuple(
                Self::widen_dynamic_origin_in_dynamic_named_tuple(db, named_tuple, origin),
            ),
            ClassLiteral::DynamicTypedDict(typed_dict) => ClassLiteral::DynamicTypedDict(
                Self::widen_dynamic_origin_in_dynamic_typed_dict(db, typed_dict, origin),
            ),
            ClassLiteral::DynamicEnum(enum_literal) => ClassLiteral::DynamicEnum(
                Self::widen_dynamic_origin_in_dynamic_enum(db, enum_literal, origin),
            ),
            ClassLiteral::Static(_) => class,
        }
    }

    fn widen_dynamic_origin_in_dynamic_class_anchor(
        db: &'db dyn Db,
        anchor: &DynamicClassAnchor<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicClassAnchor<'db> {
        match anchor {
            DynamicClassAnchor::Definition(definition) => {
                DynamicClassAnchor::Definition(*definition)
            }
            DynamicClassAnchor::ScopeOffset {
                scope,
                offset,
                explicit_bases,
            } => DynamicClassAnchor::ScopeOffset {
                scope: *scope,
                offset: *offset,
                explicit_bases: explicit_bases
                    .iter()
                    .map(|base| Self::widen_dynamic_origin_in_type(db, *base, origin))
                    .collect(),
            },
        }
    }

    fn widen_dynamic_origin_in_dynamic_class(
        db: &'db dyn Db,
        class: DynamicClassLiteral<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicClassLiteral<'db> {
        DynamicClassLiteral::new(
            db,
            class.name(db).clone(),
            Self::widen_dynamic_origin_in_dynamic_class_anchor(db, class.anchor(db), origin),
            class
                .members(db)
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        Self::widen_dynamic_origin_in_type(db, *ty, origin),
                    )
                })
                .collect::<Box<_>>(),
            class.has_dynamic_namespace(db),
            class.dataclass_params(db),
        )
    }

    fn widen_dynamic_origin_in_named_tuple_spec(
        db: &'db dyn Db,
        spec: NamedTupleSpec<'db>,
        origin: ClassLiteral<'db>,
    ) -> NamedTupleSpec<'db> {
        if !spec.has_known_fields(db) {
            return spec;
        }

        let fields: Box<_> = spec
            .fields(db)
            .iter()
            .map(|field| NamedTupleField {
                name: field.name.clone(),
                ty: Self::widen_dynamic_origin_in_type(db, field.ty, origin),
                default: field
                    .default
                    .map(|default| Self::widen_dynamic_origin_in_type(db, default, origin)),
                definition: field.definition,
            })
            .collect();
        if fields.as_ref() == spec.fields(db) {
            spec
        } else {
            NamedTupleSpec::known(db, fields)
        }
    }

    fn widen_dynamic_origin_in_named_tuple_anchor(
        db: &'db dyn Db,
        anchor: &DynamicNamedTupleAnchor<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicNamedTupleAnchor<'db> {
        match anchor {
            DynamicNamedTupleAnchor::CollectionsDefinition { definition, spec } => {
                DynamicNamedTupleAnchor::CollectionsDefinition {
                    definition: *definition,
                    spec: Self::widen_dynamic_origin_in_named_tuple_spec(db, *spec, origin),
                }
            }
            DynamicNamedTupleAnchor::TypingDefinition(definition) => {
                DynamicNamedTupleAnchor::TypingDefinition(*definition)
            }
            DynamicNamedTupleAnchor::ScopeOffset {
                scope,
                offset,
                spec,
            } => DynamicNamedTupleAnchor::ScopeOffset {
                scope: *scope,
                offset: *offset,
                spec: Self::widen_dynamic_origin_in_named_tuple_spec(db, *spec, origin),
            },
        }
    }

    fn widen_dynamic_origin_in_dynamic_named_tuple(
        db: &'db dyn Db,
        named_tuple: DynamicNamedTupleLiteral<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicNamedTupleLiteral<'db> {
        DynamicNamedTupleLiteral::new(
            db,
            named_tuple.name(db).clone(),
            Self::widen_dynamic_origin_in_named_tuple_anchor(db, named_tuple.anchor(db), origin),
        )
    }

    fn widen_dynamic_origin_in_typed_dict_schema(
        db: &'db dyn Db,
        schema: &TypedDictSchema<'db>,
        origin: ClassLiteral<'db>,
    ) -> TypedDictSchema<'db> {
        schema
            .iter()
            .map(|(name, field)| {
                let mut field = field.clone();
                field.declared_ty =
                    Self::widen_dynamic_origin_in_type(db, field.declared_ty, origin);
                (name.clone(), field)
            })
            .collect()
    }

    fn widen_dynamic_origin_in_typed_dict_openness(
        db: &'db dyn Db,
        openness: TypedDictOpenness<'db>,
        origin: ClassLiteral<'db>,
    ) -> TypedDictOpenness<'db> {
        match openness {
            TypedDictOpenness::Extra(extra) => TypedDictOpenness::extra(
                db,
                Self::widen_dynamic_origin_in_type(db, extra.declared_ty, origin),
                extra.is_read_only(),
            ),
            TypedDictOpenness::ImplicitlyOpen | TypedDictOpenness::Closed => openness,
        }
    }

    fn widen_dynamic_origin_in_typed_dict_anchor(
        db: &'db dyn Db,
        anchor: &DynamicTypedDictAnchor<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicTypedDictAnchor<'db> {
        match anchor {
            DynamicTypedDictAnchor::Definition(definition) => {
                DynamicTypedDictAnchor::Definition(*definition)
            }
            DynamicTypedDictAnchor::ScopeOffset {
                scope,
                offset,
                schema,
                openness,
            } => DynamicTypedDictAnchor::ScopeOffset {
                scope: *scope,
                offset: *offset,
                schema: Self::widen_dynamic_origin_in_typed_dict_schema(db, schema, origin),
                openness: Self::widen_dynamic_origin_in_typed_dict_openness(db, *openness, origin),
            },
        }
    }

    fn widen_dynamic_origin_in_dynamic_typed_dict(
        db: &'db dyn Db,
        typed_dict: DynamicTypedDictLiteral<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicTypedDictLiteral<'db> {
        DynamicTypedDictLiteral::new(
            db,
            typed_dict.name(db).clone(),
            Self::widen_dynamic_origin_in_typed_dict_anchor(db, typed_dict.anchor(db), origin),
        )
    }

    fn widen_dynamic_origin_in_enum_spec(
        db: &'db dyn Db,
        spec: EnumSpec<'db>,
        origin: ClassLiteral<'db>,
    ) -> EnumSpec<'db> {
        if !spec.has_known_members(db) {
            return spec;
        }

        let members: Box<_> = spec
            .members(db)
            .iter()
            .map(|(name, ty)| {
                (
                    name.clone(),
                    Self::widen_dynamic_origin_in_type(db, *ty, origin),
                )
            })
            .collect();
        if members.as_ref() == spec.members(db) {
            spec
        } else {
            EnumSpec::new(db, members, true)
        }
    }

    fn widen_dynamic_origin_in_enum_anchor(
        db: &'db dyn Db,
        anchor: &DynamicEnumAnchor<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicEnumAnchor<'db> {
        match anchor {
            DynamicEnumAnchor::Definition { definition, spec } => DynamicEnumAnchor::Definition {
                definition: *definition,
                spec: Self::widen_dynamic_origin_in_enum_spec(db, *spec, origin),
            },
            DynamicEnumAnchor::ScopeOffset {
                scope,
                offset,
                spec,
            } => DynamicEnumAnchor::ScopeOffset {
                scope: *scope,
                offset: *offset,
                spec: Self::widen_dynamic_origin_in_enum_spec(db, *spec, origin),
            },
        }
    }

    fn widen_dynamic_origin_in_dynamic_enum(
        db: &'db dyn Db,
        enum_literal: DynamicEnumLiteral<'db>,
        origin: ClassLiteral<'db>,
    ) -> DynamicEnumLiteral<'db> {
        DynamicEnumLiteral::new(
            db,
            enum_literal.name(db).clone(),
            Self::widen_dynamic_origin_in_enum_anchor(db, enum_literal.anchor(db), origin),
            enum_literal.base_class(db),
            enum_literal
                .mixin_type(db)
                .map(|mixin| Self::widen_dynamic_origin_in_type(db, mixin, origin)),
        )
    }

    fn merge_dynamic_class(
        db: &'db dyn Db,
        left: DynamicClassLiteral<'db>,
        right: DynamicClassLiteral<'db>,
    ) -> Option<DynamicClassLiteral<'db>> {
        if left.name(db) != right.name(db)
            || left.dataclass_params(db) != right.dataclass_params(db)
        {
            return None;
        }

        let origin = ClassLiteral::Dynamic(left);
        let anchor = Self::merge_dynamic_class_anchor(db, left.anchor(db), right.anchor(db))?;
        let anchor = Self::widen_dynamic_origin_in_dynamic_class_anchor(db, &anchor, origin);
        let members: Box<_> =
            Self::merge_dynamic_class_members(db, left.members(db), right.members(db))
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        Self::widen_dynamic_origin_in_type(db, *ty, origin),
                    )
                })
                .collect();

        Some(DynamicClassLiteral::new(
            db,
            left.name(db).clone(),
            anchor,
            members,
            left.has_dynamic_namespace(db) || right.has_dynamic_namespace(db),
            left.dataclass_params(db),
        ))
    }

    fn merge_named_tuple_specs(
        db: &'db dyn Db,
        left: NamedTupleSpec<'db>,
        right: NamedTupleSpec<'db>,
    ) -> NamedTupleSpec<'db> {
        if !left.has_known_fields(db) || !right.has_known_fields(db) {
            return NamedTupleSpec::unknown(db);
        }

        let left_fields = left.fields(db);
        let right_fields = right.fields(db);
        if left_fields.len() != right_fields.len() {
            return NamedTupleSpec::unknown(db);
        }

        let mut fields = Vec::with_capacity(left_fields.len());
        for (left_field, right_field) in left_fields.iter().zip(right_fields) {
            if left_field.name != right_field.name
                || left_field.definition != right_field.definition
            {
                return NamedTupleSpec::unknown(db);
            }

            let default = match (left_field.default, right_field.default) {
                (Some(left), Some(right)) => Some(Self::merge_cycle_recovery_type(db, left, right)),
                (Some(default), None) | (None, Some(default)) => Some(default),
                (None, None) => None,
            };

            fields.push(NamedTupleField {
                name: left_field.name.clone(),
                ty: Self::merge_cycle_recovery_type(db, left_field.ty, right_field.ty),
                default,
                definition: left_field.definition,
            });
        }

        NamedTupleSpec::known(db, fields.into_boxed_slice())
    }

    fn merge_dynamic_named_tuple_anchor(
        db: &'db dyn Db,
        left: &DynamicNamedTupleAnchor<'db>,
        right: &DynamicNamedTupleAnchor<'db>,
    ) -> Option<DynamicNamedTupleAnchor<'db>> {
        match (left, right) {
            (
                DynamicNamedTupleAnchor::CollectionsDefinition {
                    definition: left_definition,
                    spec: left_spec,
                },
                DynamicNamedTupleAnchor::CollectionsDefinition {
                    definition: right_definition,
                    spec: right_spec,
                },
            ) if left_definition == right_definition => {
                Some(DynamicNamedTupleAnchor::CollectionsDefinition {
                    definition: *left_definition,
                    spec: Self::merge_named_tuple_specs(db, *left_spec, *right_spec),
                })
            }
            (
                DynamicNamedTupleAnchor::TypingDefinition(left),
                DynamicNamedTupleAnchor::TypingDefinition(right),
            ) if left == right => Some(DynamicNamedTupleAnchor::TypingDefinition(*left)),
            (
                DynamicNamedTupleAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    spec: left_spec,
                },
                DynamicNamedTupleAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    spec: right_spec,
                },
            ) if left_scope == right_scope && left_offset == right_offset => {
                Some(DynamicNamedTupleAnchor::ScopeOffset {
                    scope: *left_scope,
                    offset: *left_offset,
                    spec: Self::merge_named_tuple_specs(db, *left_spec, *right_spec),
                })
            }
            _ => None,
        }
    }

    fn merge_dynamic_named_tuple(
        db: &'db dyn Db,
        left: DynamicNamedTupleLiteral<'db>,
        right: DynamicNamedTupleLiteral<'db>,
    ) -> Option<DynamicNamedTupleLiteral<'db>> {
        if left.name(db) != right.name(db) {
            return None;
        }

        Some(DynamicNamedTupleLiteral::new(
            db,
            left.name(db).clone(),
            Self::merge_dynamic_named_tuple_anchor(db, left.anchor(db), right.anchor(db))?,
        ))
    }

    fn merge_typed_dict_schemas(
        db: &'db dyn Db,
        left: &TypedDictSchema<'db>,
        right: &'db TypedDictSchema<'db>,
    ) -> TypedDictSchema<'db> {
        let mut merged = left.clone();
        for (name, right_field) in right {
            if let Some(left_field) = merged.get_mut(name) {
                left_field.declared_ty = Self::merge_cycle_recovery_type(
                    db,
                    left_field.declared_ty,
                    right_field.declared_ty,
                );
            } else {
                merged.insert(name.clone(), right_field.clone());
            }
        }
        merged
    }

    fn merge_typed_dict_openness(
        db: &'db dyn Db,
        left: TypedDictOpenness<'db>,
        right: TypedDictOpenness<'db>,
    ) -> TypedDictOpenness<'db> {
        match (left, right) {
            _ if left == right => left,
            (TypedDictOpenness::Extra(left), TypedDictOpenness::Extra(right))
                if left.is_read_only() == right.is_read_only() =>
            {
                TypedDictOpenness::extra(
                    db,
                    Self::merge_cycle_recovery_type(db, left.declared_ty, right.declared_ty),
                    left.is_read_only(),
                )
            }
            _ => TypedDictOpenness::ImplicitlyOpen,
        }
    }

    fn merge_dynamic_typed_dict_anchor(
        db: &'db dyn Db,
        left: &DynamicTypedDictAnchor<'db>,
        right: &'db DynamicTypedDictAnchor<'db>,
    ) -> Option<DynamicTypedDictAnchor<'db>> {
        match (left, right) {
            (
                DynamicTypedDictAnchor::Definition(left),
                DynamicTypedDictAnchor::Definition(right),
            ) if left == right => Some(DynamicTypedDictAnchor::Definition(*left)),
            (
                DynamicTypedDictAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    schema: left_schema,
                    openness: left_openness,
                },
                DynamicTypedDictAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    schema: right_schema,
                    openness: right_openness,
                },
            ) if left_scope == right_scope && left_offset == right_offset => {
                Some(DynamicTypedDictAnchor::ScopeOffset {
                    scope: *left_scope,
                    offset: *left_offset,
                    schema: Self::merge_typed_dict_schemas(db, left_schema, right_schema),
                    openness: Self::merge_typed_dict_openness(db, *left_openness, *right_openness),
                })
            }
            _ => None,
        }
    }

    fn merge_dynamic_typed_dict(
        db: &'db dyn Db,
        left: DynamicTypedDictLiteral<'db>,
        right: DynamicTypedDictLiteral<'db>,
    ) -> Option<DynamicTypedDictLiteral<'db>> {
        if left.name(db) != right.name(db) {
            return None;
        }

        Some(DynamicTypedDictLiteral::new(
            db,
            left.name(db).clone(),
            Self::merge_dynamic_typed_dict_anchor(db, left.anchor(db), right.anchor(db))?,
        ))
    }

    fn merge_enum_specs(
        db: &'db dyn Db,
        left: EnumSpec<'db>,
        right: EnumSpec<'db>,
    ) -> EnumSpec<'db> {
        if !left.has_known_members(db) || !right.has_known_members(db) {
            return EnumSpec::new(db, Box::default(), false);
        }

        let mut members = left.members(db).to_vec();
        for (right_name, right_ty) in right.members(db) {
            if let Some((_, left_ty)) = members
                .iter_mut()
                .find(|(left_name, _)| left_name == right_name)
            {
                *left_ty = Self::merge_cycle_recovery_type(db, *left_ty, *right_ty);
            } else {
                members.push((right_name.clone(), *right_ty));
            }
        }

        EnumSpec::new(db, members.into_boxed_slice(), true)
    }

    fn merge_dynamic_enum_anchor(
        db: &'db dyn Db,
        left: &DynamicEnumAnchor<'db>,
        right: &DynamicEnumAnchor<'db>,
    ) -> Option<DynamicEnumAnchor<'db>> {
        match (left, right) {
            (
                DynamicEnumAnchor::Definition {
                    definition: left_definition,
                    spec: left_spec,
                },
                DynamicEnumAnchor::Definition {
                    definition: right_definition,
                    spec: right_spec,
                },
            ) if left_definition == right_definition => Some(DynamicEnumAnchor::Definition {
                definition: *left_definition,
                spec: Self::merge_enum_specs(db, *left_spec, *right_spec),
            }),
            (
                DynamicEnumAnchor::ScopeOffset {
                    scope: left_scope,
                    offset: left_offset,
                    spec: left_spec,
                },
                DynamicEnumAnchor::ScopeOffset {
                    scope: right_scope,
                    offset: right_offset,
                    spec: right_spec,
                },
            ) if left_scope == right_scope && left_offset == right_offset => {
                Some(DynamicEnumAnchor::ScopeOffset {
                    scope: *left_scope,
                    offset: *left_offset,
                    spec: Self::merge_enum_specs(db, *left_spec, *right_spec),
                })
            }
            _ => None,
        }
    }

    fn merge_dynamic_enum(
        db: &'db dyn Db,
        left: DynamicEnumLiteral<'db>,
        right: DynamicEnumLiteral<'db>,
    ) -> Option<DynamicEnumLiteral<'db>> {
        if left.name(db) != right.name(db) || left.base_class(db) != right.base_class(db) {
            return None;
        }

        let mixin_type = match (left.mixin_type(db), right.mixin_type(db)) {
            (Some(left), Some(right)) => Some(Self::merge_cycle_recovery_type(db, left, right)),
            (Some(mixin), None) | (None, Some(mixin)) => Some(mixin),
            (None, None) => None,
        };

        Some(DynamicEnumLiteral::new(
            db,
            left.name(db).clone(),
            Self::merge_dynamic_enum_anchor(db, left.anchor(db), right.anchor(db))?,
            left.base_class(db),
            mixin_type,
        ))
    }

    fn merge_same_dynamic_class_origin(
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        let (Type::ClassLiteral(left), Type::ClassLiteral(right)) = (left, right) else {
            return None;
        };

        let merged = match (left, right) {
            (ClassLiteral::Dynamic(left), ClassLiteral::Dynamic(right)) => {
                ClassLiteral::Dynamic(Self::merge_dynamic_class(db, left, right)?)
            }
            (ClassLiteral::DynamicNamedTuple(left), ClassLiteral::DynamicNamedTuple(right)) => {
                ClassLiteral::DynamicNamedTuple(Self::merge_dynamic_named_tuple(db, left, right)?)
            }
            (ClassLiteral::DynamicTypedDict(left), ClassLiteral::DynamicTypedDict(right)) => {
                ClassLiteral::DynamicTypedDict(Self::merge_dynamic_typed_dict(db, left, right)?)
            }
            (ClassLiteral::DynamicEnum(left), ClassLiteral::DynamicEnum(right)) => {
                ClassLiteral::DynamicEnum(Self::merge_dynamic_enum(db, left, right)?)
            }
            _ => return None,
        };

        Some(Type::ClassLiteral(merged))
    }

    fn contains_dynamic_class_literal(db: &'db dyn Db, ty: Type<'db>) -> bool {
        match ty {
            Type::ClassLiteral(
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_),
            ) => true,
            Type::Union(union) => union
                .elements(db)
                .iter()
                .any(|element| Self::contains_dynamic_class_literal(db, *element)),
            Type::GenericAlias(alias) => alias
                .specialization(db)
                .types(db)
                .iter()
                .any(|ty| Self::contains_dynamic_class_literal(db, *ty)),
            Type::NominalInstance(instance) => match instance.class(db) {
                ClassType::Generic(alias) => alias
                    .specialization(db)
                    .types(db)
                    .iter()
                    .any(|ty| Self::contains_dynamic_class_literal(db, *ty)),
                ClassType::NonGeneric(class) => {
                    Self::contains_dynamic_class_literal(db, Type::ClassLiteral(class))
                }
            },
            Type::Divergent(divergent) if let Some(body) = divergent.body(db) => {
                Self::contains_dynamic_class_literal(db, body)
            }
            _ => false,
        }
    }

    fn merge_specializations(
        db: &'db dyn Db,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> Option<Specialization<'db>> {
        if left.generic_context(db) != right.generic_context(db)
            || left.materialization_kind(db) != right.materialization_kind(db)
        {
            return None;
        }

        let left_types = left.types(db);
        let right_types = right.types(db);
        if left_types.len() != right_types.len() {
            return None;
        }

        let mut changed = false;
        let types: Box<_> = left_types
            .iter()
            .zip(right_types)
            .map(|(left, right)| {
                if left == right {
                    Some(*left)
                } else if Self::contains_dynamic_class_literal(db, *left)
                    || Self::contains_dynamic_class_literal(db, *right)
                {
                    changed = true;
                    Some(Self::merge_cycle_recovery_type(db, *left, *right))
                } else {
                    None
                }
            })
            .collect::<Option<_>>()?;
        if !changed {
            return None;
        }

        Some(Specialization::new(
            db,
            left.generic_context(db),
            types,
            left.materialization_kind(db),
            None,
        ))
    }

    fn merge_generic_aliases(
        db: &'db dyn Db,
        left: GenericAlias<'db>,
        right: GenericAlias<'db>,
    ) -> Option<GenericAlias<'db>> {
        if left.origin(db) != right.origin(db) {
            return None;
        }
        if left.origin(db).is_tuple(db) {
            return None;
        }

        Some(GenericAlias::new(
            db,
            left.origin(db),
            Self::merge_specializations(db, left.specialization(db), right.specialization(db))?,
        ))
    }

    fn merge_class_types(
        db: &'db dyn Db,
        left: ClassType<'db>,
        right: ClassType<'db>,
    ) -> Option<ClassType<'db>> {
        match (left, right) {
            (ClassType::NonGeneric(left), ClassType::NonGeneric(right)) => {
                let merged = Self::merge_same_dynamic_class_origin(
                    db,
                    Type::ClassLiteral(left),
                    Type::ClassLiteral(right),
                )?;
                match merged {
                    Type::ClassLiteral(merged) => Some(ClassType::NonGeneric(merged)),
                    _ => None,
                }
            }
            (ClassType::Generic(left), ClassType::Generic(right)) => Some(ClassType::Generic(
                Self::merge_generic_aliases(db, left, right)?,
            )),
            _ => None,
        }
    }

    fn merge_nominal_instances(
        db: &'db dyn Db,
        left: NominalInstanceType<'db>,
        right: NominalInstanceType<'db>,
    ) -> Option<Type<'db>> {
        let class = Self::merge_class_types(db, left.class(db), right.class(db))?;
        Some(Type::instance(db, class))
    }

    fn merge_same_dynamic_origin_structural(
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        match (left, right) {
            (Type::ClassLiteral(_), Type::ClassLiteral(_)) => {
                Self::merge_same_dynamic_class_origin(db, left, right)
            }
            (Type::GenericAlias(left), Type::GenericAlias(right)) => Some(Type::GenericAlias(
                Self::merge_generic_aliases(db, left, right)?,
            )),
            (Type::NominalInstance(left), Type::NominalInstance(right)) => {
                Self::merge_nominal_instances(db, left, right)
            }
            _ => None,
        }
    }

    fn push_type(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        let mut ty = ty;
        let relation_simplification = self.relation_simplification;
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
        let should_simplify_full = !matches!(ty, Type::TypeAlias(_));

        let mut ty_negated: Option<Type> = None;
        let mut to_remove = SmallVec::<[usize; 2]>::new();
        let should_try_reduce = relation_simplification.allows_try_reduce(self.db, ty);

        for (i, element) in self.elements.iter_mut().enumerate() {
            let element_type = if !should_try_reduce {
                let UnionElement::Type(element_type) = element else {
                    continue;
                };
                *element_type
            } else {
                match element.try_reduce(self.db, ty) {
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
                }
            };

            if ty == element_type {
                return;
            }

            if let Some(merged) = Self::merge_matching_cycle_marked(self.db, ty, element_type) {
                to_remove.push(i);
                ty = merged;
                continue;
            }

            if self.cycle_recovery
                && let Some(merged) =
                    Self::merge_same_dynamic_origin_structural(self.db, ty, element_type)
            {
                to_remove.push(i);
                ty = merged;
                continue;
            }

            if let Some(fused) =
                Self::cycle_fusion_candidate(self.db, self.cycle_recovery, ty, element_type)
            {
                to_remove.push(i);
                ty = fused;
                continue;
            }

            if let Some(fused) =
                Self::cycle_fusion_candidate(self.db, self.cycle_recovery, element_type, ty)
            {
                to_remove.push(i);
                ty = fused;
                continue;
            }

            if Self::cycle_marked_represents_same_type(
                self.db,
                relation_simplification,
                ty,
                element_type,
            ) {
                to_remove.push(i);
                continue;
            }

            if Self::cycle_marked_represents_same_type(
                self.db,
                relation_simplification,
                element_type,
                ty,
            ) {
                return;
            }

            // Fold `(T & ~AlwaysTruthy) | (T & ~AlwaysFalsy)` to `T`.
            if let Some(merged_type) = merge_truthiness_guarded_pair(self.db, ty, element_type) {
                to_remove.push(i);
                ty = merged_type;
                continue;
            }

            if element_type
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

            if should_simplify_full
                && !matches!(element_type, Type::TypeAlias(_))
                && relation_simplification.allows_relation(self.db, ty, element_type)
            {
                if ty.is_redundant_with(self.db, element_type) {
                    return;
                }

                if element_type.is_redundant_with(self.db, ty) {
                    to_remove.push(i);
                    continue;
                }

                // The collapse below relies on `ty | ~ty == object` (the law of excluded middle),
                // which only holds for fully-static `ty`. The recursion markers are gradual and do
                // not negate to a true set-complement: `~Divergent(id) == Divergent(id)` (a gradual
                // leaf, like `~Any == Any`), and `~Recursive(_) == Divergent(binder_id)` (see
                // `Type::negate`). Applying De Morgan would then unsoundly collapse a union to
                // `object` — e.g. `Divergent(id) | Recursive(μid.id)` (the same marker), where
                // `~Recursive(μid.id) == Divergent(id)` is trivially a subtype of the existing
                // `Divergent(id)` element.
                if !matches!(ty, Type::Divergent(_) | Type::Recursive(_)) {
                    let negated = ty_negated.get_or_insert_with(|| ty.negate(self.db));
                    if negated.is_subtype_of(self.db, element_type) {
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
        let db = self.db;
        let unpack_aliases = self.unpack_aliases;
        let cycle_recovery = self.cycle_recovery;
        let relation_simplification = self.relation_simplification;

        let type_count = self.elements.iter().map(UnionElement::type_count).sum();
        let mut types = Vec::with_capacity(type_count);
        for element in self.elements {
            match element {
                UnionElement::IntLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(LiteralValueType::new(literal, promotable))
                    }));
                }
                UnionElement::StringLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(LiteralValueType::new(literal, promotable))
                    }));
                }
                UnionElement::BytesLiterals(literals) => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(LiteralValueType::new(literal, promotable))
                    }));
                }
                UnionElement::EnumLiterals { literals, .. } => {
                    types.extend(literals.into_iter().map(|(literal, promotable)| {
                        Type::from(LiteralValueType::new(literal, promotable))
                    }));
                }
                UnionElement::Type(ty) => types.push(ty),
            }
        }

        // `μα.α` still acts as a gradual cycle marker unless the bare `α`
        // marker is also present. Only remove the redundant wrapper.
        if !cycle_recovery {
            let divergent_ids: Vec<_> = types
                .iter()
                .filter_map(|ty| match ty {
                    Type::Divergent(divergent) => Some(divergent.id(self.db)),
                    _ => None,
                })
                .collect();
            if !divergent_ids.is_empty() {
                types.retain(|ty| {
                    !matches!(
                        ty,
                        Type::Recursive(recursive)
                            if recursive.is_non_contractive(db)
                                && divergent_ids.contains(&recursive.binder_id(db))
                    )
                });
            }
        }

        if normalize_enum_complement_unions(db, &mut types) {
            let builder = UnionBuilder::new(db)
                .unpack_aliases(unpack_aliases)
                .with_relation_simplification(relation_simplification)
                .cycle_recovery(cycle_recovery);
            return types
                .into_iter()
                .fold(builder, UnionBuilder::add)
                .try_build();
        }

        match types.len() {
            0 => None,
            1 => Some(types[0]),
            _ => Some(Type::Union(UnionType::new(db, types.into_boxed_slice()))),
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
    cycle_markers: FxOrderSet<DivergentType<'db>>,
    db: &'db dyn Db,
}

impl<'db> IntersectionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            intersections: vec![InnerIntersectionBuilder::default()],
            cycle_markers: FxOrderSet::default(),
        }
    }

    fn empty(db: &'db dyn Db) -> Self {
        Self {
            db,
            intersections: vec![],
            cycle_markers: FxOrderSet::default(),
        }
    }

    pub(crate) fn add_positive(self, ty: Type<'db>) -> Self {
        self.add_positive_impl(ty, &mut FxHashSet::default())
    }

    pub(crate) fn add_positive_impl(
        mut self,
        ty: Type<'db>,
        seen_recursive_binders: &mut FxHashSet<salsa::Id>,
    ) -> Self {
        match ty {
            Type::TypeAlias(alias) => {
                let value_type = alias.value_type(self.db);
                self.add_positive_impl(value_type, seen_recursive_binders)
            }
            Type::Recursive(rec) => {
                if !seen_recursive_binders.insert(rec.binder_id(self.db)) {
                    for inner in &mut self.intersections {
                        inner.positive.insert(ty);
                    }
                    return self;
                }
                // Unfold the recursive type so that the `Type::Union` arm below can
                // distribute over the body's union elements (the body is e.g.
                // `int | tuple[Divergent, ...] | None` for `OptNestedInt`).
                // Substitute the `Divergent` α-marker back to the source type
                // so that recursive references inside the body keep their source-name
                // display and re-trigger this recursive-unfold path if visited again.
                let body = rec.body_with_origin_marker(self.db);
                self.add_positive_impl(body, seen_recursive_binders)
            }
            Type::Divergent(divergent) if let Some(marked) = divergent.as_cycle_marked(self.db) => {
                self.cycle_markers.insert(marked);
                let inner = marked.inner(self.db);
                self.add_positive_impl(inner, seen_recursive_binders)
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
                    .map(|elem| {
                        self.clone()
                            .add_positive_impl(*elem, seen_recursive_binders)
                    })
                    .fold(IntersectionBuilder::empty(self.db), |mut builder, sub| {
                        builder.intersections.extend(sub.intersections);
                        builder.cycle_markers.extend(sub.cycle_markers);
                        builder
                    })
            }
            // `(A & B & ~C) & (D & E & ~F)` -> `A & B & D & E & ~C & ~F`
            Type::Intersection(other) => {
                let db = self.db;
                for pos in other.positive(db) {
                    self = self.add_positive_impl(*pos, seen_recursive_binders);
                }
                for neg in other.negative(db) {
                    self = self.add_negative_impl(*neg, seen_recursive_binders);
                }
                self
            }
            Type::EnumComplement(complement) => {
                let db = self.db;
                self.add_positive_impl(complement.to_intersection(db), seen_recursive_binders)
            }
            _ => {
                // If we are already a union-of-intersections, distribute the new intersected element
                // across all of those intersections.
                for inner in &mut self.intersections {
                    inner.add_positive(self.db, ty);
                }
                self
            }
        }
    }

    pub(crate) fn add_negative(self, ty: Type<'db>) -> Self {
        self.add_negative_impl(ty, &mut FxHashSet::default())
    }

    pub(crate) fn add_negative_impl(
        mut self,
        ty: Type<'db>,
        seen_recursive_binders: &mut FxHashSet<salsa::Id>,
    ) -> Self {
        // See comments above in `add_positive`; this is just the negated version.
        match ty {
            Type::TypeAlias(alias) => {
                let value_type = alias.value_type(self.db);
                self.add_negative_impl(value_type, seen_recursive_binders)
            }
            Type::Recursive(rec) => {
                if !seen_recursive_binders.insert(rec.binder_id(self.db)) {
                    for inner in &mut self.intersections {
                        inner.negative.insert(ty);
                    }
                    return self;
                }
                let body = rec.body_with_origin_marker(self.db);
                self.add_negative_impl(body, seen_recursive_binders)
            }
            Type::Divergent(divergent) if let Some(marked) = divergent.as_cycle_marked(self.db) => {
                self.cycle_markers.insert(marked);
                let inner = marked.inner(self.db);
                self.add_negative_impl(inner, seen_recursive_binders)
            }
            Type::Union(union) => {
                for elem in union.elements(self.db) {
                    self = self.add_negative_impl(*elem, seen_recursive_binders);
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
                            .add_negative_impl(*elem, &mut seen_recursive_binders.clone())
                    });

                let negative_side = intersection
                    .negative(self.db)
                    .iter()
                    // all negative constraints end up becoming positive constraints
                    .map(|elem| {
                        self.clone()
                            .add_positive_impl(*elem, &mut seen_recursive_binders.clone())
                    });

                positive_side.chain(negative_side).fold(
                    IntersectionBuilder::empty(self.db),
                    |mut builder, sub| {
                        builder.intersections.extend(sub.intersections);
                        builder.cycle_markers.extend(sub.cycle_markers);
                        builder
                    },
                )
            }
            Type::EnumComplement(complement) => {
                let db = self.db;
                self.add_negative_impl(complement.to_intersection(db), seen_recursive_binders)
            }
            _ => {
                for inner in &mut self.intersections {
                    inner.add_negative(self.db, ty);
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
        let cycle_markers = self.cycle_markers;
        UnionType::from_elements(
            self.db,
            self.intersections.into_iter().map(|inner| {
                cycle_markers
                    .iter()
                    .fold(inner.build(self.db), |ty, marked| {
                        Type::cycle_marked(self.db, marked.binder_id(self.db), ty)
                    })
            }),
        )
    }
}

#[derive(Debug, Clone, Default)]
struct InnerIntersectionBuilder<'db> {
    positive: FxOrderSet<Type<'db>>,
    negative: NegativeIntersectionElements<'db>,
}

impl<'db> InnerIntersectionBuilder<'db> {
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

            let enum_class = instance.class_literal(db);
            let Some(metadata) = enum_metadata(db, enum_class) else {
                continue;
            };

            let mut excluded_names = FxHashSet::default();
            for negative in &self.negative {
                let Some(enum_literal) = negative.as_enum_literal() else {
                    continue;
                };
                if enum_literal.enum_class(db) != enum_class {
                    continue;
                }

                let name = enum_literal.name(db);
                let canonical_name = metadata.resolve_member(name).unwrap_or(name);
                excluded_names.insert(canonical_name.clone());
            }

            if excluded_names.is_empty() {
                continue;
            }

            if metadata
                .members
                .keys()
                .all(|name| excluded_names.contains(name))
            {
                return true;
            }
        }

        false
    }

    /// Adds a positive type to this intersection.
    fn add_positive(&mut self, db: &'db dyn Db, mut new_positive: Type<'db>) {
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
        if new_positive.is_divergent(db) {
            *self = Self::default();
            self.positive.insert(new_positive);
            return;
        }
        // `Divergent & T` -> `Divergent`
        if self.positive.iter().any(|ty| ty.is_divergent(db)) {
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
                self.add_negative(db, Type::string_literal(db, ""));
            }
            // `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::AlwaysFalsy if self.positive.swap_remove(&Type::literal_string()) => {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `AlwaysTruthy & LiteralString` -> `LiteralString & ~Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string()
                    && self.positive.swap_remove(&Type::AlwaysTruthy) =>
            {
                self.add_positive(db, Type::literal_string());
                self.add_negative(db, Type::string_literal(db, ""));
            }
            // `AlwaysFalsy & LiteralString` -> `Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string() && self.positive.swap_remove(&Type::AlwaysFalsy) =>
            {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string()
                    && self.negative.swap_remove(&Type::AlwaysTruthy) =>
            {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::LiteralValue(literal)
                if literal.is_literal_string() && self.negative.swap_remove(&Type::AlwaysFalsy) =>
            {
                self.add_positive(db, Type::literal_string());
                self.add_negative(db, Type::string_literal(db, ""));
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
                for (index, existing_positive) in self.positive.iter().enumerate() {
                    // S & T = S    if S <: T
                    if existing_positive.is_redundant_with(db, new_positive) {
                        return;
                    }
                    // same rule, reverse order
                    if new_positive.is_redundant_with(db, *existing_positive) {
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

                self.positive.insert(new_positive);
            }
        }
    }

    /// Adds a negative type to this intersection.
    fn add_negative(&mut self, db: &'db dyn Db, new_negative: Type<'db>) {
        // `Never & ~T` -> `Never`.
        if self.positive.contains(&Type::Never) {
            return;
        }

        // `Divergent & ~T` -> `Divergent`.
        if self.positive.iter().any(|ty| ty.is_divergent(db)) {
            debug_assert_eq!(self.positive.len(), 1, "`Divergent` should be alone");
            return;
        }

        // `T & ~Divergent` -> `Divergent` (a divergent marker is gradual, so `~Divergent` is itself).
        if new_negative.is_divergent(db) {
            *self = Self::default();
            self.positive.insert(new_negative);
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
                    self.add_negative(db, *pos);
                }
                for neg in inter.negative(db) {
                    self.add_positive(db, *neg);
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
                self.add_positive(db, ty);
            }
            // `bool & ~AlwaysTruthy` -> `bool & Literal[False]`
            Type::AlwaysTruthy if contains_bool() => {
                self.add_positive(db, Type::bool_literal(false));
            }
            // `bool & ~Literal[True]` -> `bool & Literal[False]`
            Type::LiteralValue(literal) if literal.as_bool() == Some(true) && contains_bool() => {
                self.add_positive(db, Type::bool_literal(false));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & Literal[""]`
            Type::AlwaysTruthy if self.positive.contains(&Type::literal_string()) => {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `bool & ~AlwaysFalsy` -> `bool & Literal[True]`
            Type::AlwaysFalsy if contains_bool() => {
                self.add_positive(db, Type::bool_literal(true));
            }
            // `bool & ~Literal[False]` -> `bool & Literal[True]`
            Type::LiteralValue(literal) if literal.as_bool() == Some(false) && contains_bool() => {
                self.add_positive(db, Type::bool_literal(true));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::AlwaysFalsy if self.positive.contains(&Type::literal_string()) => {
                self.add_negative(db, Type::string_literal(db, ""));
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

                    // ~S & ~T = ~T    if S <: T
                    if existing_negative.is_redundant_with(db, new_negative) {
                        to_remove.push(index);
                    }
                    // same rule, reverse order
                    if new_negative.is_subtype_of(db, *existing_negative) {
                        return;
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
            self.add_positive(db, remaining_constraint);
        }
    }

    fn build(mut self, db: &'db dyn Db) -> Type<'db> {
        if self.has_empty_enum_complement(db) {
            return Type::Never;
        }

        self.simplify_constrained_typevars(db);

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
        IntersectionBuilder, MAX_NON_RECURSIVE_UNION_LITERALS, Type, UnionBuilder, UnionType,
    };

    use crate::db::tests::{TestDb, setup_db};
    use crate::place::{global_symbol, known_module_symbol};
    use crate::types::enums::enum_member_literals;
    use crate::types::type_alias::TypeAliasType;
    use crate::types::{KnownClass, KnownInstanceType, Truthiness};

    use ruff_db::system::DbWithWritableSystem as _;
    use salsa::plumbing::Id;
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
    fn build_union_keeps_non_contractive_recursive_without_marker() {
        let db = setup_db();
        let binder_id = Id::from_bits(1);
        let non_contractive =
            Type::implicit_recursive(&db, binder_id, Type::divergent(&db, binder_id));
        let int = KnownClass::Int.to_instance(&db);

        let union = UnionType::from_elements(&db, [int, non_contractive]).expect_union();

        assert_eq!(union.elements(&db), &[int, non_contractive]);
    }

    #[test]
    fn build_union_drops_non_contractive_recursive_with_matching_marker() {
        let db = setup_db();
        let binder_id = Id::from_bits(1);
        let marker = Type::divergent(&db, binder_id);
        let non_contractive = Type::implicit_recursive(&db, binder_id, marker);

        assert_eq!(
            UnionType::from_elements(&db, [marker, non_contractive]),
            marker
        );
        assert_eq!(
            UnionType::from_elements(&db, [non_contractive, marker]),
            marker
        );
    }

    #[test]
    fn build_cycle_recovery_union_keeps_non_contractive_recursive() {
        let db = setup_db();
        let binder_id = Id::from_bits(1);
        let marker = Type::divergent(&db, binder_id);
        let non_contractive = Type::implicit_recursive(&db, binder_id, marker);

        let union = UnionBuilder::new(&db)
            .cycle_recovery(true)
            .add(marker)
            .add(non_contractive)
            .build()
            .expect_union();

        assert_eq!(union.elements(&db), &[marker, non_contractive]);
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
