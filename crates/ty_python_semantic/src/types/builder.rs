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
//! large enums (not yet implemented) or from string/integer/bytes literals, which can grow due to
//! literal arithmetic or operations on literal strings/bytes. For normal unions, it's most
//! efficient to just store the member types in a vector, and do O(n^2) `is_subtype_of` checks to
//! maintain the union in simplified form. But literal unions can grow to a size where this becomes
//! a performance problem. For this reason, we group literal types in `UnionBuilder`. Since every
//! different string literal type shares exactly the same possible super-types, and none of them
//! are subtypes of each other (unless exactly the same literal type), we can avoid many
//! unnecessary `is_subtype_of` checks.

use crate::types::enums::{enum_member_literals, enum_metadata};
use crate::types::type_ordering::union_or_intersection_elements_ordering;
use crate::types::{
    BytesLiteralType, IntersectionType, KnownClass, StringLiteralType, Type,
    TypeVarBoundOrConstraints, UnionType,
};
use crate::{Db, FxOrderSet};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralKind {
    Int,
    String,
    Bytes,
}

impl<'db> Type<'db> {
    /// Return `true` if this type can be a supertype of some literals of `kind` and not others.
    fn splits_literals(self, db: &'db dyn Db, kind: LiteralKind) -> bool {
        match (self, kind) {
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => true,
            (Type::StringLiteral(_), LiteralKind::String) => true,
            (Type::BytesLiteral(_), LiteralKind::Bytes) => true,
            (Type::IntLiteral(_), LiteralKind::Int) => true,
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
            _ => false,
        }
    }
}

#[derive(Debug)]
enum UnionElement<'db> {
    IntLiterals(FxOrderSet<i64>),
    StringLiterals(FxOrderSet<StringLiteralType<'db>>),
    BytesLiterals(FxOrderSet<BytesLiteralType<'db>>),
    Type(Type<'db>),
}

impl<'db> UnionElement<'db> {
    const fn to_type_element(&self) -> Option<Type<'db>> {
        match self {
            UnionElement::Type(ty) => Some(*ty),
            _ => None,
        }
    }

    /// Try reducing this `UnionElement` given the presence in the same union of `other_type`.
    fn try_reduce(&mut self, db: &'db dyn Db, other_type: Type<'db>) -> ReduceResult<'db> {
        match self {
            UnionElement::IntLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::Int) {
                    let mut collapse = false;
                    let mut ignore = false;
                    let negated = other_type.negate(db);
                    literals.retain(|literal| {
                        let ty = Type::IntLiteral(*literal);
                        if negated.is_subtype_of(db, ty) {
                            collapse = true;
                        }
                        if other_type.is_subtype_of(db, ty) {
                            ignore = true;
                        }
                        !ty.is_subtype_of(db, other_type)
                    });
                    if ignore {
                        ReduceResult::Ignore
                    } else if collapse {
                        ReduceResult::CollapseToObject
                    } else {
                        ReduceResult::KeepIf(!literals.is_empty())
                    }
                } else {
                    ReduceResult::KeepIf(
                        !Type::IntLiteral(literals[0]).is_subtype_of(db, other_type),
                    )
                }
            }
            UnionElement::StringLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::String) {
                    let mut collapse = false;
                    let mut ignore = false;
                    let negated = other_type.negate(db);
                    literals.retain(|literal| {
                        let ty = Type::StringLiteral(*literal);
                        if negated.is_subtype_of(db, ty) {
                            collapse = true;
                        }
                        if other_type.is_subtype_of(db, ty) {
                            ignore = true;
                        }
                        !ty.is_subtype_of(db, other_type)
                    });
                    if ignore {
                        ReduceResult::Ignore
                    } else if collapse {
                        ReduceResult::CollapseToObject
                    } else {
                        ReduceResult::KeepIf(!literals.is_empty())
                    }
                } else {
                    ReduceResult::KeepIf(
                        !Type::StringLiteral(literals[0]).is_subtype_of(db, other_type),
                    )
                }
            }
            UnionElement::BytesLiterals(literals) => {
                if other_type.splits_literals(db, LiteralKind::Bytes) {
                    let mut collapse = false;
                    let mut ignore = false;
                    let negated = other_type.negate(db);
                    literals.retain(|literal| {
                        let ty = Type::BytesLiteral(*literal);
                        if negated.is_subtype_of(db, ty) {
                            collapse = true;
                        }
                        if other_type.is_subtype_of(db, ty) {
                            ignore = true;
                        }
                        !ty.is_subtype_of(db, other_type)
                    });
                    if ignore {
                        ReduceResult::Ignore
                    } else if collapse {
                        ReduceResult::CollapseToObject
                    } else {
                        ReduceResult::KeepIf(!literals.is_empty())
                    }
                } else {
                    ReduceResult::KeepIf(
                        !Type::BytesLiteral(literals[0]).is_subtype_of(db, other_type),
                    )
                }
            }
            UnionElement::Type(existing) => ReduceResult::Type(*existing),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum RecursivelyDefined {
    Yes,
    No,
}

