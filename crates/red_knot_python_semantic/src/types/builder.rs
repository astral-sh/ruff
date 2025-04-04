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
//!
//! The implication of these invariants is that a [`UnionBuilder`] does not necessarily build a
//! [`Type::Union`]. For example, if only one type is added to the [`UnionBuilder`], `build()` will
//! just return that type directly. The same is true for [`IntersectionBuilder`]; for example, if a
//! union type is added to the intersection, it will distribute and [`IntersectionBuilder::build`]
//! may end up returning a [`Type::Union`] of intersections.
//!
//! In the future we should have these additional invariants, but they aren't implemented yet:
//!   * No type in a union can be a subtype of any other type in the union (just eliminate the
//!     subtype from the union).
//!   * No type in an intersection can be a supertype of any other type in the intersection (just
//!     eliminate the supertype from the intersection).
//!   * An intersection containing two non-overlapping types should simplify to [`Type::Never`].

use crate::types::{IntersectionType, KnownClass, Type, TypeVarBoundOrConstraints, UnionType};
use crate::{Db, FxOrderSet};
use smallvec::SmallVec;

pub(crate) struct UnionBuilder<'db> {
    elements: Vec<Type<'db>>,
    db: &'db dyn Db,
}

impl<'db> UnionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            elements: vec![],
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Collapse the union to a single type: `object`.
    fn collapse_to_object(mut self) -> Self {
        self.elements.clear();
        self.elements.push(Type::object(self.db));
        self
    }

    /// Adds a type to this union.
    pub(crate) fn add(mut self, ty: Type<'db>) -> Self {
        match ty {
            Type::Union(union) => {
                let new_elements = union.elements(self.db);
                self.elements.reserve(new_elements.len());
                for element in new_elements {
                    self = self.add(*element);
                }
            }
            // Adding `Never` to a union is a no-op.
            Type::Never => {}
            // Adding `object` to a union results in `object`.
            ty if ty.is_object(self.db) => {
                return self.collapse_to_object();
            }
            _ => {
                let bool_pair = if let Type::BooleanLiteral(b) = ty {
                    Some(Type::BooleanLiteral(!b))
                } else {
                    None
                };

                let mut to_add = ty;
                let mut to_remove = SmallVec::<[usize; 2]>::new();
                let ty_negated = ty.negate(self.db);

                for (index, element) in self.elements.iter().enumerate() {
                    if Some(*element) == bool_pair {
                        to_add = KnownClass::Bool.to_instance(self.db);
                        to_remove.push(index);
                        // The type we are adding is a BooleanLiteral, which doesn't have any
                        // subtypes. And we just found that the union already contained our
                        // mirror-image BooleanLiteral, so it can't also contain bool or any
                        // supertype of bool. Therefore, we are done.
                        break;
                    }

                    if ty.is_same_gradual_form(*element)
                        || ty.is_subtype_of(self.db, *element)
                        || element.is_object(self.db)
                    {
                        return self;
                    } else if element.is_subtype_of(self.db, ty) {
                        to_remove.push(index);
                    } else if ty_negated.is_subtype_of(self.db, *element) {
                        // We add `ty` to the union. We just checked that `~ty` is a subtype of an existing `element`.
                        // This also means that `~ty | ty` is a subtype of `element | ty`, because both elements in the
                        // first union are subtypes of the corresponding elements in the second union. But `~ty | ty` is
                        // just `object`. Since `object` is a subtype of `element | ty`, we can only conclude that
                        // `element | ty` must be `object` (object has no other supertypes). This means we can simplify
                        // the whole union to just `object`, since all other potential elements would also be subtypes of
                        // `object`.
                        return self.collapse_to_object();
                    }
                }
                if let Some((&first, rest)) = to_remove.split_first() {
                    self.elements[first] = to_add;
                    // We iterate in descending order to keep remaining indices valid after `swap_remove`.
                    for &index in rest.iter().rev() {
                        self.elements.swap_remove(index);
                    }
                } else {
                    self.elements.push(to_add);
                }
            }
        }
        self
    }

    pub(crate) fn build(self) -> Type<'db> {
        match self.elements.len() {
            0 => Type::Never,
            1 => self.elements[0],
            _ => Type::Union(UnionType::new(self.db, self.elements.into_boxed_slice())),
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
}

