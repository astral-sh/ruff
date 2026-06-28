use std::borrow::Cow;

use crate::Db;
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::visitor;
use crate::types::{
    ApplyTypeMappingVisitor, CallableTypes, DivergentType, GeneratorTypes, PropertyInstanceType,
    TupleSpec, Type, TypeAliasType, TypeContext, TypeMapping,
    generics::{ApplySpecialization, Specialization, SpecializationError},
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
        let body = body.apply_type_mapping(
            db,
            &TypeMapping::ReplaceRecursiveBinder(binder),
            TypeContext::default(),
        );
        if body.contains_divergent_marker(db, binder) {
            Type::Recursive(Self::new_internal(db, binder, origin, body))
        } else {
            body
        }
    }

    pub(crate) fn has_same_binder(self, db: &'db dyn Db, other: Self) -> bool {
        Type::Divergent(self.binder(db)).same_divergent_marker(Type::Divergent(other.binder(db)))
    }

    pub(crate) fn has_same_binder_marker(self, db: &'db dyn Db, binder: DivergentType) -> bool {
        Type::Divergent(self.binder(db)).same_divergent_marker(Type::Divergent(binder))
    }

    /// Return the recursive body with occurrences of the binder replaced by this recursive type.
    pub fn unfolded(self, db: &'db dyn Db) -> Type<'db> {
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
        Self::new(db, self.binder(db), self.map_origin(db, type_mapping), body)
    }

    fn map_origin(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
    ) -> RecursiveOrigin<'db> {
        let origin = self.origin(db);
        let RecursiveOrigin::TypeAlias(alias) = origin else {
            return origin;
        };

        let specialization = match type_mapping {
            TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(specialization))
            | TypeMapping::ApplySpecializationWithMaterialization {
                specialization: ApplySpecialization::TypeAlias(specialization),
                ..
            } => *specialization,
            _ => return origin,
        };

        RecursiveOrigin::TypeAlias(alias.apply_specialization(db, |_| specialization))
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

    pub(crate) fn map_or_else_folded<T: Foldable<'db>>(
        self,
        db: &'db dyn Db,
        default: impl FnOnce() -> T,
        map: impl FnOnce(Type<'db>) -> T,
    ) -> T {
        self.map_or_else(db, default, |unfolded| {
            map(unfolded).fold_recursive(db, self)
        })
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

impl<'db, T> Foldable<'db> for Option<T>
where
    T: Foldable<'db>,
{
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map(|value| value.fold_recursive(db, recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map(|value| value.unfold_recursive(db, recursive))
    }
}

impl<'db, T, U> Foldable<'db> for (T, U)
where
    T: Foldable<'db>,
    U: Foldable<'db>,
{
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        (
            self.0.fold_recursive(db, recursive),
            self.1.fold_recursive(db, recursive),
        )
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        (
            self.0.unfold_recursive(db, recursive),
            self.1.unfold_recursive(db, recursive),
        )
    }
}

impl<'db, T, E> Foldable<'db> for Result<T, E>
where
    T: Foldable<'db>,
    E: Foldable<'db>,
{
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map(|value| value.fold_recursive(db, recursive))
            .map_err(|err| err.fold_recursive(db, recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map(|value| value.unfold_recursive(db, recursive))
            .map_err(|err| err.unfold_recursive(db, recursive))
    }
}

impl<'db> Foldable<'db> for Place<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map_type(|ty| ty.fold_recursive(db, recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map_type(|ty| ty.unfold_recursive(db, recursive))
    }
}

impl<'db> Foldable<'db> for PlaceAndQualifiers<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map_type(|ty| ty.fold_recursive(db, recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map_type(|ty| ty.unfold_recursive(db, recursive))
    }
}

impl<'db> Foldable<'db> for GeneratorTypes<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        Self {
            yield_ty: self.yield_ty.fold_recursive(db, recursive),
            send_ty: self.send_ty.fold_recursive(db, recursive),
            return_ty: self.return_ty.fold_recursive(db, recursive),
        }
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        Self {
            yield_ty: self.yield_ty.unfold_recursive(db, recursive),
            send_ty: self.send_ty.unfold_recursive(db, recursive),
            return_ty: self.return_ty.unfold_recursive(db, recursive),
        }
    }
}

impl<'db> Foldable<'db> for CallableTypes<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map(|callable| callable.fold_recursive(db, recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.map(|callable| callable.unfold_recursive(db, recursive))
    }
}

impl<'db> Foldable<'db> for PropertyInstanceType<'db> {
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

impl<'db> Foldable<'db> for Specialization<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.apply_type_mapping(db, &TypeMapping::FoldRecursive(recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        self.apply_type_mapping(db, &TypeMapping::UnfoldRecursive(recursive))
    }
}

impl<'db> Foldable<'db> for SpecializationError<'db> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        match self {
            Self::MismatchedBound {
                bound_typevar,
                argument,
            } => Self::MismatchedBound {
                bound_typevar,
                argument: argument.fold_recursive(db, recursive),
            },
            Self::MismatchedConstraint {
                bound_typevar,
                argument,
            } => Self::MismatchedConstraint {
                bound_typevar,
                argument: argument.fold_recursive(db, recursive),
            },
        }
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        match self {
            Self::MismatchedBound {
                bound_typevar,
                argument,
            } => Self::MismatchedBound {
                bound_typevar,
                argument: argument.unfold_recursive(db, recursive),
            },
            Self::MismatchedConstraint {
                bound_typevar,
                argument,
            } => Self::MismatchedConstraint {
                bound_typevar,
                argument: argument.unfold_recursive(db, recursive),
            },
        }
    }
}

impl<'db> Foldable<'db> for TupleSpec<'db> {
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

impl<'db> Foldable<'db> for Cow<'db, TupleSpec<'db>> {
    fn fold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        Cow::Owned(self.into_owned().fold_recursive(db, recursive))
    }

    fn unfold_recursive(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> Self {
        Cow::Owned(self.into_owned().unfold_recursive(db, recursive))
    }
}