impl RecursivelyDefined {
    const fn is_yes(self) -> bool {
        matches!(self, RecursivelyDefined::Yes)
    }

    const fn or(self, other: RecursivelyDefined) -> RecursivelyDefined {
        match (self, other) {
            (RecursivelyDefined::Yes, _) | (_, RecursivelyDefined::Yes) => RecursivelyDefined::Yes,
            _ => RecursivelyDefined::No,
        }
    }
}

/// If the value ​​is defined recursively, widening is performed from fewer literal elements, resulting in faster convergence of the fixed-point iteration.
const MAX_RECURSIVE_UNION_LITERALS: usize = 10;
/// If the value ​​is defined non-recursively, the fixed-point iteration will converge in one go,
/// so in principle we can have as many literal elements as we want, but to avoid unintended huge computational loads, we limit it to 256.
const MAX_NON_RECURSIVE_UNION_LITERALS: usize = 256;

pub(crate) struct UnionBuilder<'db> {
    elements: Vec<UnionElement<'db>>,
    db: &'db dyn Db,
    unpack_aliases: bool,
    order_elements: bool,
    // This is enabled when joining types in a `cycle_recovery` function.
    // Since a cycle cannot be created within a `cycle_recovery` function, execution of `is_redundant_with` is skipped.
    cycle_recovery: bool,
    recursively_defined: RecursivelyDefined,
}

