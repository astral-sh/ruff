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
use crate::types::{IntersectionType, Type, UnionType};
use crate::{Db, FxOrderSet};
use smallvec::SmallVec;

use super::KnownClass;

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
                    if ty.is_subtype_of(self.db, *element) {
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
            intersections: vec![InnerIntersectionBuilder::new()],
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
            union
                .elements(self.db)
                .iter()
                .map(|elem| self.clone().add_negative(*elem))
                .fold(IntersectionBuilder::empty(self.db), |mut builder, sub| {
                    builder.intersections.extend(sub.intersections);
                    builder
                })
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
    fn new() -> Self {
        Self::default()
    }

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
            if let Type::Instance(class_type) = new_positive {
                if class_type.is_known(db, KnownClass::Bool) {
                    if let Some(&Type::BooleanLiteral(value)) = self
                        .negative
                        .iter()
                        .find(|element| element.is_boolean_literal())
                    {
                        *self = Self::new();
                        self.positive.insert(Type::BooleanLiteral(!value));
                        return;
                    }
                }
            }

            let mut to_remove = SmallVec::<[usize; 1]>::new();
            for (index, existing_positive) in self.positive.iter().enumerate() {
                // S & T = S    if S <: T
                if existing_positive.is_subtype_of(db, new_positive) {
                    return;
                }
                // same rule, reverse order
                if new_positive.is_subtype_of(db, *existing_positive) {
                    to_remove.push(index);
                }
                // A & B = Never    if A and B are disjoint
                if new_positive.is_disjoint_from(db, *existing_positive) {
                    *self = Self::new();
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
                    *self = Self::new();
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
            Type::Unbound => {}
            ty @ (Type::Any | Type::Unknown | Type::Todo) => {
                // Adding any of these types to the negative side of an intersection
                // is equivalent to adding it to the positive side. We do this to
                // simplify the representation.
                self.positive.insert(ty);
            }
            // ~Literal[True] & bool = Literal[False]
            Type::BooleanLiteral(bool)
                if self
                    .positive
                    .iter()
                    .any(|pos| *pos == KnownClass::Bool.to_instance(db)) =>
            {
                *self = Self::new();
                self.positive.insert(Type::BooleanLiteral(!bool));
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
                        *self = Self::new();
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

    fn simplify_unbound(&mut self) {
        if self.positive.contains(&Type::Unbound) {
            self.positive.retain(Type::is_unbound);
            self.negative.clear();
        }
    }

    fn build(mut self, db: &'db dyn Db) -> Type<'db> {
        self.simplify_unbound();
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
    use super::{IntersectionBuilder, IntersectionType, Type, UnionType};
    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::types::{KnownClass, StringLiteralType, UnionBuilder};
    use crate::ProgramSettings;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use test_case::test_case;

    fn setup_db() -> TestDb {
        let db = TestDb::new();

        let src_root = SystemPathBuf::from("/src");
        db.memory_file_system()
            .create_directory_all(&src_root)
            .unwrap();

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(src_root),
            },
        )
        .expect("Valid search path settings");

        db
    }

    #[test]
    fn build_union() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let t1 = Type::IntLiteral(1);
        let union = UnionType::from_elements(&db, [t0, t1]).expect_union();

        assert_eq!(union.elements(&db), &[t0, t1]);
    }

    #[test]
    fn build_union_single() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let ty = UnionType::from_elements(&db, [t0]);
        assert_eq!(ty, t0);
    }

    #[test]
    fn build_union_empty() {
        let db = setup_db();
        let ty = UnionBuilder::new(&db).build();
        assert_eq!(ty, Type::Never);
    }

    #[test]
    fn build_union_never() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let ty = UnionType::from_elements(&db, [t0, Type::Never]);
        assert_eq!(ty, t0);
    }

    #[test]
    fn build_union_bool() {
        let db = setup_db();
        let bool_instance_ty = KnownClass::Bool.to_instance(&db);

        let t0 = Type::BooleanLiteral(true);
        let t1 = Type::BooleanLiteral(true);
        let t2 = Type::BooleanLiteral(false);
        let t3 = Type::IntLiteral(17);

        let union = UnionType::from_elements(&db, [t0, t1, t3]).expect_union();
        assert_eq!(union.elements(&db), &[t0, t3]);

        let union = UnionType::from_elements(&db, [t0, t1, t2, t3]).expect_union();
        assert_eq!(union.elements(&db), &[bool_instance_ty, t3]);

        let result_ty = UnionType::from_elements(&db, [bool_instance_ty, t0]);
        assert_eq!(result_ty, bool_instance_ty);

        let result_ty = UnionType::from_elements(&db, [t0, bool_instance_ty]);
        assert_eq!(result_ty, bool_instance_ty);
    }

    #[test]
    fn build_union_flatten() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let t1 = Type::IntLiteral(1);
        let t2 = Type::IntLiteral(2);
        let u1 = UnionType::from_elements(&db, [t0, t1]);
        let union = UnionType::from_elements(&db, [u1, t2]).expect_union();

        assert_eq!(union.elements(&db), &[t0, t1, t2]);
    }

    #[test]
    fn build_union_simplify_subtype() {
        let db = setup_db();
        let t0 = KnownClass::Str.to_instance(&db);
        let t1 = Type::LiteralString;
        let u0 = UnionType::from_elements(&db, [t0, t1]);
        let u1 = UnionType::from_elements(&db, [t1, t0]);

        assert_eq!(u0, t0);
        assert_eq!(u1, t0);
    }

    #[test]
    fn build_union_no_simplify_unknown() {
        let db = setup_db();
        let t0 = KnownClass::Str.to_instance(&db);
        let t1 = Type::Unknown;
        let u0 = UnionType::from_elements(&db, [t0, t1]);
        let u1 = UnionType::from_elements(&db, [t1, t0]);

        assert_eq!(u0.expect_union().elements(&db), &[t0, t1]);
        assert_eq!(u1.expect_union().elements(&db), &[t1, t0]);
    }

    #[test]
    fn build_union_subsume_multiple() {
        let db = setup_db();
        let str_ty = KnownClass::Str.to_instance(&db);
        let int_ty = KnownClass::Int.to_instance(&db);
        let object_ty = KnownClass::Object.to_instance(&db);
        let unknown_ty = Type::Unknown;

        let u0 = UnionType::from_elements(&db, [str_ty, unknown_ty, int_ty, object_ty]);

        assert_eq!(u0.expect_union().elements(&db), &[unknown_ty, object_ty]);
    }

    impl<'db> IntersectionType<'db> {
        fn pos_vec(self, db: &'db TestDb) -> Vec<Type<'db>> {
            self.positive(db).into_iter().copied().collect()
        }

        fn neg_vec(self, db: &'db TestDb) -> Vec<Type<'db>> {
            self.negative(db).into_iter().copied().collect()
        }
    }

    #[test]
    fn build_intersection() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let ta = Type::Any;
        let intersection = IntersectionBuilder::new(&db)
            .add_positive(ta)
            .add_negative(t0)
            .build()
            .expect_intersection();

        assert_eq!(intersection.pos_vec(&db), &[ta]);
        assert_eq!(intersection.neg_vec(&db), &[t0]);
    }

    #[test]
    fn build_intersection_empty_intersection_equals_object() {
        let db = setup_db();

        let ty = IntersectionBuilder::new(&db).build();

        assert_eq!(ty, KnownClass::Object.to_instance(&db));
    }

    #[test]
    fn build_intersection_flatten_positive() {
        let db = setup_db();
        let ta = Type::Any;
        let t1 = Type::IntLiteral(1);
        let t2 = Type::IntLiteral(2);
        let i0 = IntersectionBuilder::new(&db)
            .add_positive(ta)
            .add_negative(t1)
            .build();
        let intersection = IntersectionBuilder::new(&db)
            .add_positive(t2)
            .add_positive(i0)
            .build()
            .expect_intersection();

        assert_eq!(intersection.pos_vec(&db), &[t2, ta]);
        assert_eq!(intersection.neg_vec(&db), &[]);
    }

    #[test]
    fn build_intersection_flatten_negative() {
        let db = setup_db();
        let ta = Type::Any;
        let t1 = Type::IntLiteral(1);
        let t2 = KnownClass::Int.to_instance(&db);
        let i0 = IntersectionBuilder::new(&db)
            .add_positive(ta)
            .add_negative(t1)
            .build();
        let intersection = IntersectionBuilder::new(&db)
            .add_positive(t2)
            .add_negative(i0)
            .build()
            .expect_intersection();

        assert_eq!(intersection.pos_vec(&db), &[ta, t1]);
        assert_eq!(intersection.neg_vec(&db), &[]);
    }

    #[test]
    fn intersection_distributes_over_union() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let t1 = Type::IntLiteral(1);
        let ta = Type::Any;
        let u0 = UnionType::from_elements(&db, [t0, t1]);

        let union = IntersectionBuilder::new(&db)
            .add_positive(ta)
            .add_positive(u0)
            .build()
            .expect_union();
        let [Type::Intersection(i0), Type::Intersection(i1)] = union.elements(&db)[..] else {
            panic!("expected a union of two intersections");
        };
        assert_eq!(i0.pos_vec(&db), &[ta, t0]);
        assert_eq!(i1.pos_vec(&db), &[ta, t1]);
    }

    #[test]
    fn build_intersection_self_negation() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::None)
            .add_negative(Type::None)
            .build();

        assert_eq!(ty, Type::Never);
    }

    #[test]
    fn build_intersection_simplify_negative_never() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::None)
            .add_negative(Type::Never)
            .build();

        assert_eq!(ty, Type::None);
    }

    #[test]
    fn build_intersection_simplify_positive_never() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::None)
            .add_positive(Type::Never)
            .build();

        assert_eq!(ty, Type::Never);
    }

    #[test]
    fn build_intersection_simplify_positive_unbound() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::Unbound)
            .add_positive(Type::IntLiteral(1))
            .build();

        assert_eq!(ty, Type::Unbound);
    }

    #[test]
    fn build_intersection_simplify_negative_unbound() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_negative(Type::Unbound)
            .add_positive(Type::IntLiteral(1))
            .build();

        assert_eq!(ty, Type::IntLiteral(1));
    }

    #[test]
    fn build_intersection_simplify_negative_none() {
        let db = setup_db();

        let ty = IntersectionBuilder::new(&db)
            .add_negative(Type::None)
            .add_positive(Type::IntLiteral(1))
            .build();
        assert_eq!(ty, Type::IntLiteral(1));

        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::IntLiteral(1))
            .add_negative(Type::None)
            .build();
        assert_eq!(ty, Type::IntLiteral(1));
    }

    #[test]
    fn build_intersection_simplify_positive_type_and_positive_subtype() {
        let db = setup_db();

        let t = KnownClass::Str.to_instance(&db);
        let s = Type::LiteralString;

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t)
            .add_positive(s)
            .build();
        assert_eq!(ty, s);

        let ty = IntersectionBuilder::new(&db)
            .add_positive(s)
            .add_positive(t)
            .build();
        assert_eq!(ty, s);

        let literal = Type::StringLiteral(StringLiteralType::new(&db, "a"));
        let expected = IntersectionBuilder::new(&db)
            .add_positive(s)
            .add_negative(literal)
            .build();

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t)
            .add_negative(literal)
            .add_positive(s)
            .build();
        assert_eq!(ty, expected);

        let ty = IntersectionBuilder::new(&db)
            .add_positive(s)
            .add_negative(literal)
            .add_positive(t)
            .build();
        assert_eq!(ty, expected);
    }

    #[test]
    fn build_intersection_simplify_negative_type_and_negative_subtype() {
        let db = setup_db();

        let t = KnownClass::Str.to_instance(&db);
        let s = Type::LiteralString;

        let expected = IntersectionBuilder::new(&db).add_negative(t).build();

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t)
            .add_negative(s)
            .build();
        assert_eq!(ty, expected);

        let ty = IntersectionBuilder::new(&db)
            .add_negative(s)
            .add_negative(t)
            .build();
        assert_eq!(ty, expected);

        let object = KnownClass::Object.to_instance(&db);
        let expected = IntersectionBuilder::new(&db)
            .add_negative(t)
            .add_positive(object)
            .build();

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t)
            .add_positive(object)
            .add_negative(s)
            .build();
        assert_eq!(ty, expected);
    }

    #[test]
    fn build_intersection_simplify_negative_type_and_multiple_negative_subtypes() {
        let db = setup_db();

        let s1 = Type::IntLiteral(1);
        let s2 = Type::IntLiteral(2);
        let t = KnownClass::Int.to_instance(&db);

        let expected = IntersectionBuilder::new(&db).add_negative(t).build();

        let ty = IntersectionBuilder::new(&db)
            .add_negative(s1)
            .add_negative(s2)
            .add_negative(t)
            .build();
        assert_eq!(ty, expected);
    }

    #[test]
    fn build_intersection_simplify_negative_type_and_positive_subtype() {
        let db = setup_db();

        let t = KnownClass::Str.to_instance(&db);
        let s = Type::LiteralString;

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t)
            .add_positive(s)
            .build();
        assert_eq!(ty, Type::Never);

        let ty = IntersectionBuilder::new(&db)
            .add_positive(s)
            .add_negative(t)
            .build();
        assert_eq!(ty, Type::Never);

        // This should also work in the presence of additional contributions:
        let ty = IntersectionBuilder::new(&db)
            .add_positive(KnownClass::Object.to_instance(&db))
            .add_negative(t)
            .add_positive(s)
            .build();
        assert_eq!(ty, Type::Never);

        let ty = IntersectionBuilder::new(&db)
            .add_positive(s)
            .add_negative(Type::StringLiteral(StringLiteralType::new(&db, "a")))
            .add_negative(t)
            .build();
        assert_eq!(ty, Type::Never);
    }

    #[test]
    fn build_intersection_simplify_disjoint_positive_types() {
        let db = setup_db();

        let t1 = Type::IntLiteral(1);
        let t2 = Type::None;

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t1)
            .add_positive(t2)
            .build();
        assert_eq!(ty, Type::Never);

        // If there are any negative contributions, they should
        // be removed too.
        let ty = IntersectionBuilder::new(&db)
            .add_positive(KnownClass::Str.to_instance(&db))
            .add_negative(Type::LiteralString)
            .add_positive(t2)
            .build();
        assert_eq!(ty, Type::Never);
    }

    #[test]
    fn build_intersection_simplify_disjoint_positive_and_negative_types() {
        let db = setup_db();

        let t_p = KnownClass::Int.to_instance(&db);
        let t_n = Type::StringLiteral(StringLiteralType::new(&db, "t_n"));

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_p)
            .add_negative(t_n)
            .build();
        assert_eq!(ty, t_p);

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t_n)
            .add_positive(t_p)
            .build();
        assert_eq!(ty, t_p);

        let int_literal = Type::IntLiteral(1);
        let expected = IntersectionBuilder::new(&db)
            .add_positive(t_p)
            .add_negative(int_literal)
            .build();

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_p)
            .add_negative(int_literal)
            .add_negative(t_n)
            .build();
        assert_eq!(ty, expected);

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t_n)
            .add_negative(int_literal)
            .add_positive(t_p)
            .build();
        assert_eq!(ty, expected);
    }

    #[test_case(true)]
    #[test_case(false)]
    fn build_intersection_simplify_split_bool(bool_value: bool) {
        let db = setup_db();

        let t_bool = KnownClass::Bool.to_instance(&db);
        let t_boolean_literal = Type::BooleanLiteral(bool_value);

        // We add t_object in various orders (in first or second position) in
        // the tests below to ensure that the boolean simplification eliminates
        // everything from the intersection, not just `bool`.
        let t_object = KnownClass::Object.to_instance(&db);

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_object)
            .add_positive(t_bool)
            .add_negative(t_boolean_literal)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_bool)
            .add_positive(t_object)
            .add_negative(t_boolean_literal)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));

        let ty = IntersectionBuilder::new(&db)
            .add_positive(t_object)
            .add_negative(t_boolean_literal)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));

        let ty = IntersectionBuilder::new(&db)
            .add_negative(t_boolean_literal)
            .add_positive(t_object)
            .add_positive(t_bool)
            .build();
        assert_eq!(ty, Type::BooleanLiteral(!bool_value));
    }

    #[test_case(Type::Any)]
    #[test_case(Type::Unknown)]
    #[test_case(Type::Todo)]
    fn build_intersection_t_and_negative_t_does_not_simplify(ty: Type) {
        let db = setup_db();

        let result = IntersectionBuilder::new(&db)
            .add_positive(ty)
            .add_negative(ty)
            .build();
        assert_eq!(result, ty);

        let result = IntersectionBuilder::new(&db)
            .add_negative(ty)
            .add_positive(ty)
            .build();
        assert_eq!(result, ty);
    }
}
