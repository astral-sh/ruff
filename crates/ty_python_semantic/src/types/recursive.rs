use crate::Db;
use crate::types::visitor;
use crate::types::{
    ApplyTypeMappingVisitor, DivergentType, Type, TypeAliasType, TypeContext, TypeMapping,
};

/// The source that introduced a recursive type.
///
/// This is display metadata only; type operations must not distinguish recursive types by origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum RecursiveOrigin<'db> {
    Implicit,
    TypeAlias(TypeAliasType<'db>),
}

/// A recursive type `mu binder. body`, represented with occurrences of `binder` in `body`.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    pub binder: DivergentType,
    pub origin: RecursiveOrigin<'db>,
    pub body: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for RecursiveType<'_> {}

pub(crate) fn walk_recursive_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    recursive: RecursiveType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, recursive.body(db));
}

impl<'db> RecursiveType<'db> {
    #[expect(
        clippy::new_ret_no_self,
        reason = "The constructor canonicalizes away unused binders."
    )]
    pub(crate) fn new(
        db: &'db dyn Db,
        binder: DivergentType,
        origin: RecursiveOrigin<'db>,
        body: Type<'db>,
    ) -> Type<'db> {
        if body.contains_divergent_marker(db, binder) {
            Type::Recursive(Self::new_internal(db, binder, origin, body))
        } else {
            body
        }
    }

    pub(crate) fn has_same_binder(self, db: &'db dyn Db, other: Self) -> bool {
        Type::Divergent(self.binder(db)).same_divergent_marker(Type::Divergent(other.binder(db)))
    }

    pub(crate) fn unfolded(self, db: &'db dyn Db) -> Type<'db> {
        self.body(db).unfold_recursive(db, self)
    }

    pub(crate) fn map_type(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        let body = self
            .body(db)
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        Self::new(db, self.binder(db), self.origin(db), body)
    }

    pub(crate) fn map_if_unfolded<T>(
        self,
        db: &'db dyn Db,
        map: impl FnOnce(Type<'db>) -> T,
    ) -> Option<T> {
        let unfolded = self.unfolded(db);
        if unfolded == Type::Recursive(self) {
            None
        } else {
            Some(map(unfolded))
        }
    }

    pub(crate) fn map_or_else<T>(
        self,
        db: &'db dyn Db,
        default: impl FnOnce() -> T,
        map: impl FnOnce(Type<'db>) -> T,
    ) -> T {
        self.map_if_unfolded(db, map).unwrap_or_else(default)
    }
}

pub trait Foldable<'db>: Sized {
    #[must_use]
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self;

    #[must_use]
    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self;
}

impl<'db> Foldable<'db> for Type<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.apply_type_mapping_impl(
            db,
            &TypeMapping::FoldRecursive(recursive),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        )
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.apply_type_mapping_impl(
            db,
            &TypeMapping::UnfoldRecursive(recursive),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        )
    }
}
