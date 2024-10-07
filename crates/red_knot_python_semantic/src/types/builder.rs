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

use super::{KnownClass, Truthiness};

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
                    // Handle `True | False` -> `bool`
                    if Some(*element) == bool_pair {
                        to_add = KnownClass::Bool.to_instance(self.db);
                        to_remove.push(index);
                        // The type we are adding is a BooleanLiteral, which doesn't have any
                        // subtypes. And we just found that the union already contained our
                        // mirror-image BooleanLiteral, so it can't also contain bool or any
                        // supertype of bool. Therefore, we are done.
                        break;
                    }

                    match (ty, *element) {
                        // Handle `Truthy | Falsy` -> `Never`
                        (Type::Truthy, Type::Falsy) | (Type::Falsy, Type::Truthy) => {
                            to_add = Type::Never;
                            to_remove.push(index);
                            break;
                        }
                        // Handle `X & Truthy | X & Falsy` -> `X`
                        (Type::Intersection(present), Type::Intersection(inserted)) => {
                            // Detect `X & Truthy | Y & Falsy`
                            if let (Some(present_ty), Some(inserted_ty)) =
                                (present.truthy_of(self.db), inserted.falsy_of(self.db))
                            {
                                // If `X` = `Y`, we can simplify `X & Truthy | X & Falsy` to `X`
                                if present_ty == inserted_ty {
                                    to_add = present_ty;
                                    to_remove.push(index);
                                    break;
                                }
                            }

                            // Detect `X & Falsy | Y & Truthy`
                            if let (Some(present_ty), Some(inserted_ty)) =
                                (present.falsy_of(self.db), inserted.truthy_of(self.db))
                            {
                                // If `X` = `Y`, we can simplify `X & Falsy | X & Truthy` to `X`
                                if present_ty == inserted_ty {
                                    to_add = present_ty;
                                    to_remove.push(index);
                                    break;
                                }
                            }
                        }

                        // Corner-case of the previous `X & Truthy | X & Falsy` -> `X`
                        // Some `X & Truthy` or `X & Falsy` types have been simplified to a
                        // specific subset of instances of the type.
                        (Type::Intersection(inter), ty) | (ty, Type::Intersection(inter)) => {
                            if let Some(inter_ty) = inter.truthy_of(self.db) {
                                // 'X & Truthy | y' -> test if `y` = `X & Falsy`
                                if let Some(falsy_set) = inter_ty.falsy_set(self.db) {
                                    if falsy_set == ty {
                                        to_add = inter_ty;
                                        to_remove.push(index);
                                        break;
                                    }
                                }
                            }

                            if let Some(inter_ty) = inter.falsy_of(self.db) {
                                // 'X & Falsy | y' -> test if `y` = `X & Truthy`
                                if let Some(truthy_set) = inter_ty.truthy_set(self.db) {
                                    if truthy_set == ty {
                                        to_add = inter_ty;
                                        to_remove.push(index);
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {}
                    };

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
            _ => Type::Union(UnionType::new(self.db, self.elements.into())),
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

    /// Creates an intersection builder with the given type & `Truthy` and returns the built
    /// intersection type.
    pub(crate) fn build_truthy(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Self::new(db)
            .add_positive(ty)
            .add_positive(Type::Truthy)
            .build()
    }

    /// Creates an intersection builder with the given type & `Falsy` and returns the built
    /// intersection type.
    pub(crate) fn build_falsy(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Self::new(db)
            .add_positive(ty)
            .add_positive(Type::Falsy)
            .build()
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
    fn add_positive(&mut self, db: &'db dyn Db, ty: Type<'db>) {
        // TODO `Any`/`Unknown`/`Todo` actually should not self-cancel
        match ty {
            Type::Intersection(inter) => {
                let pos = inter.positive(db);
                let neg = inter.negative(db);
                self.positive.extend(pos.difference(&self.negative));
                self.negative.extend(neg.difference(&self.positive));
                self.positive.retain(|elem| !neg.contains(elem));
                self.negative.retain(|elem| !pos.contains(elem));
            }
            _ => {
                if !self.negative.remove(&ty) {
                    self.positive.insert(ty);
                };
            }
        }
    }

    /// Adds a negative type to this intersection.
    fn add_negative(&mut self, db: &'db dyn Db, ty: Type<'db>) {
        // TODO `Any`/`Unknown`/`Todo` actually should not self-cancel

        // `[Type::Truthy]` and `[Type::Falsy]` should never be in the negative set, so we add
        // their opposite to the positive set.
        match ty {
            Type::Truthy => {
                self.positive.insert(Type::Falsy);
            }
            Type::Falsy => {
                self.positive.insert(Type::Truthy);
            }
            Type::Intersection(intersection) => {
                let pos = intersection.negative(db);
                let neg = intersection.positive(db);
                self.positive.extend(pos.difference(&self.negative));
                self.negative.extend(neg.difference(&self.positive));
                self.positive.retain(|elem| !neg.contains(elem));
                self.negative.retain(|elem| !pos.contains(elem));
            }
            Type::Never => {}
            Type::Unbound => {}
            _ => {
                if !self.positive.remove(&ty) {
                    self.negative.insert(ty);
                };
            }
        }
    }

    fn simplify(&mut self, db: &'db dyn Db) {
        // TODO this should be generalized based on subtyping, for now we just handle a few cases

        // Never is a subtype of all types
        if self.positive.contains(&Type::Never) {
            self.positive.retain(Type::is_never);
            self.negative.clear();
        }

        if self.positive.contains(&Type::Unbound) {
            self.positive.retain(Type::is_unbound);
            self.negative.clear();
        }

        // If we have `Truthy` and all elements are always true, we can remove it
        if self.positive.contains(&Type::Truthy)
            && self
                .positive
                .iter()
                .all(|ty| ty.bool(db) == Truthiness::AlwaysTrue)
        {
            self.positive.remove(&Type::Truthy);
        }

        // If we have `Falsy` and all elements are always false, we can remove it
        if self.positive.contains(&Type::Falsy)
            && self
                .positive
                .iter()
                .all(|ty| ty.bool(db) == Truthiness::AlwaysFalse)
        {
            self.positive.remove(&Type::Falsy);
        }

        // If we have both `AlwaysTrue` and `AlwaysFalse`, this intersection should be empty.
        if self
            .positive
            .iter()
            .any(|ty| ty.bool(db) == Truthiness::AlwaysFalse)
            && self
                .positive
                .iter()
                .any(|ty| ty.bool(db) == Truthiness::AlwaysTrue)
        {
            self.positive.clear();
            self.negative.clear();
        }

        // None intersects only with object
        for pos in &self.positive {
            if let Type::Instance(_) = pos {
                // could be `object` type
            } else {
                self.negative.remove(&Type::None);
                break;
            }
        }

        // If an intersection is `X & Falsy`, try to replace it by the falsy set of `X`
        // TODO: this doesn't handle the case `X & Y & Falsy` where `(X & Y)` would have a known
        // falsy set (this doesn't happen yet, can it happen?)
        if self.positive.len() == 2 && self.positive.contains(&Type::Falsy) {
            self.positive.remove(&Type::Falsy);
            let ty = self.positive.iter().next().unwrap();
            if let Some(falsy) = ty.falsy_set(db) {
                self.positive.clear();
                self.positive.insert(falsy);
            } else {
                self.positive.insert(Type::Falsy);
            }
        }

        // If an intersection is `X & Truthy`, try to replace it by the truthy set of `X`
        // TODO: this doesn't handle the case `X & Y & Truthy` where `(X & Y)` would have a known
        // truthy set (this doesn't happen yet, can it happen?)
        if self.positive.len() == 2 && self.positive.contains(&Type::Truthy) {
            self.positive.remove(&Type::Truthy);
            let ty = self.positive.iter().next().unwrap();
            if let Some(truthy) = ty.truthy_set(db) {
                self.positive.clear();
                self.positive.insert(truthy);
            } else {
                self.positive.insert(Type::Truthy);
            }
        }

        // If an intersection is `X`, check for `y` in negatives where `y` is the truthy/falsy set
        // of `X`
        // TODO: same as above, does not handle a case like `X & Y & ~z`.
        // TODO: we don't handle the case where the truthy/falsy set of `X` is multiple elements.
        if self.positive.len() == 1 {
            // Because our case is so narrow (len == 1), there's no need to simplify again
            let ty = self.positive.iter().next().unwrap();
            let truthy_set = ty.truthy_set(db).unwrap_or(Type::Never);
            if self.negative.iter().any(|n| *n == truthy_set) {
                self.positive.insert(Type::Falsy);
                self.negative.retain(|n| n != &truthy_set);
            }

            // Query `ty` again to avoid borrowing multiple times as mutable & immutable
            let ty = self.positive.iter().next().unwrap();
            let falsy_set = ty.falsy_set(db).unwrap_or(Type::Never);
            if self.negative.iter().any(|n| *n == falsy_set) {
                self.positive.insert(Type::Truthy);
                self.negative.retain(|n| n != &falsy_set);
            }
        }
    }

    fn build(mut self, db: &'db dyn Db) -> Type<'db> {
        self.simplify(db);
        match (self.positive.len(), self.negative.len()) {
            (0, 0) => Type::Never,
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
    use crate::types::{BytesLiteralType, KnownClass, StringLiteralType, TupleType, UnionBuilder};
    use crate::ProgramSettings;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

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

    #[test]
    fn build_union_truthy_falsy() {
        let db = setup_db();

        // `Truthy | Falsy` -> `Never` -- this probably should never happen in practice
        let t0 = UnionType::from_elements(&db, [Type::Truthy, Type::Falsy]);
        let t1 = UnionType::from_elements(&db, [Type::Truthy, Type::Falsy, Type::IntLiteral(0)]);

        assert_eq!(t0, Type::Never);
        assert_eq!(t1, Type::IntLiteral(0));
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
        assert_eq!(intersection.neg_vec(&db), &[t1]);
    }

    #[test]
    fn build_intersection_flatten_negative() {
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
            .add_negative(i0)
            .build()
            .expect_intersection();

        assert_eq!(intersection.pos_vec(&db), &[t2, t1]);
        assert_eq!(intersection.neg_vec(&db), &[ta]);
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
    }

    /// Test all tpes where `X & Falsy` or `X & Truthy` can be replaced by specific literals.
    #[test]
    fn build_intersection_simplify_to_falsy_or_truthy_literals() {
        let db = setup_db();

        let falsy_int = IntersectionBuilder::build_falsy(&db, KnownClass::Int.to_instance(&db));
        assert_eq!(falsy_int, Type::IntLiteral(0));

        let empty_str = Type::StringLiteral(StringLiteralType::new(&db, "".into()));
        let falsy_str = IntersectionBuilder::build_falsy(&db, KnownClass::Str.to_instance(&db));
        assert_eq!(falsy_str, empty_str);

        let falsy_literal_str = IntersectionBuilder::build_falsy(&db, Type::LiteralString);
        assert_eq!(falsy_literal_str, empty_str);

        let falsy_bool = IntersectionBuilder::build_falsy(&db, KnownClass::Bool.to_instance(&db));
        assert_eq!(falsy_bool, Type::BooleanLiteral(false));

        let empty_tuple = Type::Tuple(TupleType::new(&db, vec![].into()));
        let falsy_tuple = IntersectionBuilder::build_falsy(&db, KnownClass::Tuple.to_instance(&db));
        assert_eq!(falsy_tuple, empty_tuple);

        let empty_bytes = Type::BytesLiteral(BytesLiteralType::new(&db, vec![].into()));
        let falsy_bytes = IntersectionBuilder::build_falsy(&db, KnownClass::Bytes.to_instance(&db));
        assert_eq!(falsy_bytes, empty_bytes);

        // Currently the only case of known `Truthy` set
        let falsy_bool = IntersectionBuilder::build_truthy(&db, KnownClass::Bool.to_instance(&db));
        assert_eq!(falsy_bool, Type::BooleanLiteral(true));
    }

    /// Tests that we simplify
    /// - When `X` -> `AlwaysTrue`: `X & Truthy` = `X`
    /// - When `X` -> `AlwaysTrue`: `X & Falsy` = `Never`
    /// - When `X` -> `AlwaysFalse`: `X & Truthy` = `Never`
    /// - When `X` -> `AlwaysFalse`: `X & Falsy` = `X`
    #[test]
    fn build_intersection_with_truthy_or_falsy_simplifies_when_always_true_or_false() {
        let db = setup_db();

        // `X` -> `AlwaysTrue` => `X & Truthy` = `X`
        let hello_literal = Type::StringLiteral(StringLiteralType::new(&db, "hello".into()));
        assert_eq!(
            IntersectionBuilder::build_truthy(&db, hello_literal),
            hello_literal
        );

        assert_eq!(
            IntersectionBuilder::build_truthy(&db, KnownClass::FunctionType.to_instance(&db)),
            KnownClass::FunctionType.to_instance(&db)
        );

        // `X` -> `AlwaysTrue` => `X & Falsy` = `Never`
        assert_eq!(
            IntersectionBuilder::build_falsy(&db, Type::IntLiteral(8)),
            Type::Never
        );

        assert_eq!(
            IntersectionBuilder::build_falsy(
                &db,
                Type::Tuple(TupleType::new(&db, vec![Type::IntLiteral(0)].into()))
            ),
            Type::Never
        );

        // `X` -> `AlwaysFalse` => `X & Truthy` = `Never`
        // TODO: add a test case for `NoneType` when supported

        let empty_string = Type::StringLiteral(StringLiteralType::new(&db, "".into()));
        assert_eq!(
            IntersectionBuilder::build_truthy(&db, empty_string),
            Type::Never
        );

        let empty_bytes = Type::BytesLiteral(BytesLiteralType::new(&db, vec![].into()));
        assert_eq!(
            IntersectionBuilder::build_truthy(
                &db,
                UnionType::from_elements(&db, [empty_string, empty_bytes])
            ),
            Type::Never
        );

        // `X` -> `AlwaysFalse` => `X & Falsy` = `X`
        let empty_tuple = Type::Tuple(TupleType::new(&db, vec![].into()));
        assert_eq!(
            IntersectionBuilder::build_falsy(&db, empty_tuple),
            empty_tuple
        );

        assert_eq!(
            IntersectionBuilder::build_falsy(&db, empty_bytes),
            empty_bytes
        );
    }

    /// Tests that `X & !y` where `y` is the only value in `X & Falsy` simplifies to `X & Truthy`
    #[test]
    fn build_intersection_of_type_with_all_falsy_set_in_negatives() {
        let db = setup_db();

        let int_instance = KnownClass::Int.to_instance(&db);
        assert_eq!(
            IntersectionBuilder::new(&db)
                .add_positive(int_instance)
                .add_negative(Type::IntLiteral(0))
                .build(),
            IntersectionBuilder::build_truthy(&db, int_instance)
        );
    }

    #[test]
    fn build_intersection_truthy_and_falsy() {
        let db = setup_db();

        // `Truthy & Falsy` -> `Never`
        let truthy_and_falsy = IntersectionBuilder::build_truthy(&db, Type::Falsy);
        assert_eq!(truthy_and_falsy, Type::Never);
    }

    #[test]
    fn build_intersection_truthy_and_falsy_cant_be_in_negative_elements() {
        let db = setup_db();

        // `X & !Truthy` -> `X & Falsy`
        let falsy_int_negative = IntersectionBuilder::new(&db)
            .add_positive(KnownClass::Int.to_instance(&db))
            .add_negative(Type::Falsy)
            .build();
        assert_eq!(
            IntersectionBuilder::build_truthy(&db, KnownClass::Int.to_instance(&db)),
            falsy_int_negative
        );

        // `X & !Falsy` -> `X & Truthy`
        let truthy_int_negative = IntersectionBuilder::new(&db)
            .add_positive(KnownClass::Int.to_instance(&db))
            .add_negative(Type::Truthy)
            .build();
        assert_eq!(
            IntersectionBuilder::build_falsy(&db, KnownClass::Int.to_instance(&db)),
            truthy_int_negative
        );
    }

    #[test]
    fn build_union_of_type_truthy_and_type_falsy() {
        let db = setup_db();

        // `object & Falsy | object & Truthy` -> `X`
        let object_instance = KnownClass::Object.to_instance(&db);
        let object_truthy_and_object_falsy = UnionBuilder::new(&db)
            .add(IntersectionBuilder::build_truthy(&db, object_instance))
            .add(IntersectionBuilder::build_falsy(&db, object_instance))
            .build();
        assert_eq!(object_truthy_and_object_falsy, object_instance);

        // `int & Falsy | int & Truthy` -> `X`
        // This is a special case because we know that `int & False` is `{0}`, so `int & Falsy`
        // gets simplified to `Literal[0]` - but the feature should hold.
        let int_instance = KnownClass::Int.to_instance(&db);
        let int_truthy_and_int_falsy = UnionBuilder::new(&db)
            .add(IntersectionBuilder::build_truthy(&db, int_instance))
            .add(IntersectionBuilder::build_falsy(&db, int_instance))
            .build();
        assert_eq!(int_truthy_and_int_falsy, int_instance);
    }

    /// Tests building a union between `X & Truthy | y` where `y` is the only value in `X & Falsy`
    #[test]
    fn build_union_of_type_truthy_and_falsy_set() {
        let db = setup_db();

        let int_instance = KnownClass::Int.to_instance(&db);
        assert_eq!(
            UnionBuilder::new(&db)
                .add(IntersectionBuilder::build_truthy(&db, int_instance))
                .add(Type::IntLiteral(0))
                .build(),
            int_instance
        );

        let str_instance = KnownClass::Str.to_instance(&db);
        let empty_str = Type::StringLiteral(StringLiteralType::new(&db, "".into()));
        assert_eq!(
            UnionBuilder::new(&db)
                .add(empty_str)
                .add(IntersectionBuilder::build_truthy(&db, str_instance))
                .build(),
            str_instance
        );
    }
}
