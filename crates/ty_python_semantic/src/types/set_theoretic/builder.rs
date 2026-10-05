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
//! cycles, an intersection can retain redundant or contradictory elements instead.
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

use std::convert::Infallible;
use std::hint::cold_path;
use std::ops::ControlFlow;

use super::RecursivelyDefined;
use crate::types::enums::EnumComplement;
use crate::types::{
    BytesLiteralType, ClassLiteral, EnumLiteralType, IntersectionType, KnownClass,
    LiteralValueType, LiteralValueTypeKind, NegativeIntersectionElements, StringLiteralType, Type,
    TypePair, UnionType,
};
use crate::{Db, FxOrderMap, FxOrderSet, ProgramEnvironment};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

pub(in crate::types) mod controlled_union;
pub(in crate::types) mod intersection_assembly;
pub(in crate::types) mod intersection_distribution;
pub(in crate::types) mod intersection_distribution_storage;
pub(in crate::types) mod intersection_expansion;
pub(in crate::types) mod intersection_finalization;
pub(in crate::types) mod intersection_insertion;
pub(in crate::types) mod intersection_simplification;
pub(in crate::types) mod intersection_speculation;
pub(in crate::types) mod intersection_storage;

fn merge_truthiness_guarded_cores<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    (left_core, left_guard): (Type<'db>, Type<'db>),
    (right_core, right_guard): (Type<'db>, Type<'db>),
) -> Option<Type<'db>> {
    if left_core.is_equivalent_to(db, env, right_core) {
        return Some(left_core);
    }

    let candidate = UnionType::from_elements(db, env, [left_core, right_core]);
    let left_reconstructed = IntersectionType::from_two_elements(db, env, candidate, left_guard);
    let right_reconstructed = IntersectionType::from_two_elements(db, env, candidate, right_guard);
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
fn normalize_enum_complement_union<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    types: &mut Vec<Type<'db>>,
    complement_index: usize,
    complement: EnumComplement<'db>,
) -> bool {
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
        let mut builder = IntersectionBuilder::new(db, env)
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

        remove_indices.sort_unstable();
        for index in remove_indices.into_iter().rev() {
            types.swap_remove(index);
        }
        return true;
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
pub(in crate::types) enum UnionElement<'db> {
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
        env: &ProgramEnvironment<'db>,
        other_type: Type<'db>,
        cycle_recovery: bool,
    ) -> ReduceResult<'db> {
        if let UnionElement::Type(existing) = self {
            return ReduceResult::Type(*existing);
        }

        if cycle_recovery {
            cold_path();

            // A widened literal group must absorb matching literals from later iterations for
            // recovery to converge. Preserve that exact fallback reduction without relation queries.
            return match self {
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
                        .is_none_or(|instance| instance.class_literal(db, env) != *enum_class),
                ),
                UnionElement::Type(_) => unreachable!("ordinary types are handled before recovery"),
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
            if ignore || other_type.is_redundant_with(db, env, ty) {
                ignore = true;
                return true;
            }
            if collapse
                || other_type.negation_is_subtype_of_cached(
                    db,
                    env,
                    ty,
                    &mut other_type_negated_cache,
                )
            {
                collapse = true;
                return true;
            }
            !ty.is_redundant_with(db, env, other_type)
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
                        .is_redundant_with(db, env, other_type)
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
                        .is_redundant_with(db, env, other_type)
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
                        .is_redundant_with(db, env, other_type)
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
                        .is_redundant_with(db, env, other_type)
                }
            }
            UnionElement::Type(_) => unreachable!("ordinary types are handled before reduction"),
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

