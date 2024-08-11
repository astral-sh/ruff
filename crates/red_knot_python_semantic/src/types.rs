use ruff_db::files::File;
use ruff_python_ast::name::Name;

use crate::builtins::builtins_scope;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{global_scope, symbol_table, use_def_map};
use crate::{Db, FxOrderSet};

mod display;
mod infer;

pub(crate) use self::infer::{infer_definition_types, infer_scope_types};

/// Infer the public type of a symbol (its type as seen from outside its scope).
pub(crate) fn symbol_ty<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Type<'db> {
    let _span = tracing::trace_span!("symbol_ty", ?symbol).entered();

    let use_def = use_def_map(db, scope);
    definitions_ty(
        db,
        use_def.public_definitions(symbol),
        use_def
            .public_may_be_unbound(symbol)
            .then_some(Type::Unbound),
    )
}

/// Shorthand for `symbol_ty` that takes a symbol name instead of an ID.
pub(crate) fn symbol_ty_by_name<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> Type<'db> {
    let table = symbol_table(db, scope);
    table
        .symbol_id_by_name(name)
        .map(|symbol| symbol_ty(db, scope, symbol))
        .unwrap_or(Type::Unbound)
}

/// Shorthand for `symbol_ty` that looks up a module-global symbol by name in a file.
pub(crate) fn global_symbol_ty_by_name<'db>(db: &'db dyn Db, file: File, name: &str) -> Type<'db> {
    symbol_ty_by_name(db, global_scope(db, file), name)
}

/// Shorthand for `symbol_ty` that looks up a symbol in the builtins.
///
/// Returns `Unbound` if the builtins module isn't available for some reason.
pub(crate) fn builtins_symbol_ty_by_name<'db>(db: &'db dyn Db, name: &str) -> Type<'db> {
    builtins_scope(db)
        .map(|builtins| symbol_ty_by_name(db, builtins, name))
        .unwrap_or(Type::Unbound)
}

/// Infer the type of a [`Definition`].
pub(crate) fn definition_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.definition_ty(definition)
}

/// Infer the combined type of an array of [`Definition`]s, plus one optional "unbound type".
///
/// Will return a union if there is more than one definition, or at least one plus an unbound
/// type.
///
/// The "unbound type" represents the type in case control flow may not have passed through any
/// definitions in this scope. If this isn't possible, then it will be `None`. If it is possible,
/// and the result in that case should be Unbound (e.g. an unbound function local), then it will be
/// `Some(Type::Unbound)`. If it is possible and the result should be something else (e.g. an
/// implicit global lookup), then `unbound_type` will be `Some(the_global_symbol_type)`.
///
/// # Panics
/// Will panic if called with zero definitions and no `unbound_ty`. This is a logic error,
/// as any symbol with zero visible definitions clearly may be unbound, and the caller should
/// provide an `unbound_ty`.
pub(crate) fn definitions_ty<'db>(
    db: &'db dyn Db,
    definitions: &[Definition<'db>],
    unbound_ty: Option<Type<'db>>,
) -> Type<'db> {
    let def_types = definitions.iter().map(|def| definition_ty(db, *def));
    let mut all_types = unbound_ty.into_iter().chain(def_types);

    let Some(first) = all_types.next() else {
        panic!("definitions_ty should never be called with zero definitions and no unbound_ty.")
    };

    if let Some(second) = all_types.next() {
        let mut builder = UnionBuilder::new(db);
        builder = builder.add(first).add(second);

        for variant in all_types {
            builder = builder.add(variant);
        }

        builder.build()
    } else {
        first
    }
}

/// Unique ID for a type.
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum Type<'db> {
    /// the dynamic type: a statically-unknown set of values
    Any,
    /// the empty set of values
    Never,
    /// unknown type (no annotation)
    /// equivalent to Any, or possibly to object in strict mode
    Unknown,
    /// name does not exist or is not bound to any value (this represents an error, but with some
    /// leniency options it could be silently resolved to Unknown in some cases)
    Unbound,
    /// the None object -- TODO remove this in favor of Instance(types.NoneType)
    None,
    /// a specific function object
    Function(FunctionType<'db>),
    /// a specific module object
    Module(File),
    /// a specific class object
    Class(ClassType<'db>),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(ClassType<'db>),
    /// the set of objects in any of the types in the union
    Union(UnionType<'db>),
    /// the set of objects in all of the types in the intersection
    Intersection(IntersectionType<'db>),
    /// An integer literal
    IntLiteral(i64),
    /// A boolean literal, either `True` or `False`.
    BooleanLiteral(bool),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl<'db> Type<'db> {
    pub const fn is_unbound(&self) -> bool {
        matches!(self, Type::Unbound)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    #[must_use]
    pub fn member(&self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            Type::Never => todo!("attribute lookup on Never type"),
            Type::Unknown => Type::Unknown,
            Type::Unbound => Type::Unbound,
            Type::None => todo!("attribute lookup on None type"),
            Type::Function(_) => todo!("attribute lookup on Function type"),
            Type::Module(file) => global_symbol_ty_by_name(db, *file, name),
            Type::Class(class) => class.class_member(db, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                todo!("attribute lookup on Instance type")
            }
            Type::Union(union) => union
                .elements(db)
                .iter()
                .fold(UnionBuilder::new(db), |builder, element_ty| {
                    builder.add(element_ty.member(db, name))
                })
                .build(),
            Type::Intersection(_) => {
                // TODO perform the get_member on each type in the intersection
                // TODO return the intersection of those results
                todo!("attribute lookup on Intersection type")
            }
            Type::IntLiteral(_) => {
                // TODO raise error
                Type::Unknown
            }
            Type::BooleanLiteral(_) => Type::Unknown,
        }
    }

    #[must_use]
    pub fn instance(&self) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            Type::Class(class) => Type::Instance(*class),
            _ => Type::Unknown, // TODO type errors
        }
    }
}

#[salsa::interned]
pub struct FunctionType<'db> {
    /// name of the function at definition
    pub name: Name,

    /// types of all decorators on this function
    decorators: Vec<Type<'db>>,
}