impl<'db> IntersectionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            intersections: vec![InnerIntersectionBuilder::default()],
        }
    }

    fn empty(db: &'db dyn Db) -> Self {
        Self {
            db,
            intersections: vec![],
        }
    }

    pub(crate) fn add_positive(mut self, ty: Type<'db>) -> Self {
        if let Type::Union(union) = ty {
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
                .map(|elem| self.clone().add_positive(*elem))
                .fold(IntersectionBuilder::empty(self.db), |mut builder, sub| {
                    builder.intersections.extend(sub.intersections);
                    builder
                })
        } else {
            // If we are already a union-of-intersections, distribute the new intersected element
            // across all of those intersections.
            for inner in &mut self.intersections {
                inner.add_positive(self.db, ty);
            }
            self
        }
    }

    pub(crate) fn add_negative(mut self, ty: Type<'db>) -> Self {
        // See comments above in `add_positive`; this is just the negated version.
        if let Type::Union(union) = ty {
            for elem in union.elements(self.db) {
                self = self.add_negative(*elem);
            }
            self
        } else if let Type::Intersection(intersection) = ty {
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
                .map(|elem| self.clone().add_negative(*elem));

            let negative_side = intersection
                .negative(self.db)
                .iter()
                // all negative constraints end up becoming positive constraints
                .map(|elem| self.clone().add_positive(*elem));

            positive_side.chain(negative_side).fold(
                IntersectionBuilder::empty(self.db),
                |mut builder, sub| {
                    builder.intersections.extend(sub.intersections);
                    builder
                },
            )
        } else {
            for inner in &mut self.intersections {
                inner.add_negative(self.db, ty);
            }
            self
        }
    }

    pub(crate) fn build(mut self) -> Type<'db> {
        // Avoid allocating the UnionBuilder unnecessarily if we have just one intersection:
        if self.intersections.len() == 1 {
            self.intersections.pop().unwrap().build(self.db)
        } else {
            UnionType::from_elements(
                self.db,
                self.intersections
                    .into_iter()
                    .map(|inner| inner.build(self.db)),
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
            // `(A & B & ~C) & (D & E & ~F)` -> `A & B & D & E & ~C & ~F`
            Type::Intersection(other) => {
                for pos in other.positive(db) {
                    self.add_positive(db, *pos);
                }
                for neg in other.negative(db) {
                    self.add_negative(db, *neg);
                }
            }
            _ => {
                let known_instance = new_positive
                    .into_instance()
                    .and_then(|instance| instance.class().known(db));

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
                        Type::Instance(instance)
                            if instance.class().is_known(db, KnownClass::Bool) =>
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
                    if existing_positive.is_subtype_of(db, new_positive)
                        || existing_positive.is_same_gradual_form(new_positive)
                    {
                        return;
                    }
                    // same rule, reverse order
                    if new_positive.is_subtype_of(db, *existing_positive) {
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
                .filter_map(|ty| ty.into_instance())
                .filter_map(|instance| instance.class().known(db))
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
            Type::Instance(instance) if instance.class().is_object(db) => {
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
                    if existing_negative.is_subtype_of(db, new_negative) {
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
            let Type::TypeVar(typevar) = ty else {
                continue;
            };
            let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
                typevar.bound_or_constraints(db)
            else {
                continue;
            };

            // Determine which constraints appear as positive entries in the intersection. Note
            // that we shouldn't have duplicate entries in the positive or negative lists, so we
            // don't need to worry about finding any particular constraint more than once.
            let constraints = constraints.elements(db);
            let mut positive_constraint_count = 0;
            for positive in &self.positive {
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

    fn build(mut self, db: &'db dyn Db) -> Type<'db> {
        self.simplify_constrained_typevars(db);
        match (self.positive.len(), self.negative.len()) {
            (0, 0) => Type::object(db),
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
    use super::{IntersectionBuilder, Type, UnionBuilder, UnionType};

    use crate::db::tests::setup_db;
    use crate::types::{KnownClass, Truthiness};

    use test_case::test_case;

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
        assert_eq!(intersection, Type::object(&db));
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
        let t_object = Type::object(&db);
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
}