pub(in crate::types) enum ReduceResult<'db> {
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
pub(super) const MAX_NON_RECURSIVE_UNION_LITERALS: usize = 8192;
pub(crate) struct UnionBuilder<'db> {
    elements: Vec<UnionElement<'db>>,
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    unpack_aliases: bool,
    /// This is enabled when joining types in a `cycle_recovery` function. Because recovery cannot
    /// introduce a new cycle, relation-based union simplifications are skipped in this mode.
    cycle_recovery: bool,
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
        }
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
        self.merge_recursively_defined(val);
        self
    }

    pub(in crate::types) fn merge_recursively_defined(&mut self, val: RecursivelyDefined) {
        self.recursively_defined = self.recursively_defined.or(val);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub(in crate::types) fn environment(&self) -> &ProgramEnvironment<'db> {
        &self.env
    }

    pub(in crate::types) fn element(&self, index: usize) -> Option<&UnionElement<'db>> {
        self.elements.get(index)
    }

    pub(in crate::types) fn recursion_state(&self) -> RecursivelyDefined {
        self.recursively_defined
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn elements_storage(&self) -> (usize, usize) {
        (self.elements.len(), self.elements.capacity())
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn reserve_elements(&mut self, additional: usize) {
        self.elements.reserve_exact(additional);
    }

    pub(in crate::types) fn next_element(&self, cursor: &mut usize) -> Option<usize> {
        if *cursor < self.elements.len() {
            let index = *cursor;
            *cursor += 1;
            Some(index)
        } else {
            None
        }
    }

    pub(in crate::types) fn next_type_count(&self, cursor: &mut usize) -> Option<usize> {
        self.next_element(cursor)
            .map(|index| self.elements[index].type_count())
    }

    pub(in crate::types) fn next_literal_count(&self, cursor: &mut usize) -> Option<usize> {
        self.next_element(cursor)
            .map(|index| match &self.elements[index] {
                UnionElement::IntLiterals(literals) => literals.len(),
                UnionElement::StringLiterals(literals) => literals.len(),
                UnionElement::BytesLiterals(literals) => literals.len(),
                UnionElement::EnumLiterals { literals, .. } => literals.len(),
                UnionElement::Type(_) => 0,
            })
    }

    pub(in crate::types) fn take_elements(&mut self) -> controlled_union::UnionElements<'db> {
        controlled_union::UnionElements::new(std::mem::take(&mut self.elements))
    }

    pub(in crate::types) fn replace_type(&mut self, index: usize, ty: Type<'db>) {
        self.elements[index] = UnionElement::Type(ty);
    }

    pub(in crate::types) fn remove_type(&mut self, index: usize) {
        self.elements.swap_remove(index);
    }

    pub(in crate::types) fn append_type(&mut self, ty: Type<'db>) {
        self.elements.push(UnionElement::Type(ty));
    }

    /// Collapse the union to a single type: `object`.
    pub(in crate::types) fn collapse_to_object(&mut self) {
        self.elements.clear();
        self.elements.push(UnionElement::Type(Type::object()));
    }

    fn widen_literal_types(&mut self, seen_aliases: &mut Vec<Type<'db>>) {
        let db = self.db;
        let mut replace_with = vec![];
        for elem in &self.elements {
            match elem {
                UnionElement::IntLiterals(_) => {
                    replace_with.push(KnownClass::Int.to_instance(db, &self.env));
                }
                UnionElement::StringLiterals(_) => {
                    replace_with.push(KnownClass::Str.to_instance(db, &self.env));
                }
                UnionElement::BytesLiterals(_) => {
                    replace_with.push(KnownClass::Bytes.to_instance(db, &self.env));
                }
                UnionElement::EnumLiterals { literals, .. } => {
                    let (enum_literal, _) = literals.first().unwrap();
                    replace_with.push(enum_literal.enum_class_instance(db, &self.env));
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
        match controlled_union::add_in_place_sync(
            self,
            ty,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn add_in_place_impl(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        match controlled_union::add_in_place_impl_sync(
            self,
            ty,
            seen_aliases,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn add_union(&mut self, union: UnionType<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        match controlled_union::add_union_sync(
            self,
            union,
            seen_aliases,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn add_alias(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        let db = self.db;
        if seen_aliases.contains(&ty) {
            // Union contains itself recursively via a type alias. This is an error, just
            // leave out the recursive alias. TODO surface this error.
        } else {
            seen_aliases.push(ty);
            self.add_in_place_impl(ty.resolve_type_alias(db), seen_aliases);
        }
    }

    fn add_literal(&mut self, literal: LiteralValueType<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        match controlled_union::add_literal_sync(
            self,
            literal,
            seen_aliases,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    pub(in crate::types) fn merge_literal_recursion(&mut self, literal: LiteralValueType<'db>) {
        self.recursively_defined = self.recursively_defined.or(literal.recursively_defined());
    }

    fn add_grouped_literal(
        &mut self,
        literal: LiteralValueType<'db>,
        group: controlled_union::GroupedLiteral<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) {
        let db = self.db;
        let ty = Type::LiteralValue(literal);
        let cycle_recovery = self.cycle_recovery;
        let should_widen = |literals, recursively_defined: RecursivelyDefined| {
            if recursively_defined.is_yes() && cycle_recovery {
                literals >= MAX_RECURSIVE_UNION_LITERALS
            } else {
                literals >= MAX_NON_RECURSIVE_UNION_LITERALS
            }
        };

        let mut ty_negated_cache = None;
        let mut ty_negated = || *ty_negated_cache.get_or_insert_with(|| ty.negate(db, &self.env));
        match group {
            // If adding a string literal, look for an existing `UnionElement::StringLiterals` to
            // add it to, or an existing element that is a super-type of string literals, which
            // means we shouldn't add it. Otherwise, add a new `UnionElement::StringLiterals`
            // containing it.
            controlled_union::GroupedLiteral::String(string_literal) => {
                let mut found = None;
                let mut to_remove = None;
                for (index, element) in self.elements.iter_mut().enumerate() {
                    match element {
                        UnionElement::StringLiterals(literals) => {
                            if should_widen(literals.len(), self.recursively_defined) {
                                let replace_with = KnownClass::Str.to_instance(db, &self.env);
                                self.add_in_place_impl(replace_with, seen_aliases);
                                return;
                            }
                            found = Some(literals);
                            continue;
                        }
                        UnionElement::Type(existing)
                            if cycle_recovery
                                && literal.fallback_instance(db, &self.env) == *existing =>
                        {
                            return;
                        }
                        UnionElement::Type(existing) if !cycle_recovery => {
                            // e.g. `existing` could be `Literal[""] & Any`,
                            // and `ty` could be `Literal[""]`
                            if ty.is_redundant_with(db, &self.env, *existing) {
                                return;
                            }
                            if existing.is_redundant_with(db, &self.env, ty) {
                                to_remove = Some(index);
                                continue;
                            }
                            if ty_negated().is_subtype_of(db, &self.env, *existing) {
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
                    self.elements
                        .push(UnionElement::StringLiterals(FxOrderMap::from_iter([(
                            string_literal,
                            literal.is_promotable(),
                        )])));
                }
                if let Some(index) = to_remove {
                    self.elements.swap_remove(index);
                }
            }
            // Same for bytes literals as for string literals, above.
            controlled_union::GroupedLiteral::Bytes(bytes_literal) => {
                let mut found = None;
                let mut to_remove = None;
                for (index, element) in self.elements.iter_mut().enumerate() {
                    match element {
                        UnionElement::BytesLiterals(literals) => {
                            if should_widen(literals.len(), self.recursively_defined) {
                                let replace_with = KnownClass::Bytes.to_instance(db, &self.env);
                                self.add_in_place_impl(replace_with, seen_aliases);
                                return;
                            }
                            found = Some(literals);
                            continue;
                        }
                        UnionElement::Type(existing)
                            if cycle_recovery
                                && literal.fallback_instance(db, &self.env) == *existing =>
                        {
                            return;
                        }
                        UnionElement::Type(existing) if !cycle_recovery => {
                            if ty.is_redundant_with(db, &self.env, *existing) {
                                return;
                            }
                            // e.g. `existing` could be `Literal[b""] & Any`,
                            // and `ty` could be `Literal[b""]`
                            if existing.is_redundant_with(db, &self.env, ty) {
                                to_remove = Some(index);
                                continue;
                            }
                            if ty_negated().is_subtype_of(db, &self.env, *existing) {
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
            controlled_union::GroupedLiteral::Int(int_literal) => {
                let mut found = None;
                let mut to_remove = None;
                for (index, element) in self.elements.iter_mut().enumerate() {
                    match element {
                        UnionElement::IntLiterals(literals) => {
                            if should_widen(literals.len(), self.recursively_defined) {
                                let replace_with = KnownClass::Int.to_instance(db, &self.env);
                                self.add_in_place_impl(replace_with, seen_aliases);
                                return;
                            }
                            found = Some(literals);
                            continue;
                        }
                        UnionElement::Type(existing)
                            if cycle_recovery
                                && literal.fallback_instance(db, &self.env) == *existing =>
                        {
                            return;
                        }
                        UnionElement::Type(existing) if !cycle_recovery => {
                            if ty.is_redundant_with(db, &self.env, *existing) {
                                return;
                            }
                            // e.g. `existing` could be `Literal[1] & Any`,
                            // and `ty` could be `Literal[1]`
                            if existing.is_redundant_with(db, &self.env, ty) {
                                to_remove = Some(index);
                                continue;
                            }
                            if ty_negated().is_subtype_of(db, &self.env, *existing) {
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
                    *found.entry(int_literal.as_i64()).or_insert(is_promotable) &= is_promotable;
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
            controlled_union::GroupedLiteral::Enum(enum_member_to_add) => {
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
                                let replace_with = literal.enum_class_instance(db, &self.env);
                                self.add_in_place_impl(replace_with, seen_aliases);
                                return;
                            }
                            found = Some(literals);
                            continue;
                        }
                        UnionElement::Type(existing)
                            if cycle_recovery
                                && literal.fallback_instance(db, &self.env) == *existing =>
                        {
                            return;
                        }
                        UnionElement::Type(existing) if !cycle_recovery => {
                            if ty.is_redundant_with(db, &self.env, *existing) {
                                return;
                            }
                            // e.g. `existing` could be `Literal[Foo.X] & Any`,
                            // and `ty` could be `Literal[Foo.X]`
                            if existing.is_redundant_with(db, &self.env, ty) {
                                to_remove = Some(index);
                                continue;
                            }
                            if ty_negated().is_subtype_of(db, &self.env, *existing) {
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
        }
    }

    fn push_type(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        match controlled_union::push_type_sync(
            self,
            ty,
            seen_aliases,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn reduce_type_element(
        &mut self,
        i: usize,
        insertion: &mut controlled_union::UnionTypeInsertion<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> bool {
        match controlled_union::reduce_type_element_sync(
            self,
            i,
            insertion,
            seen_aliases,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(keep_going) => keep_going,
            Err(never) => match never {},
        }
    }

    pub(crate) fn build(self) -> Type<'db> {
        self.try_build().unwrap_or(Type::Never)
    }

    pub(crate) fn try_build(self) -> Option<Type<'db>> {
        match controlled_union::try_build_sync(
            self,
            controlled_union::UnionFacts,
            &controlled_union::OrdinaryUnionEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    fn convert_element(&self, element: UnionElement<'db>, types: &mut Vec<Type<'db>>) {
        let recursively_defined = self.recursively_defined;
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
}

impl<'db> IntersectionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env: env.clone(),
            intersections: vec![InnerIntersectionBuilder::default()],
            has_disjunction: false,
        }
    }

    pub(super) fn bounded_from_elements<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
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

        // Before distributing multiple disjunctions, apply narrowing factors regardless of their
        // input order. With at most one disjunction, retain the original intersection element order.
        // Classification follows aliases and negations without expanding into DNF; the builder
        // performs that expansion under its budget and recursion guard.
        let is_disjunctive = |ty: &Type<'db>| Self::is_disjunctive(db, env, *ty);
        let multiple_disjunctions = elements.clone().filter(is_disjunctive).nth(1).is_some();
        let mut builder = Self::new(db, env);
        for element in elements
            .clone()
            .filter(|ty| !multiple_disjunctions || !is_disjunctive(ty))
            .chain(elements.filter(|ty| multiple_disjunctions && is_disjunctive(ty)))
        {
            builder
                .add_positive_impl::<BoundedIntersection>(element, &mut vec![])
                .continue_value()?;
        }
        Some(builder.build())
    }

    /// Whether expanding a factor can introduce alternatives, including through De Morgan's law.
    fn is_disjunctive(db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> bool {
        let mut pending = SmallVec::<[_; 4]>::from_slice(&[(ty, false)]);
        let mut seen_aliases = FxHashSet::default();
        while let Some((ty, negated)) = pending.pop() {
            match ty {
                Type::TypeAlias(_) | Type::Recursive(_) => {
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
            self.add_positive_impl::<UnboundedIntersection>(ty, &mut vec![]);
    }

    fn add_positive_impl<L: IntersectionLimits>(
        &mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> ControlFlow<L::Break> {
        match intersection_expansion::add_sync(
            self,
            ty,
            intersection_expansion::Sign::Positive,
            seen_aliases,
            intersection_expansion::ExpansionFacts,
            &intersection_expansion::OrdinaryExpansionEffects::<L>::new(),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    pub(crate) fn add_negative(mut self, ty: Type<'db>) -> Self {
        self.add_negative_in_place(ty);
        self
    }

    pub(crate) fn add_negative_in_place(&mut self, ty: Type<'db>) {
        let ControlFlow::Continue(()) =
            self.add_negative_impl::<UnboundedIntersection>(ty, &mut vec![]);
    }

    fn add_negative_impl<L: IntersectionLimits>(
        &mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> ControlFlow<L::Break> {
        match intersection_expansion::add_sync(
            self,
            ty,
            intersection_expansion::Sign::Negative,
            seen_aliases,
            intersection_expansion::ExpansionFacts,
            &intersection_expansion::OrdinaryExpansionEffects::<L>::new(),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
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

    pub(crate) fn build(mut self) -> Type<'db> {
        intersection_assembly::build(&mut self)
    }
}

/// The signs of a pair of intersection elements. For `Mixed`, the first is positive.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub(in crate::types) enum IntersectionPolarity {
    Positive,
    Negative,
    Mixed,
}

// Hashing and equality inspect only the enum discriminant.
impl salsa::plumbing::function::FixedQueryFields for IntersectionPolarity {}

/// Describes the signed intersection elements, so `Disjoint` also covers `S & ~T` when `S <: T`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::SalsaValue, get_size2::GetSize)]
pub(in crate::types) enum IntersectionSimplification {
    Unchanged,
    FirstRedundant,
    SecondRedundant,
    Disjoint,
}

pub(in crate::types) fn simplify_intersection_pair<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    first: Type<'db>,
    second: Type<'db>,
    polarity: IntersectionPolarity,
) -> IntersectionSimplification {
    intersection_simplification::simplify_sync(
        first,
        second,
        polarity,
        &intersection_simplification::OrdinarySimplificationComparison { db, env },
    )
    .unwrap_or_else(|never| match never {})
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
#[salsa::tracked(configuration = (pub(in crate::types) SimplifyIntersectionPairImplConfiguration),
    attempt = ReturnOnly,
    returns(copy),
    cycle_result=|_, _, _, _| IntersectionSimplification::Unchanged,
    heap_size=ruff_memory_usage::heap_size,
)]
fn simplify_intersection_pair_impl<'db>(
    db: &'db dyn Db,
    types: TypePair<'db>,
    polarity: IntersectionPolarity,
) -> IntersectionSimplification {
    intersection_simplification::produce_sync(
        types,
        polarity,
        &intersection_simplification::OrdinaryIntersectionSimplification { db },
    )
    .unwrap_or_else(|never| match never {})
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn intersection_simplification_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<SimplifyIntersectionPairImplConfiguration> {
    simplify_intersection_pair_impl::fn_ingredient_(db, db.zalsa())
}

#[derive(Debug, Default)]
pub(in crate::types) struct InnerIntersectionBuilder<'db> {
    positive: FxOrderSet<Type<'db>>,
    negative: NegativeIntersectionElements<'db>,
    #[cfg(any(test, feature = "experimental-analysis"))]
    storage: intersection_storage::RetainedStorage,
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
    fn has_empty_enum_complement(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        crate::types::enums::intersection::has_empty_enum_complement(
            db,
            env,
            &self.positive,
            &self.negative,
        )
    }

    /// Adds a positive type to this intersection.
    fn add_positive(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        new_positive: Type<'db>,
    ) {
        match intersection_insertion::add_sync(
            self,
            new_positive,
            intersection_insertion::Sign::Positive,
            intersection_insertion::InsertionFacts,
            &intersection_insertion::OrdinaryInsertionEffects::new(db, env),
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    /// Adds a negative type to this intersection.
    fn add_negative(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        new_negative: Type<'db>,
    ) {
        match intersection_insertion::add_sync(
            self,
            new_negative,
            intersection_insertion::Sign::Negative,
            intersection_insertion::InsertionFacts,
            &intersection_insertion::OrdinaryInsertionEffects::new(db, env),
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn build(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        intersection_finalization::build(db, env, self)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IntersectionBuilder, IntersectionPolarity, MAX_NON_RECURSIVE_UNION_LITERALS,
        MAX_RECURSIVE_UNION_LITERALS, RecursivelyDefined, Type, UnionBuilder, UnionType,
        simplify_intersection_pair, simplify_intersection_pair_impl,
    };

    use crate::db::tests::{TestDb, setup_db};
    use crate::place::{global_symbol, known_module_symbol};
    use crate::types::enums::enum_member_literals;
    use crate::types::tuple::TupleType;
    use crate::types::type_alias::TypeAliasType;
    use crate::types::{
        ApplyTypeMappingVisitor, BytesLiteralType, KnownClass, KnownInstanceType, LiteralValueType,
        LiteralValueTypeKind, Signature, StringLiteralType, Truthiness, TypeContext, TypeMapping,
        TypePair,
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
    fn cycle_recovery_widens_after_union_recursion_is_merged() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let literal_limit =
            i64::try_from(MAX_RECURSIVE_UNION_LITERALS).expect("literal limit fits in i64");

        for (count, recursive, widens) in [
            (literal_limit - 1, RecursivelyDefined::Yes, false),
            (literal_limit, RecursivelyDefined::No, false),
            (literal_limit, RecursivelyDefined::Yes, true),
        ] {
            let elements: Box<[_]> = (0..count).map(Type::int_literal).collect();
            let element_count = elements.len();
            let union = UnionType::new(db, elements, recursive);
            let result = UnionBuilder::new(db, &env)
                .cycle_recovery(true)
                .add(Type::Union(union))
                .build();

            if widens {
                assert_eq!(result, KnownClass::Int.to_instance(db, &env));
            } else {
                let result = result.expect_union();
                assert_eq!(result.elements(db).len(), element_count);
                assert_eq!(result.recursively_defined(db), recursive);
            }
        }
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
        assert_eq!(
            union.apply_type_mapping_impl(
                &db,
                &TypeMapping::ReplaceParameterDefaults,
                TypeContext::default(),
                &ApplyTypeMappingVisitor::new(&env),
            ),
            union_ty
        );
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
