use std::borrow::Cow;

use crate::types::generics::Specialization;
use crate::types::{DynamicType, KnownClass, Type};
use crate::{Db, ProgramEnvironment};

/// The kind of type context constraint being provided.
#[derive(
    Default, Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue,
)]
enum TypeContextKind {
    /// A direct declared type annotation.
    #[default]
    Declared,

    /// The declared upper bound of a type variable.
    ///
    /// Note that unlike a declared type, the upper bound of a type variable acts only as a validity
    /// constraint, and does not provide a preferred specialization during generic call inference.
    Validity,
}

/// The type context for a given expression, e.g., the type annotation
/// in an annotated assignment.
///
/// Knowing the outer type context when inferring an expression can enable
/// more precise inference results, aka "bidirectional type inference".
#[derive(
    Default, Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue,
)]
pub(crate) struct TypeContext<'db> {
    declared_type: Option<Type<'db>>,
    kind: TypeContextKind,
}

impl<'db> TypeContext<'db> {
    fn new(declared_type: Option<Type<'db>>, kind: TypeContextKind) -> Self {
        // During generic call argument inference, type variables that cannot yet be specialized are
        // replaced by `UnspecializedTypeVar`. This marker intentionally carries neither type-variable
        // identity nor a concrete expected type, and so it should not be treated as a useful constraint.
        //
        // This deliberately matches only the bare marker: a partially specialized context such as
        // `list[UnspecializedTypeVar | int]` still carries useful structure and concrete type
        // information.
        let declared_type =
            declared_type.filter(|ty| *ty != Type::Dynamic(DynamicType::UnspecializedTypeVar));

        Self {
            declared_type,
            kind,
        }
    }

    pub(crate) fn declared(declared_type: Option<Type<'db>>) -> Self {
        Self::new(declared_type, TypeContextKind::Declared)
    }

    pub(crate) fn validity(declared_type: Type<'db>) -> Self {
        Self::new(Some(declared_type), TypeContextKind::Validity)
    }

    pub(in crate::types) fn declared_type(self) -> Option<Type<'db>> {
        self.declared_type
    }

    pub(crate) fn is_declared(self) -> bool {
        matches!(self.kind, TypeContextKind::Declared)
    }

    /// If the declared type is a specialized instance of the given `KnownClass`, returns the
    /// specialization.
    pub(super) fn known_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        known_class: KnownClass,
    ) -> Option<Specialization<'db>> {
        self.declared_type
            .and_then(|ty| ty.known_specialization(db, env, known_class))
    }

    pub(super) fn map(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        self.with_declared_type(self.declared_type.map(f))
    }

    pub(crate) fn with_declared_type(self, declared_type: Option<Type<'db>>) -> Self {
        Self::new(declared_type, self.kind)
    }

    pub(super) fn is_typealias(&self) -> bool {
        self.declared_type
            .is_some_and(|ty| ty.is_typealias_special_form())
    }

    /// If the declared type is a union, returns the target elements that it can be narrowed to.
    pub(super) fn narrow_targets(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Cow<'db, [Type<'db>]>> {
        let union = self.declared_type?.as_union_like(db)?;

        let targets = if union.has_aliases(db) {
            let expanded = union.expand_aliases(db, env);
            if let Some(union) = expanded.as_union_like(db) {
                Cow::Borrowed(union.elements(db))
            } else {
                Cow::Owned(vec![expanded])
            }
        } else {
            Cow::Borrowed(union.elements(db))
        };

        // TODO: We could theoretically attempt to narrow to every element of
        // the power set of this union. However, this leads to an exponential
        // explosion of inference attempts, and is rarely needed in practice.
        Some(targets)
    }
}

impl<'db> From<Type<'db>> for TypeContext<'db> {
    fn from(declared_type: Type<'db>) -> Self {
        Self::declared(Some(declared_type))
    }
}
