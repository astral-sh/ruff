//! Core representation and operations for recursive types.
//!
//! A recursive type is represented as `mu binder. body`, where recursive
//! references inside `body` are `Type::Divergent` markers carrying `binder`.
//! Structural operations on a recursive type must not use the raw body directly:
//! they should unfold one layer, perform the operation, and fold the resulting
//! type back under the same binder.

use salsa::plumbing::AsId;

use crate::Db;
use crate::place::PlaceAndQualifiers;
use crate::types::{Type, TypeAliasType, TypeContext, TypeMapping};

/// Identifier for the bound variable of a recursive type.
///
/// A recursive type is represented as `mu binder. body`, where occurrences of
/// `Type::Divergent` carrying this binder identify recursive references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct BinderId(salsa::Id);

// `salsa::Id` is an index into Salsa storage, whose memory is tracked separately.
impl get_size2::GetSize for BinderId {}

impl BinderId {
    pub(crate) const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    pub(crate) const fn into_id(self) -> salsa::Id {
        self.0
    }
}

/// Source of a recursive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum RecursiveOrigin<'db> {
    /// A structural recursion not directly tied to a named alias.
    Implicit,
    /// Recursion introduced while resolving a type alias.
    TypeAlias(TypeAliasType<'db>),
}

impl<'db> RecursiveOrigin<'db> {
    pub(crate) fn source_type(self) -> Option<Type<'db>> {
        match self {
            Self::Implicit => None,
            Self::TypeAlias(alias) => Some(Type::TypeAlias(alias)),
        }
    }

    pub(crate) fn matches_type(self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        match (self, ty) {
            (Self::Implicit, _) => false,
            (Self::TypeAlias(alias), Type::TypeAlias(other)) => {
                alias.definition(db) == other.definition(db)
            }
            _ => false,
        }
    }

    pub(crate) fn contains_in_type(self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        crate::types::visitor::any_over_type(db, ty, false, |inner| self.matches_type(db, inner))
    }

    pub(crate) fn binder_id(self, db: &'db dyn Db) -> Option<salsa::Id> {
        match self {
            Self::Implicit => None,
            Self::TypeAlias(TypeAliasType::PEP695(alias)) => Some(alias.as_id()),
            Self::TypeAlias(alias) => Some(alias.definition(db).as_id()),
        }
    }
}

pub(crate) trait Foldable<'db>: Sized {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self;
}

impl<'db> Foldable<'db> for Type<'db> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        rec.fold(db, self)
    }
}

impl<'db> Foldable<'db> for () {
    fn fold(self, _db: &'db dyn Db, _rec: RecursiveType<'db>) -> Self {}
}

impl<'db> Foldable<'db> for bool {
    fn fold(self, _db: &'db dyn Db, _rec: RecursiveType<'db>) -> Self {
        self
    }
}

impl<'db> Foldable<'db> for usize {
    fn fold(self, _db: &'db dyn Db, _rec: RecursiveType<'db>) -> Self {
        self
    }
}

impl<'db> Foldable<'db> for PlaceAndQualifiers<'db> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        self.map_type(|ty| rec.fold(db, ty))
    }
}

impl<'db, F: Foldable<'db>> Foldable<'db> for Option<F> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        self.map(|inner| inner.fold(db, rec))
    }
}

impl<'db, F: Foldable<'db>> Foldable<'db> for Box<F> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        Box::new((*self).fold(db, rec))
    }
}

impl<'db, F: Foldable<'db>> Foldable<'db> for Box<[F]> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        self.into_vec()
            .into_iter()
            .map(|inner| inner.fold(db, rec))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    }
}

impl<'db, F: Foldable<'db>> Foldable<'db> for Vec<F> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        self.into_iter()
            .map(|inner| inner.fold(db, rec))
            .collect::<Vec<_>>()
    }
}

impl<'db, T: Foldable<'db>, E: Foldable<'db>> Foldable<'db> for Result<T, E> {
    fn fold(self, db: &'db dyn Db, rec: RecursiveType<'db>) -> Self {
        match self {
            Ok(ok) => Ok(ok.fold(db, rec)),
            Err(err) => Err(err.fold(db, rec)),
        }
    }
}

/// A recursive type `mu binder. body`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    pub binder: BinderId,
    pub origin: RecursiveOrigin<'db>,
    pub body: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for RecursiveType<'_> {}

