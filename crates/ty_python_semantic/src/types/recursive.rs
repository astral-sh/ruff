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
                alias.has_same_definition(db, other)
            }
            _ => false,
        }
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
    /// [`map_or_else`](Self::map_or_else), which preserves recursive positions as this recursive
    /// type.
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

    /// `μa. body -> [a -> μa. body]body`
    /// e.g. `μa. tuple[a, int] -> tuple[μa. tuple[a, int], int]`
    fn unfold(self, db: &'db dyn Db) -> Type<'db> {
        let body = self.body(db);
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement: Type::Recursive(self),
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }

    /// `T[μa. body] -> μa. T[a]`
    /// e.g. `tuple[μa. tuple[a, int], int] -> μa. tuple[a, int]`
    pub(crate) fn fold(self, db: &'db dyn Db, unfolded_result: Type<'db>) -> Type<'db> {
        let mapping = TypeMapping::FoldRecursive {
            recursive: self,
            replacement: Type::Recursive(self),
        };
        let folded_body = unfolded_result.apply_type_mapping(db, &mapping, TypeContext::default());
        let marker = Type::divergent(self.binder_id(db));
        if folded_body == unfolded_result && !folded_body.contains_cycle_marker(db, marker) {
            unfolded_result
        } else {
            Type::recursive(db, self.binder_id(db), self.origin(db), folded_body)
        }
    }

    /// Apply an operation only if one-step unfolding exposes a new outer structure.
    pub(crate) fn map_if_unfolded<F: Foldable<'db>>(
        self,
        db: &'db dyn Db,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> Option<F> {
        let unfolded = self.unfold(db);
        if unfolded == Type::Recursive(self) {
            None
        } else {
            Some(operation(unfolded).fold(db, self))
        }
    }

    /// Apply an operation, or use a caller-provided fallback if unfolding makes no progress.
    pub(crate) fn map_or_else<F: Foldable<'db>>(
        self,
        db: &'db dyn Db,
        fallback: impl FnOnce() -> F,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> F {
        self.map_if_unfolded(db, operation).unwrap_or_else(fallback)
    }

    /// Apply a type-producing operation if one-step unfolding exposes a new outer structure.
    pub(crate) fn map_type(
        self,
        db: &'db dyn Db,
        operation: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.map_or_else(db, || Type::Recursive(self), operation)
    }

    /// Inspect one unfolded layer, or use a caller-provided fallback if unfolding makes no progress.
    pub(crate) fn project_or_else<F>(
        self,
        db: &'db dyn Db,
        fallback: impl FnOnce() -> F,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> F {
        let unfolded = self.unfold(db);
        if unfolded == Type::Recursive(self) {
            fallback()
        } else {
            operation(unfolded)
        }
    }

    /// Whether this recursive type is the non-contractive `mu a. a`.
    pub(crate) fn is_non_contractive(self, db: &'db dyn Db) -> bool {
        self.body(db) == Type::divergent(self.binder_id(db))
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_python_ast as ast;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::{
        CallableType, KnownClass, KnownInstanceType, Parameters, Signature, TypeAliasType,
        UnionType, visitor,
    };

    #[test]
    fn project_reads_from_one_unfolded_layer() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let element = recursive.project_or_else(
            &db,
            || panic!("recursive type should unfold"),
            |unfolded| {
                unfolded
                    .subscript(&db, Type::int_literal(0), ast::ExprContext::Load)
                    .expect("tuple subscript should succeed")
            },
        );

        assert_eq!(element, recursive_ty);
    }

    #[test]
    fn fold_closes_top_level_recursive_occurrence() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let folded = recursive.fold(&db, recursive_ty);
        let Type::Recursive(folded_recursive) = folded else {
            panic!("expected non-contractive recursive type");
        };

        assert!(folded_recursive.is_non_contractive(&db));
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
    fn map_or_else_identity_preserves_implicit_recursive_type() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        assert_eq!(
            recursive.map_or_else(&db, || Type::Recursive(recursive), |unfolded| unfolded),
            recursive_ty
        );
    }

    #[test]
    fn body_with_origin_marker_restores_explicit_recursive_alias_positions() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "type RecursiveList = list[RecursiveList]")
            .unwrap();

        let module = system_path_to_file(&db, "/src/a.py").unwrap();
        let alias_ty = global_symbol(&db, module, "RecursiveList")
            .place
            .expect_type();
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(alias))) =
            alias_ty
        else {
            panic!("expected RecursiveList to be a PEP 695 type alias");
        };
        let Type::Recursive(recursive) = alias.value_type(&db) else {
            panic!("expected RecursiveList to resolve to a recursive type");
        };

        assert_eq!(
            recursive.body(&db).display(&db).to_string(),
            "list[Divergent]"
        );
        assert_eq!(
            recursive
                .body_with_origin_marker(&db)
                .display(&db)
                .to_string(),
            "list[RecursiveList]"
        );
    }

    #[test]
    fn map_or_else_identity_preserves_explicit_recursive_alias_type() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "type RecursiveList = list[RecursiveList]")
            .unwrap();

        let module = system_path_to_file(&db, "/src/a.py").unwrap();
        let alias_ty = global_symbol(&db, module, "RecursiveList")
            .place
            .expect_type();
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(alias))) =
            alias_ty
        else {
            panic!("expected RecursiveList to be a PEP 695 type alias");
        };
        let recursive_ty = alias.value_type(&db);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected RecursiveList to resolve to a recursive type");
        };

        assert_eq!(
            recursive.map_or_else(&db, || Type::Recursive(recursive), |unfolded| unfolded),
            recursive_ty
        );
    }

    #[test]
    fn callable_fold_closes_signature_types() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::single_callable(
            &db,
            Signature::new(Parameters::empty(), Type::divergent(binder_id)),
        );
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let unfolded_callable =
            CallableType::single(&db, Signature::new(Parameters::empty(), recursive_ty));
        let folded_callable = unfolded_callable.fold(&db, recursive);
        let return_ty = folded_callable
            .signatures(&db)
            .overload_return_type_or_unknown(&db);
        let Type::Recursive(return_recursive) = return_ty else {
            panic!("expected folded return type to remain recursive");
        };

        assert!(return_recursive.is_non_contractive(&db));
    }

    #[test]
    fn fold_closes_recursive_positions_inside_operation_results() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let derived = KnownClass::List.to_specialized_instance(&db, &[recursive_ty]);
        let expected_body =
            KnownClass::List.to_specialized_instance(&db, &[Type::divergent(binder_id)]);
        let expected = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, expected_body);

        assert_eq!(recursive.fold(&db, derived), expected);
    }

    #[test]
    fn fold_preserves_non_recursive_union_members_in_closed_result() {
        let db = setup_db();
        let binder_id = salsa::plumbing::Id::from_bits(1);
        let body = Type::homogeneous_tuple(&db, Type::divergent(binder_id));
        let recursive_ty = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, body);
        let Type::Recursive(recursive) = recursive_ty else {
            panic!("expected recursive type");
        };

        let element = UnionType::from_elements(&db, [Type::int_literal(1), recursive_ty]);
        let derived = KnownClass::List.to_specialized_instance(&db, &[element]);
        let expected_element =
            UnionType::from_elements(&db, [Type::int_literal(1), Type::divergent(binder_id)]);
        let expected_body = KnownClass::List.to_specialized_instance(&db, &[expected_element]);
        let expected = Type::recursive(&db, binder_id, RecursiveOrigin::Implicit, expected_body);
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
