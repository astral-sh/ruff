//! Projection solver terms.
//!
//! A term is the query-free result of applying one projection path to one
//! container arm. Equation solving consumes these terms; inference-time evidence
//! stores them for later replay.

use crate::Db;
use crate::types::visitor::any_over_type;
use crate::types::{DynamicType, KnownClass, ProgramEnvironment, Type, UnionType};

/// The result of applying one projection path to one container arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue, get_size2::GetSize)]
pub(super) enum ProjectionTerm<'db> {
    Exact(Type<'db>),
    Homogeneous(Type<'db>),
    List(Type<'db>),
}

impl<'db> ProjectionTerm<'db> {
    pub(super) fn ty(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            ProjectionTerm::Exact(ty) | ProjectionTerm::Homogeneous(ty) => ty,
            ProjectionTerm::List(element) => {
                KnownClass::List.to_specialized_instance(db, env, &[element])
            }
        }
    }

    /// Converts inference-time projection metadata before a term enters recovery.
    pub(super) fn materialize_projection_derivations(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        match self {
            Self::Exact(ty) => Self::Exact(ty.materialize_projection_derivations(db, env)),
            Self::Homogeneous(ty) => {
                Self::Homogeneous(ty.materialize_projection_derivations(db, env))
            }
            Self::List(element) => Self::List(element.materialize_projection_derivations(db, env)),
        }
    }

    pub(super) fn from_union_terms(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        terms: &[Self],
    ) -> Option<Self> {
        let wrap_in_list = terms
            .iter()
            .any(|term| matches!(term, ProjectionTerm::List(_)));
        if wrap_in_list
            && terms
                .iter()
                .any(|term| !matches!(term, ProjectionTerm::List(_)))
        {
            return None;
        }

        let elements = terms.iter().map(|term| match *term {
            ProjectionTerm::List(element) => element,
            ProjectionTerm::Exact(ty) | ProjectionTerm::Homogeneous(ty) => ty,
        });
        let ty = UnionType::from_elements_cycle_recovery(db, env, elements);
        Some(if wrap_in_list {
            ProjectionTerm::List(ty)
        } else {
            ProjectionTerm::Exact(ty)
        })
    }

    pub(super) fn is_ambiguous(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        any_over_type(db, env, self.ty(db, env), false, |ty| {
            matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
        })
    }
}