impl<'db> RecursiveType<'db> {
    pub(crate) fn build(
        db: &'db dyn Db,
        binder_id: salsa::Id,
        origin: RecursiveOrigin<'db>,
        body: Type<'db>,
    ) -> Type<'db> {
        Type::Recursive(Self::new(db, BinderId::new(binder_id), origin, body))
    }

    pub(crate) fn binder_id(self, db: &'db dyn Db) -> salsa::Id {
        self.binder(db).into_id()
    }

    /// Returns the body with recursive-position markers replaced by the source type when known.
    ///
    /// This is for finite views such as display. Structural type operations should use
    /// [`map`](Self::map), which preserves recursive positions as this recursive type.
    pub fn body_with_origin_marker(self, db: &'db dyn Db) -> Type<'db> {
        let body = self.body(db);
        let Some(replacement) = self.origin(db).source_type() else {
            return body;
        };
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement,
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }

    fn unfold(self, db: &'db dyn Db) -> Type<'db> {
        let body = self.body(db);
        let replacement = self.unfold_replacement(db);
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement,
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }

    fn unfold_replacement(self, db: &'db dyn Db) -> Type<'db> {
        self.origin(db)
            .source_type()
            .unwrap_or(Type::Recursive(self))
    }

    fn fold(self, db: &'db dyn Db, unfolded_result: Type<'db>) -> Type<'db> {
        let replacement = self.unfold_replacement(db);
        if unfolded_result == replacement || unfolded_result == Type::Recursive(self) {
            return unfolded_result;
        }
        let mapping = TypeMapping::FoldRecursive {
            recursive: self,
            replacement,
        };
        let folded_body = unfolded_result.apply_type_mapping(db, &mapping, TypeContext::default());
        let marker = Type::divergent(self.binder_id(db));
        if folded_body == self.body(db) {
            Type::Recursive(self)
        } else {
            match self.origin(db) {
                RecursiveOrigin::Implicit => folded_body.drop_top_level_cycle_markers(db, marker),
                RecursiveOrigin::TypeAlias(_) if folded_body.contains_cycle_marker(db, marker) => {
                    let mapping = TypeMapping::ReplaceDivergent {
                        binder_id: self.binder(db),
                        replacement,
                    };
                    folded_body.apply_type_mapping(db, &mapping, TypeContext::default())
                }
                RecursiveOrigin::TypeAlias(_) => folded_body,
            }
        }
    }

    /// Apply an operation to one unfolded layer, then fold the result back under this binder.
    pub(crate) fn map<F: Foldable<'db>>(
        self,
        db: &'db dyn Db,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> F {
        operation(self.unfold(db)).fold(db, self)
    }

    /// Whether this recursive type is the non-contractive `mu a. a`.
    pub(crate) fn is_non_contractive(self, db: &'db dyn Db) -> bool {
        self.body(db) == Type::divergent(self.binder_id(db))
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast as ast;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::{KnownClass, UnionType, visitor};

    #[test]
    fn map_folds_operation_result_back_to_recursive_type() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let element = recursive.map(&db, |unfolded| {
            unfolded
                .subscript(&db, Type::int_literal(0), ast::ExprContext::Load)
                .expect("tuple subscript should succeed")
        });

        assert_eq!(element, recursive_ty);
    }

    #[test]
    fn fold_rebinds_unfolded_recursive_positions() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let unfolded_body = Type::homogeneous_tuple(&db, recursive_ty);

        assert_eq!(recursive.fold(&db, unfolded_body), recursive_ty);
    }

    #[test]
    fn fold_maps_implicit_derived_recursive_positions_to_markers() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let derived = KnownClass::List.to_specialized_instance(&db, &[recursive_ty]);
        let expected = KnownClass::List.to_specialized_instance(&db, &[Type::divergent(binder_id)]);

        assert_eq!(recursive.fold(&db, derived), expected);
    }

    #[test]
    fn fold_preserves_non_recursive_union_members_in_nested_generic_arguments() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let element = UnionType::from_elements(&db, [Type::int_literal(1), recursive_ty]);
        let derived = KnownClass::List.to_specialized_instance(&db, &[element]);
        let expected = KnownClass::List.to_specialized_instance(
            &db,
            &[UnionType::from_elements(
                &db,
                [Type::int_literal(1), Type::divergent(binder_id)],
            )],
        );
        let folded = recursive.fold(&db, derived);

        assert_eq!(folded, expected);
    }

    #[test]
    fn recursive_constructor_simplifies_unused_binder() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::int_literal(1));

        assert_eq!(
            Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body),
            body
        );
    }

    #[test]
    fn default_visitors_do_not_unfold_recursive_types() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let unfolded_once = Type::homogeneous_tuple(&db, recursive_ty);

        assert!(!visitor::any_over_type(&db, recursive_ty, false, |ty| {
            ty == unfolded_once
        }));
        assert!(visitor::any_over_type(&db, recursive_ty, false, |ty| {
            ty.is_divergent()
        }));
    }
}