impl<'db> FunctionType<'db> {
    pub fn has_decorator(self, db: &dyn Db, decorator: Type<'_>) -> bool {
        self.decorators(db).contains(&decorator)
    }
}

#[salsa::interned]
pub struct ClassType<'db> {
    /// Name of the class at definition
    pub name: Name,

    /// Types of all class bases
    bases: Vec<Type<'db>>,

    body_scope: ScopeId<'db>,
}

impl<'db> ClassType<'db> {
    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member of the class itself or any of its bases.
    pub fn class_member(self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        let member = self.own_class_member(db, name);
        if !member.is_unbound() {
            return member;
        }

        self.inherited_class_member(db, name)
    }

    /// Returns the inferred type of the class member named `name`.
    pub fn own_class_member(self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        let scope = self.body_scope(db);
        symbol_ty_by_name(db, scope, name)
    }

    pub fn inherited_class_member(self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        for base in self.bases(db) {
            let member = base.member(db, name);
            if !member.is_unbound() {
                return member;
            }
        }

        Type::Unbound
    }
}

#[salsa::interned]
pub struct UnionType<'db> {
    /// the union type includes values in any of these types
    elements: FxOrderSet<Type<'db>>,
}

impl<'db> UnionType<'db> {
    pub fn contains(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        self.elements(db).contains(&ty)
    }
}

struct UnionBuilder<'db> {
    elements: FxOrderSet<Type<'db>>,
    db: &'db dyn Db,
}

impl<'db> UnionBuilder<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            elements: FxOrderSet::default(),
        }
    }

    /// Adds a type to this union.
    fn add(mut self, ty: Type<'db>) -> Self {
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

    fn build(self) -> Type<'db> {
        match self.elements.len() {
            0 => Type::Never,
            1 => self.elements[0],
            _ => Type::Union(UnionType::new(self.db, self.elements)),
        }
    }
}

// Negation types aren't expressible in annotations, and are most likely to arise from type
// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
// directly in intersections rather than as a separate type. This sacrifices some efficiency in the
// case where a Not appears outside an intersection (unclear when that could even happen, but we'd
// have to represent it as a single-element intersection if it did) in exchange for better
// efficiency in the within-intersection case.
#[salsa::interned]
pub struct IntersectionType<'db> {
    // the intersection type includes only values in all of these types
    positive: FxOrderSet<Type<'db>>,
    // the intersection type does not include any value in any of these types
    negative: FxOrderSet<Type<'db>>,
}

#[allow(unused)]
#[derive(Clone)]
struct IntersectionBuilder<'db> {
    // Really this builds a union-of-intersections, because we always keep our set-theoretic types
    // in disjunctive normal form (DNF), a union of intersections. In the simplest case there's
    // just a single intersection in this vector, and we are building a single intersection type,
    // but if a union is added to the intersection, we'll distribute ourselves over that union and
    // create a union of intersections.
    intersections: Vec<InnerIntersectionBuilder<'db>>,
    db: &'db dyn Db,
}

impl<'db> IntersectionBuilder<'db> {
    #[allow(dead_code)]
    fn new(db: &'db dyn Db) -> Self {
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

    #[allow(dead_code)]
    fn add_positive(mut self, ty: Type<'db>) -> Self {
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

    #[allow(dead_code)]
    fn add_negative(mut self, ty: Type<'db>) -> Self {
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

    #[allow(dead_code)]
    fn build(self) -> Type<'db> {
        let mut builder = UnionBuilder::new(self.db);
        for inner in self.intersections {
            builder = builder.add(inner.build(self.db));
        }
        builder.build()
    }
}

#[allow(unused)]
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
            self.positive.clear();
            self.negative.clear();
            self.positive.insert(Type::Never);
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

    fn setup_db() -> TestDb {
        TestDb::new()
    }

    impl<'db> UnionType<'db> {
        fn elements_vec(self, db: &'db TestDb) -> Vec<Type<'db>> {
            self.elements(db).into_iter().collect()
        }
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
}