impl<'db> UnionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            elements: vec![],
            unpack_aliases: true,
            order_elements: false,
            cycle_recovery: false,
            recursively_defined: RecursivelyDefined::No,
        }
    }

    pub(crate) fn unpack_aliases(mut self, val: bool) -> Self {
        self.unpack_aliases = val;
        self
    }

    pub(crate) fn order_elements(mut self, val: bool) -> Self {
        self.order_elements = val;
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
                    self.add_in_place_impl(*element, seen_aliases);
                }
                self.recursively_defined = self
                    .recursively_defined
                    .or(union.recursively_defined(self.db));
                if self.cycle_recovery && self.recursively_defined.is_yes() {
                    let literals = self.elements.iter().fold(0, |acc, elem| match elem {
                        UnionElement::IntLiterals(literals) => acc + literals.len(),
                        UnionElement::StringLiterals(literals) => acc + literals.len(),
                        UnionElement::BytesLiterals(literals) => acc + literals.len(),
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
                if seen_aliases.contains(&ty) {
                    // Union contains itself recursively via a type alias. This is an error, just
                    // leave out the recursive alias. TODO surface this error.
                } else {
                    seen_aliases.push(ty);
                    self.add_in_place_impl(alias.value_type(self.db), seen_aliases);
                }
            }
            // If adding a string literal, look for an existing `UnionElement::StringLiterals` to
            // add it to, or an existing element that is a super-type of string literals, which
            // means we shouldn't add it. Otherwise, add a new `UnionElement::StringLiterals`
            // containing it.
            Type::StringLiteral(literal) => {
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
                        UnionElement::Type(existing) => {
                            // e.g. `existing` could be `Literal[""] & Any`,
                            // and `ty` could be `Literal[""]`
                            if ty.is_subtype_of(self.db, *existing) {
                                return;
                            }
                            if existing.is_subtype_of(self.db, ty) {
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
                    found.insert(literal);
                } else {
                    self.elements
                        .push(UnionElement::StringLiterals(FxOrderSet::from_iter([
                            literal,
                        ])));
                }
                if let Some(index) = to_remove {
                    self.elements.swap_remove(index);
                }
            }
            // Same for bytes literals as for string literals, above.
            Type::BytesLiteral(literal) => {
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
                        UnionElement::Type(existing) => {
                            if ty.is_subtype_of(self.db, *existing) {
                                return;
                            }
                            // e.g. `existing` could be `Literal[b""] & Any`,
                            // and `ty` could be `Literal[b""]`
                            if existing.is_subtype_of(self.db, ty) {
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
                    found.insert(literal);
                } else {
                    self.elements
                        .push(UnionElement::BytesLiterals(FxOrderSet::from_iter([
                            literal,
                        ])));
                }
                if let Some(index) = to_remove {
                    self.elements.swap_remove(index);
                }
            }
            // And same for int literals as well.
            Type::IntLiteral(literal) => {
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
                        UnionElement::Type(existing) => {
                            if ty.is_subtype_of(self.db, *existing) {
                                return;
                            }
                            // e.g. `existing` could be `Literal[1] & Any`,
                            // and `ty` could be `Literal[1]`
                            if existing.is_subtype_of(self.db, ty) {
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
                    found.insert(literal);
                } else {
                    self.elements
                        .push(UnionElement::IntLiterals(FxOrderSet::from_iter([literal])));
                }
                if let Some(index) = to_remove {
                    self.elements.swap_remove(index);
                }
            }
            Type::EnumLiteral(enum_member_to_add) => {
                let enum_class = enum_member_to_add.enum_class(self.db);
                let metadata =
                    enum_metadata(self.db, enum_class).expect("Class of enum literal is an enum");

                let enum_members_in_union = self
                    .elements
                    .iter()
                    .filter_map(UnionElement::to_type_element)
                    .filter_map(Type::as_enum_literal)
                    .map(|literal| literal.name(self.db))
                    .chain(std::iter::once(enum_member_to_add.name(self.db)))
                    .collect::<FxHashSet<_>>();

                let all_members_are_in_union = metadata
                    .members
                    .keys()
                    .all(|name| enum_members_in_union.contains(name));

                if all_members_are_in_union {
                    self.add_in_place_impl(
                        enum_member_to_add.enum_class_instance(self.db),
                        seen_aliases,
                    );
                } else if !self
                    .elements
                    .iter()
                    .filter_map(UnionElement::to_type_element)
                    .any(|ty| Type::EnumLiteral(enum_member_to_add).is_subtype_of(self.db, ty))
                {
                    self.push_type(Type::EnumLiteral(enum_member_to_add), seen_aliases);
                }
            }
            // Adding `object` to a union results in `object`.
            ty if ty.is_object() => {
                self.collapse_to_object();
            }
            _ => {
                self.push_type(ty, seen_aliases);
            }
        }
    }

    fn push_type(&mut self, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) {
        let bool_pair = if let Type::BooleanLiteral(b) = ty {
            Some(Type::BooleanLiteral(!b))
        } else {
            None
        };

        // If an alias gets here, it means we aren't unpacking aliases, and we also
        // shouldn't try to simplify aliases out of the union, because that will require
        // unpacking them.
        let should_simplify_full = !matches!(ty, Type::TypeAlias(_)) && !self.cycle_recovery;

        let mut ty_negated: Option<Type> = None;
        let mut to_remove = SmallVec::<[usize; 2]>::new();

        for (i, element) in self.elements.iter_mut().enumerate() {
            let element_type = match element.try_reduce(self.db, ty) {
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

            if Some(element_type) == bool_pair {
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
        let mut types = vec![];
        for element in self.elements {
            match element {
                UnionElement::IntLiterals(literals) => {
                    types.extend(literals.into_iter().map(Type::IntLiteral));
                }
                UnionElement::StringLiterals(literals) => {
                    types.extend(literals.into_iter().map(Type::StringLiteral));
                }
                UnionElement::BytesLiterals(literals) => {
                    types.extend(literals.into_iter().map(Type::BytesLiteral));
                }
                UnionElement::Type(ty) => types.push(ty),
            }
        }
        if self.order_elements {
            types.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(self.db, l, r));
        }
        match types.len() {
            0 => None,
            1 => Some(types[0]),
            _ => Some(Type::Union(UnionType::new(
                self.db,
                types.into_boxed_slice(),
                self.recursively_defined,
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
    order_elements: bool,
    db: &'db dyn Db,
}

impl<'db> IntersectionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            order_elements: false,
            intersections: vec![InnerIntersectionBuilder::default()],
        }
    }

    fn empty(db: &'db dyn Db) -> Self {
        Self {
            db,
            order_elements: false,
            intersections: vec![],
        }
    }

    pub(crate) fn add_positive(self, ty: Type<'db>) -> Self {
        self.add_positive_impl(ty, &mut vec![])
    }

    pub(crate) fn add_positive_impl(
        mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Self {
        match ty {
            Type::TypeAlias(alias) => {
                if seen_aliases.contains(&ty) {
                    // Recursive alias, add it without expanding to avoid infinite recursion.
                    for inner in &mut self.intersections {
                        inner.positive.insert(ty);
                    }
                    return self;
                }
                seen_aliases.push(ty);
                let value_type = alias.value_type(self.db);
                self.add_positive_impl(value_type, seen_aliases)
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
                    .fold(IntersectionBuilder::empty(self.db), |mut builder, sub| {
                        builder.intersections.extend(sub.intersections);
                        builder
                    })
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
            Type::NominalInstance(instance)
                if enum_metadata(self.db, instance.class_literal(self.db)).is_some() =>
            {
                let mut contains_enum_literal_as_negative_element = false;
                for intersection in &self.intersections {
                    if intersection.negative.iter().any(|negative| {
                        negative
                            .as_enum_literal()
                            .is_some_and(|lit| lit.enum_class_instance(self.db) == ty)
                    }) {
                        contains_enum_literal_as_negative_element = true;
                        break;
                    }
                }

                if contains_enum_literal_as_negative_element {
                    // If we have an enum literal of this enum already in the negative side of
                    // the intersection, expand the instance into the union of enum members, and
                    // add that union to the intersection.
                    // Note: we manually construct a `UnionType` here instead of going through
                    // `UnionBuilder` because we would simplify the union to just the enum instance
                    // and end up in this branch again.
                    let db = self.db;
                    self.add_positive_impl(
                        Type::Union(UnionType::new(
                            db,
                            enum_member_literals(db, instance.class_literal(db), None)
                                .expect("Calling `enum_member_literals` on an enum class")
                                .collect::<Box<[_]>>(),
                            RecursivelyDefined::No,
                        )),
                        seen_aliases,
                    )
                } else {
                    for inner in &mut self.intersections {
                        inner.add_positive(self.db, ty);
                    }
                    self
                }
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
        self.add_negative_impl(ty, &mut vec![])
    }

    pub(crate) fn add_negative_impl(
        mut self,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Self {
        let contains_enum = |enum_instance| {
            self.intersections
                .iter()
                .flat_map(|intersection| &intersection.positive)
                .any(|ty| *ty == enum_instance)
        };

        // See comments above in `add_positive`; this is just the negated version.
        match ty {
            Type::TypeAlias(alias) => {
                if seen_aliases.contains(&ty) {
                    // Recursive alias, add it without expanding to avoid infinite recursion.
                    for inner in &mut self.intersections {
                        inner.negative.insert(ty);
                    }
                    return self;
                }
                seen_aliases.push(ty);
                let value_type = alias.value_type(self.db);
                self.add_negative_impl(value_type, seen_aliases)
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
                    IntersectionBuilder::empty(self.db),
                    |mut builder, sub| {
                        builder.intersections.extend(sub.intersections);
                        builder
                    },
                )
            }
            Type::EnumLiteral(enum_literal)
                if contains_enum(enum_literal.enum_class_instance(self.db)) =>
            {
                let db = self.db;
                self.add_positive_impl(
                    UnionType::from_elements(
                        db,
                        enum_member_literals(
                            db,
                            enum_literal.enum_class(db),
                            Some(enum_literal.name(db)),
                        )
                        .expect("Calling `enum_member_literals` on an enum class"),
                    ),
                    seen_aliases,
                )
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

    pub(crate) fn build(mut self) -> Type<'db> {
        // Avoid allocating the UnionBuilder unnecessarily if we have just one intersection:
        if self.intersections.len() == 1 {
            self.intersections
                .pop()
                .unwrap()
                .build(self.db, self.order_elements)
        } else {
            UnionType::from_elements(
                self.db,
                self.intersections
                    .into_iter()
                    .map(|inner| inner.build(self.db, self.order_elements)),
            )
        }
    }
}

#[derive(Debug, Clone, Default)]
struct InnerIntersectionBuilder<'db> {
    positive: FxOrderSet<Type<'db>>,
    negative: FxOrderSet<Type<'db>>,
}

impl<'db> InnerIntersectionBuilder<'db> {
    /// Adds a positive type to this intersection.
    fn add_positive(&mut self, db: &'db dyn Db, mut new_positive: Type<'db>) {
        match new_positive {
            // `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
            Type::AlwaysTruthy if self.positive.contains(&Type::LiteralString) => {
                self.add_negative(db, Type::string_literal(db, ""));
            }
            // `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::AlwaysFalsy if self.positive.swap_remove(&Type::LiteralString) => {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `AlwaysTruthy & LiteralString` -> `LiteralString & ~Literal[""]`
            Type::LiteralString if self.positive.swap_remove(&Type::AlwaysTruthy) => {
                self.add_positive(db, Type::LiteralString);
                self.add_negative(db, Type::string_literal(db, ""));
            }
            // `AlwaysFalsy & LiteralString` -> `Literal[""]`
            Type::LiteralString if self.positive.swap_remove(&Type::AlwaysFalsy) => {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & AlwaysFalsy` -> `Literal[""]`
            Type::LiteralString if self.negative.swap_remove(&Type::AlwaysTruthy) => {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::LiteralString if self.negative.swap_remove(&Type::AlwaysFalsy) => {
                self.add_positive(db, Type::LiteralString);
                self.add_negative(db, Type::string_literal(db, ""));
            }

            _ => {
                let known_instance = new_positive
                    .as_nominal_instance()
                    .and_then(|instance| instance.known_class(db));

                if known_instance == Some(KnownClass::Object) {
                    // `object & T` -> `T`; it is always redundant to add `object` to an intersection
                    return;
                }

                let addition_is_bool_instance = known_instance == Some(KnownClass::Bool);

                for (index, existing_positive) in self.positive.iter().enumerate() {
                    match existing_positive {
                        // `AlwaysTruthy & bool` -> `Literal[True]`
                        Type::AlwaysTruthy if addition_is_bool_instance => {
                            new_positive = Type::BooleanLiteral(true);
                        }
                        // `AlwaysFalsy & bool` -> `Literal[False]`
                        Type::AlwaysFalsy if addition_is_bool_instance => {
                            new_positive = Type::BooleanLiteral(false);
                        }
                        Type::NominalInstance(instance)
                            if instance.has_known_class(db, KnownClass::Bool) =>
                        {
                            match new_positive {
                                // `bool & AlwaysTruthy` -> `Literal[True]`
                                Type::AlwaysTruthy => {
                                    new_positive = Type::BooleanLiteral(true);
                                }
                                // `bool & AlwaysFalsy` -> `Literal[False]`
                                Type::AlwaysFalsy => {
                                    new_positive = Type::BooleanLiteral(false);
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
                            Type::BooleanLiteral(bool_value) => {
                                new_positive = Type::BooleanLiteral(!bool_value);
                            }
                            // `bool & ~AlwaysTruthy` -> `Literal[False]`
                            Type::AlwaysTruthy => {
                                new_positive = Type::BooleanLiteral(false);
                            }
                            // `bool & ~AlwaysFalsy` -> `Literal[True]`
                            Type::AlwaysFalsy => {
                                new_positive = Type::BooleanLiteral(true);
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
            // `bool & ~Literal[True]` -> `bool & Literal[False]`
            Type::AlwaysTruthy | Type::BooleanLiteral(true) if contains_bool() => {
                self.add_positive(db, Type::BooleanLiteral(false));
            }
            // `LiteralString & ~AlwaysTruthy` -> `LiteralString & Literal[""]`
            Type::AlwaysTruthy if self.positive.contains(&Type::LiteralString) => {
                self.add_positive(db, Type::string_literal(db, ""));
            }
            // `bool & ~AlwaysFalsy` -> `bool & Literal[True]`
            // `bool & ~Literal[False]` -> `bool & Literal[True]`
            Type::AlwaysFalsy | Type::BooleanLiteral(false) if contains_bool() => {
                self.add_positive(db, Type::BooleanLiteral(true));
            }
            // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
            Type::AlwaysFalsy if self.positive.contains(&Type::LiteralString) => {
                self.add_negative(db, Type::string_literal(db, ""));
            }
            _ => {
                let mut to_remove = SmallVec::<[usize; 1]>::new();
                for (index, existing_negative) in self.negative.iter().enumerate() {
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

    /// Tries to simplify any constrained typevars in the intersection:
    ///
    /// - If the intersection contains a positive entry for exactly one of the constraints, we can
    ///   remove the typevar (effectively replacing it with that one positive constraint).
    ///
    /// - If the intersection contains negative entries for all but one of the constraints, we can
    ///   remove the negative constraints and replace the typevar with the remaining positive
    ///   constraint.
    ///
    /// - If the intersection contains negative entries for all of the constraints, the overall
    ///   intersection is `Never`.
    fn simplify_constrained_typevars(&mut self, db: &'db dyn Db) {
        let mut to_add = SmallVec::<[Type<'db>; 1]>::new();
        let mut positive_to_remove = SmallVec::<[usize; 1]>::new();

        for (typevar_index, ty) in self.positive.iter().enumerate() {
            let Type::TypeVar(bound_typevar) = ty else {
                continue;
            };
            let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
                bound_typevar.typevar(db).bound_or_constraints(db)
            else {
                continue;
            };

            // Determine which constraints appear as positive entries in the intersection. Note
            // that we shouldn't have duplicate entries in the positive or negative lists, so we
            // don't need to worry about finding any particular constraint more than once.
            let constraints = constraints.elements(db);
            let mut positive_constraint_count = 0;
            for (i, positive) in self.positive.iter().enumerate() {
                if i == typevar_index {
                    continue;
                }

                // This linear search should be fine as long as we don't encounter typevars with
                // thousands of constraints.
                positive_constraint_count += constraints
                    .iter()
                    .filter(|c| c.is_subtype_of(db, *positive))
                    .count();
            }

            // If precisely one constraint appears as a positive element, we can replace the
            // typevar with that positive constraint.
            if positive_constraint_count == 1 {
                positive_to_remove.push(typevar_index);
                continue;
            }

            // Determine which constraints appear as negative entries in the intersection.
            let mut to_remove = Vec::with_capacity(constraints.len());
            let mut remaining_constraints: Vec<_> = constraints.iter().copied().map(Some).collect();
            for (negative_index, negative) in self.negative.iter().enumerate() {
                // This linear search should be fine as long as we don't encounter typevars with
                // thousands of constraints.
                let matching_constraints = constraints
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.is_subtype_of(db, *negative));
                for (constraint_index, _) in matching_constraints {
                    to_remove.push(negative_index);
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

            // Only one typevar constraint remains. Remove all of the negative constraints, and
            // replace the typevar itself with the remaining positive constraint.
            to_add.push(remaining_constraint);
            positive_to_remove.push(typevar_index);
        }

        // We don't need to sort the positive list, since we only append to it in increasing order.
        for index in positive_to_remove.into_iter().rev() {
            self.positive.swap_remove_index(index);
        }

        for remaining_constraint in to_add {
            self.add_positive(db, remaining_constraint);
        }
    }

    fn build(mut self, db: &'db dyn Db, order_elements: bool) -> Type<'db> {
        self.simplify_constrained_typevars(db);

        // If any typevars are in `self.positive`, speculatively solve all bounded type variables
        // to their upper bound and all constrained type variables to the union of their constraints.
        // If that speculative intersection simplifies to `Never`, this intersection must also simplify
        // to `Never`.
        if self.positive.iter().any(|ty| ty.is_type_var()) {
            let mut speculative = IntersectionBuilder::new(db);
            for pos in &self.positive {
                match pos {
                    Type::TypeVar(type_var) => {
                        match type_var.typevar(db).bound_or_constraints(db) {
                            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                                speculative = speculative.add_positive(bound);
                            }
                            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                                speculative = speculative.add_positive(constraints.as_type(db));
                            }
                            // TypeVars without a bound or constraint implicitly have `object` as their
                            // upper bound, and it is always a no-op to add `object` to an intersection.
                            None => {}
                        }
                    }
                    _ => speculative = speculative.add_positive(*pos),
                }
            }
            for neg in &self.negative {
                speculative = speculative.add_negative(*neg);
            }
            if speculative.build().is_never() {
                return Type::Never;
            }
        }

        match (self.positive.len(), self.negative.len()) {
            (0, 0) => Type::object(),
            (1, 0) => self.positive[0],
            _ => {
                self.positive.shrink_to_fit();
                self.negative.shrink_to_fit();
                if order_elements {
                    self.positive
                        .sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
                    self.negative
                        .sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
                }
                Type::Intersection(IntersectionType::new(db, self.positive, self.negative))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IntersectionBuilder, Type, UnionBuilder, UnionType};

    use crate::db::tests::setup_db;
    use crate::place::known_module_symbol;
    use crate::types::enums::enum_member_literals;
    use crate::types::{KnownClass, Truthiness};

    use test_case::test_case;
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

        let t0 = Type::IntLiteral(0);
        let union = UnionType::from_elements(&db, [t0]);
        assert_eq!(union, t0);
    }

    #[test]
    fn build_union_two_elements() {
        let db = setup_db();

        let t0 = Type::IntLiteral(0);
        let t1 = Type::IntLiteral(1);
        let union = UnionType::from_elements(&db, [t0, t1]).expect_union();

        assert_eq!(union.elements(&db), &[t0, t1]);
    }

    #[test]
    fn build_intersection_empty_intersection_equals_object() {
        let db = setup_db();

        let intersection = IntersectionBuilder::new(&db).build();
        assert_eq!(intersection, Type::object());
    }

    #[test_case(Type::BooleanLiteral(true))]
    #[test_case(Type::BooleanLiteral(false))]
    #[test_case(Type::AlwaysTruthy)]
    #[test_case(Type::AlwaysFalsy)]
    fn build_intersection_simplify_split_bool(t_splitter: Type) {
        let db = setup_db();
        let bool_value = t_splitter.bool(&db) == Truthiness::AlwaysTrue;

        // We add t_object in various orders (in first or second position) in
        // the tests below to ensure that the boolean simplification eliminates
        // everything from the intersection, not just `bool`.
        let t_object = Type::object();
        let t_bool = KnownClass::Bool.to_instance(&db);

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_object)
            .add_positive(t_bool)
            .add_negative(t_splitter)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_bool)
            .add_positive(t_object)
            .add_negative(t_splitter)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_object)
            .add_negative(t_splitter)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t_splitter)
            .add_positive(t_object)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));
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
