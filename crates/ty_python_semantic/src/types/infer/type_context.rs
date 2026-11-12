use std::borrow::Cow;

use crate::place::TypeOrigin;
use crate::types::generics::Specialization;
use crate::types::{DynamicType, KnownClass, Type};
use crate::{Db, ProgramEnvironment};

/// The kind of type context constraint being provided.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum TypeContextKind {
    /// A concrete expected type, whether explicitly declared, or inferred through previous assignments.
    Expected {
        /// Whether the type context should provide a solver preference in the case that the
        /// declared and inferred solutions are mutually assignable.
        prefer: bool,
    },

    /// A constraint originating from inference evidence.
    ///
    /// During generic call inference, constraints inferred from call arguments are provided as
    /// type context for subsequent inference attempts during fixpoint iteration.
    Inferred,

    /// The declared upper bound of a type variable.
    ///
    /// Note that unlike a declared type, the upper bound of a type variable acts only as a validity
    /// constraint, and does not provide a preferred specialization. A validity constraint is also not
    /// used to solve otherwise unconstrained type variables.
    Validity,
}

impl TypeContextKind {
    pub(crate) fn preference() -> Self {
        Self::Expected { prefer: true }
    }
}

/// The type context for a given expression, e.g., the type annotation
/// in an annotated assignment.
///
/// Knowing the outer type context when inferring an expression can enable
/// more precise inference results, aka "bidirectional type inference".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct TypeContext<'db> {
    expected_type: Option<Type<'db>>,
    kind: TypeContextKind,
}

impl Default for TypeContext<'_> {
    fn default() -> Self {
        Self::preference(None)
    }
}

impl<'db> TypeContext<'db> {
    pub(crate) fn new(expected_type: Option<Type<'db>>, kind: TypeContextKind) -> Self {
        // During generic call argument inference, type variables that cannot yet be specialized are
        // replaced by `UnspecializedTypeVar`. This marker intentionally carries neither type-variable
        // identity nor a concrete expected type, and so it should not be treated as a useful constraint.
        //
        // This deliberately matches only the bare marker: a partially specialized context such as
        // `list[UnspecializedTypeVar | int]` still carries useful structure and concrete type
        // information.
        let expected_type =
            expected_type.filter(|ty| *ty != Type::Dynamic(DynamicType::UnspecializedTypeVar));

        Self {
            expected_type,
            kind,
        }
    }

    pub(crate) fn preference(expected_type: Option<Type<'db>>) -> Self {
        Self::new(expected_type, TypeContextKind::preference())
    }

    fn expected(expected_type: Option<Type<'db>>) -> Self {
        Self::new(expected_type, TypeContextKind::Expected { prefer: false })
    }

    /// Keep an expected type as a fallback when comparing overloads. Evidence inferred from
    /// other arguments retains its usual meaning across fixpoint iterations.
    pub(crate) fn without_preference(self) -> Self {
        if self.expected_type.is_some() && self.kind == TypeContextKind::preference() {
            Self::expected(self.expected_type)
        } else {
            self
        }
    }

    pub(super) fn from_origin(ty: Type<'db>, origin: TypeOrigin) -> Self {
        match origin {
            TypeOrigin::Declared => Self::preference(Some(ty)),
            // Later assignments are validated against the type of an inferred attribute, but it
            // does not provide a declared type preference, allowing the type of the attribute to
            // be overwritten by a gradual type.
            TypeOrigin::Inferred => Self::expected(Some(ty)),
        }
    }

    pub(crate) fn expected_type(self) -> Option<Type<'db>> {
        self.expected_type
    }

    pub(crate) fn kind(self) -> TypeContextKind {
        self.kind
    }

    pub(super) fn is_inferred(self) -> bool {
        matches!(self.kind, TypeContextKind::Inferred)
    }

    pub(crate) fn should_prefer_expected_type(self) -> bool {
        match self.kind {
            TypeContextKind::Expected { prefer } => prefer,
            // Preserve a solution inferred from other arguments in the previous fixpoint round.
            TypeContextKind::Inferred => true,
            TypeContextKind::Validity => false,
        }
    }

    /// If the expected type is a specialized instance of the given `KnownClass`, returns the
    /// specialization.
    pub(super) fn known_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        known_class: KnownClass,
    ) -> Option<Specialization<'db>> {
        self.expected_type
            .and_then(|ty| ty.known_specialization(db, env, known_class))
    }

    pub(super) fn map(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        self.with_expected_type(self.expected_type.map(f))
    }

    pub(crate) fn with_expected_type(self, expected_type: Option<Type<'db>>) -> Self {
        Self::new(expected_type, self.kind)
    }

    pub(super) fn is_typealias(&self) -> bool {
        self.expected_type
            .is_some_and(|ty| ty.is_typealias_special_form())
    }

    /// If the expected type is a union, returns the target elements that it can be narrowed to.
    pub(super) fn narrow_targets(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Cow<'db, [Type<'db>]>> {
        let union = self.expected_type?.as_union_like(db)?;

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
    fn from(expected_type: Type<'db>) -> Self {
        Self::preference(Some(expected_type))
    }
}
