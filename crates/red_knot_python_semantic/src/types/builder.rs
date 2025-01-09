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

use crate::types::{InstanceType, IntersectionType, KnownClass, Type, UnionType};
use crate::{Db, FxOrderSet};
use smallvec::SmallVec;

use super::Truthiness;

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
            Type::Never => {}
            _ => {
                let bool_pair = if let Type::BooleanLiteral(b) = ty {
                    Some(Type::BooleanLiteral(!b))
                } else {
                    None
                };

                let mut to_add = ty;
                let mut to_remove = SmallVec::<[usize; 2]>::new();
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

                    if ty.is_same_gradual_form(*element) || ty.is_subtype_of(self.db, *element) {
                        return self;
                    } else if element.is_subtype_of(self.db, ty) {
                        to_remove.push(index);
                    }
                }
                match to_remove[..] {
                    [] => self.elements.push(to_add),
                    [index] => self.elements[index] = to_add,
                    _ => {
                        let mut current_index = 0;
                        let mut to_remove = to_remove.into_iter();
                        let mut next_to_remove_index = to_remove.next();
                        self.elements.retain(|_| {
                            let retain = if Some(current_index) == next_to_remove_index {
                                next_to_remove_index = to_remove.next();
                                false
                            } else {
                                true
                            };
                            current_index += 1;
                            retain
                        });
                        self.elements.push(to_add);
                    }
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
    fn add_positive(&mut self, db: &'db dyn Db, new_positive: Type<'db>) {
        if let Type::Intersection(other) = new_positive {
            for pos in other.positive(db) {
                self.add_positive(db, *pos);
            }
            for neg in other.negative(db) {
                self.add_negative(db, *neg);
            }
        } else {
            // ~Literal[True] & bool = Literal[False]
            // ~AlwaysTruthy & bool = Literal[False]
            if let Type::Instance(InstanceType { class }) = new_positive {
                if class.is_known(db, KnownClass::Bool) {
                    if let Some(new_type) = self
                        .negative
                        .iter()
                        .find(|element| {
                            element.is_boolean_literal()
                                | matches!(element, Type::AlwaysFalsy | Type::AlwaysTruthy)
                        })
                        .map(|element| {
                            Type::BooleanLiteral(element.bool(db) != Truthiness::AlwaysTrue)
                        })
                    {
                        *self = Self::default();
                        self.positive.insert(new_type);
                        return;
                    }
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
            for index in to_remove.iter().rev() {
                self.positive.swap_remove_index(*index);
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
            for index in to_remove.iter().rev() {
                self.negative.swap_remove_index(*index);
            }

            self.positive.insert(new_positive);
        }
    }

    /// Adds a negative type to this intersection.
    fn add_negative(&mut self, db: &'db dyn Db, new_negative: Type<'db>) {
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
            Type::Instance(instance) if instance.class.is_known(db, KnownClass::Object) => {
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
            // bool & ~Literal[True] = Literal[False]
            // bool & ~AlwaysTruthy = Literal[False]
            Type::BooleanLiteral(_) | Type::AlwaysFalsy | Type::AlwaysTruthy
                if self.positive.contains(&KnownClass::Bool.to_instance(db)) =>
            {
                *self = Self::default();
                self.positive.insert(Type::BooleanLiteral(
                    new_negative.bool(db) != Truthiness::AlwaysTrue,
                ));
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
                for index in to_remove.iter().rev() {
                    self.negative.swap_remove_index(*index);
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

    fn build(mut self, db: &'db dyn Db) -> Type<'db> {
        match (self.positive.len(), self.negative.len()) {
            (0, 0) => KnownClass::Object.to_instance(db),
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
        assert_eq!(intersection, KnownClass::Object.to_instance(&db));
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
        let t_object = KnownClass::Object.to_instance(&db);
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
