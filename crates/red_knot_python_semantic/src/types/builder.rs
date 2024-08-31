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

use super::builtins_symbol_ty_by_name;

pub(crate) struct UnionBuilder<'db> {
    elements: FxOrderSet<Type<'db>>,
    db: &'db dyn Db,
}

impl<'db> UnionBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            elements: FxOrderSet::default(),
        }
    }

    /// Adds a type to this union.
    pub(crate) fn add(mut self, ty: Type<'db>) -> Self {
        match ty {
            Type::Union(union) => {
                self.elements.extend(&union.elements(self.db));
            }
            Type::Never => {}
            _ => {
                self.elements.insert(ty);
            }
        }

        self
    }

    pub(crate) fn build(self) -> Type<'db> {
        match self.elements.len() {
            0 => Type::Never,
            1 => self.elements[0],
            2 if self.elements.contains(&Type::BooleanLiteral(true))
                && self.elements.contains(&Type::BooleanLiteral(false)) =>
            {
                let ty = builtins_symbol_ty_by_name(self.db, "bool");
                if ty.is_unbound() {
                    Type::Union(UnionType::new(self.db, self.elements))
                } else {
                    ty
                }
            }
            _ => Type::Union(UnionType::new(self.db, self.elements)),
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
            let mut builder = UnionBuilder::new(self.db);
            for inner in self.intersections {
                builder = builder.add(inner.build(self.db));
            }
            builder.build()
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
        // TODO Any/Unknown actually should not self-cancel
        match ty {
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

    fn simplify(&mut self) {
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

        // None intersects only with object
        for pos in &self.positive {
            if let Type::Instance(_) = pos {
                // could be `object` type
            } else {
                self.negative.remove(&Type::None);
                break;
            }
        }
    }

    fn build(mut self, db: &'db dyn Db) -> Type<'db> {
        self.simplify();
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
    use super::{IntersectionBuilder, IntersectionType, Type, UnionBuilder, UnionType};
    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::types::builtins_symbol_ty_by_name;
    use crate::ProgramSettings;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

    fn setup_db() -> TestDb {
        TestDb::new()
    }

    impl<'db> UnionType<'db> {
        fn elements_vec(self, db: &'db TestDb) -> Vec<Type<'db>> {
            self.elements(db).into_iter().collect()
        }
    }

    fn setup_db_with_python() -> TestDb {
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
        let Type::Union(union) = UnionBuilder::new(&db).add(t0).add(t1).build() else {
            panic!("expected a union");
        };

        assert_eq!(union.elements_vec(&db), &[t0, t1]);
    }

    #[test]
    fn build_union_single() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let ty = UnionBuilder::new(&db).add(t0).build();

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
        let ty = UnionBuilder::new(&db).add(t0).add(Type::Never).build();

        assert_eq!(ty, t0);
    }

    #[test]
    fn build_union_bool() {
        let db = setup_db_with_python();
        let ty_expect = builtins_symbol_ty_by_name(&db, "bool");
        let t0 = Type::BooleanLiteral(true);
        let t1 = Type::BooleanLiteral(true);
        let t2 = Type::BooleanLiteral(false);
        let ty_true = UnionBuilder::new(&db).add(t0).add(t1).build();
        let ty = UnionBuilder::new(&db).add(t0).add(t1).add(t2).build();

        assert_eq!(ty_true, Type::BooleanLiteral(true));
        assert_eq!(ty, ty_expect);
    }

    #[test]
    fn build_union_flatten() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let t1 = Type::IntLiteral(1);
        let t2 = Type::IntLiteral(2);
        let u1 = UnionBuilder::new(&db).add(t0).add(t1).build();
        let Type::Union(union) = UnionBuilder::new(&db).add(u1).add(t2).build() else {
            panic!("expected a union");
        };

        assert_eq!(union.elements_vec(&db), &[t0, t1, t2]);
    }

    impl<'db> IntersectionType<'db> {
        fn pos_vec(self, db: &'db TestDb) -> Vec<Type<'db>> {
            self.positive(db).into_iter().collect()
        }

        fn neg_vec(self, db: &'db TestDb) -> Vec<Type<'db>> {
            self.negative(db).into_iter().collect()
        }
    }

    #[test]
    fn build_intersection() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let ta = Type::Any;
        let Type::Intersection(inter) = IntersectionBuilder::new(&db)
            .add_positive(ta)
            .add_negative(t0)
            .build()
        else {
            panic!("expected to be an intersection");
        };

        assert_eq!(inter.pos_vec(&db), &[ta]);
        assert_eq!(inter.neg_vec(&db), &[t0]);
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
        let Type::Intersection(inter) = IntersectionBuilder::new(&db)
            .add_positive(t2)
            .add_positive(i0)
            .build()
        else {
            panic!("expected to be an intersection");
        };

        assert_eq!(inter.pos_vec(&db), &[t2, ta]);
        assert_eq!(inter.neg_vec(&db), &[t1]);
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
        let Type::Intersection(inter) = IntersectionBuilder::new(&db)
            .add_positive(t2)
            .add_negative(i0)
            .build()
        else {
            panic!("expected to be an intersection");
        };

        assert_eq!(inter.pos_vec(&db), &[t2, t1]);
        assert_eq!(inter.neg_vec(&db), &[ta]);
    }

    #[test]
    fn intersection_distributes_over_union() {
        let db = setup_db();
        let t0 = Type::IntLiteral(0);
        let t1 = Type::IntLiteral(1);
        let ta = Type::Any;
        let u0 = UnionBuilder::new(&db).add(t0).add(t1).build();

        let Type::Union(union) = IntersectionBuilder::new(&db)
            .add_positive(ta)
            .add_positive(u0)
            .build()
        else {
            panic!("expected a union");
        };
        let [Type::Intersection(i0), Type::Intersection(i1)] = union.elements_vec(&db)[..] else {
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
}
