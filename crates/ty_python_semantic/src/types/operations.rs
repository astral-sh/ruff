//! Entry points for semantic type operations and the state borrowed by their recursive helpers.

use std::borrow::Cow;
use std::cell::Cell;
use std::ops::Deref;
use std::rc::Rc;

use salsa::plumbing::AsId;
#[cfg(any(test, feature = "testing"))]
use ty_python_core::Program;
use ty_python_core::ProgramFile;

use super::cyclic::GuardToken;
use super::{ApplyTypeMappingVisitor, TypeRecursionState};
use crate::{Db, ProgramEnvironment};

/// Active guards shared by every semantic operation descended from the same root.
/// Transformation caches remain local to each mapping.
#[derive(Clone)]
pub(crate) struct TypeRecursionContext<'db>(Rc<TypeRecursionState<'db>>);

impl<'db> TypeRecursionContext<'db> {
    pub(super) fn state(&self) -> &TypeRecursionState<'db> {
        &self.0
    }
}

/// Access to semantic operations within one active computation.
///
/// Helpers borrow this object instead of choosing whether to preserve a caller's recursion
/// state. Structural transformations use its mapping visitor; semantic comparisons and body
/// inspection retain the same context.
///
/// Semantic comparisons borrow the current operation:
///
/// ```
/// use ty_python_semantic::{Db, TypeOperations};
/// use ty_python_semantic::types::Type;
///
/// fn compare<'db>(db: &'db dyn Db, env: &TypeOperations<'db>, left: Type<'db>, right: Type<'db>) -> bool {
///     left.is_assignable_to(db, env, right)
/// }
/// ```
///
/// Program metadata alone cannot perform a semantic comparison:
///
/// ```compile_fail
/// use ty_python_semantic::{Db, ProgramEnvironment};
/// use ty_python_semantic::types::Type;
///
/// fn compare<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>) -> bool {
///     left.is_assignable_to(db, env, right)
/// }
/// ```
#[derive(Clone)]
pub struct TypeOperations<'db> {
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    context: TypeRecursionContext<'db>,
}

impl<'db> Deref for TypeOperations<'db> {
    type Target = ProgramEnvironment<'db>;

    fn deref(&self) -> &Self::Target {
        &self.env
    }
}

impl<'db> TypeOperations<'db> {
    /// Start an independent source query, or a private cache entry selected by `cached_or_compute`.
    /// Helpers that inspect applied types borrow their caller's operations instead, so they retain
    /// its active recursion guards.
    pub(crate) fn new_root(db: &'db dyn Db, env: ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env,
            context: TypeRecursionContext(Rc::new(TypeRecursionState::default())),
        }
    }

    /// Use a source cache only when its result cannot depend on an active recursion guard.
    pub(crate) fn cached_or_compute<T: Clone>(
        &self,
        cached: impl FnOnce() -> &'db T,
        compute: impl FnOnce() -> T,
    ) -> Cow<'db, T> {
        if self.context.state().operation.is_active() {
            Cow::Owned(compute())
        } else {
            Cow::Borrowed(cached())
        }
    }

    /// A recursive narrowing checkpoint cannot establish a narrower type for its input.
    pub(crate) fn narrowing_checkpoint(
        &self,
        scope: ty_python_core::scope::ScopeId<'db>,
        place: ty_python_core::place::ScopedPlaceId,
        constraint: ty_python_core::narrowing_constraints::ScopedNarrowingConstraint,
        base: super::Type<'db>,
        compute: impl FnOnce() -> crate::reachability::ProjectedNarrowingCheckpoint<'db>,
    ) -> crate::reachability::ProjectedNarrowingCheckpoint<'db> {
        self.context.state().narrowing_checkpoints.visit(
            self,
            &(scope, place, constraint, base),
            || crate::reachability::ProjectedNarrowingCheckpoint::Unconstrained,
            compute,
        )
    }

    pub(crate) fn semantic_model(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> crate::SemanticModel<'db> {
        crate::SemanticModel::with_operations(
            db,
            file,
            Self::new_root(db, ProgramEnvironment::from_file(file)),
        )
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn for_test_program(db: &'db dyn Db, program: Program<'db>) -> Self {
        Self::new_root(db, ProgramEnvironment::from_program(program))
    }

    /// Keep global query caches unavailable while a local recursive operation is active.
    pub(super) fn enter_recursive_scope<'guard>(
        &self,
        guard: GuardToken<'guard>,
    ) -> RecursiveOperationScope<'guard, 'db> {
        let operation = &self.context.state().operation;
        let previous = operation.depth.replace(operation.depth.get() + 1);
        RecursiveOperationScope {
            context: self.context.clone(),
            previous,
            _guard: guard,
        }
    }

    pub(super) fn identity(&self) -> TypeOperationIdentity {
        TypeOperationIdentity {
            owner: self.context.state().operation.identity.clone(),
            program: self.program(self.db).as_id(),
        }
    }

    /// Read source metadata from another environment without replacing the active operation.
    pub(crate) fn in_environment(&self, env: ProgramEnvironment<'db>) -> Self {
        Self {
            db: self.db,
            env,
            context: self.context.clone(),
        }
    }

    /// Start a mapping whose caches belong to this transformation and whose guards remain shared.
    pub(super) fn mapping(&self) -> ApplyTypeMappingVisitor<'_, 'db> {
        ApplyTypeMappingVisitor::new(self)
    }

    pub(super) fn context(&self) -> &TypeRecursionContext<'db> {
        &self.context
    }
}

#[derive(Default)]
pub(super) struct OperationState {
    identity: Rc<()>,
    depth: Cell<usize>,
}

impl OperationState {
    fn is_active(&self) -> bool {
        self.depth.get() != 0
    }
}

/// A local cache belongs to one operation and one program environment.
#[derive(Clone, Debug)]
pub(super) struct TypeOperationIdentity {
    owner: Rc<()>,
    program: salsa::Id,
}

impl PartialEq for TypeOperationIdentity {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner) && self.program == other.program
    }
}

impl Eq for TypeOperationIdentity {}

/// Restores the enclosing guard scope even when recursive analysis unwinds.
#[must_use]
pub(super) struct RecursiveOperationScope<'guard, 'db> {
    context: TypeRecursionContext<'db>,
    previous: usize,
    _guard: GuardToken<'guard>,
}

impl Drop for RecursiveOperationScope<'_, '_> {
    fn drop(&mut self) {
        self.context.state().operation.depth.set(self.previous);
    }
}

#[cfg(test)]
mod tests {
    use crate::ProgramEnvironment;
    use crate::db::tests::setup_db;
    use crate::types::Type;

    #[test]
    fn nested_operations_share_guards_without_caching_completed_results() {
        let db = setup_db();
        let env = db.program_environment();
        let nested = env.in_environment(ProgramEnvironment::from_program(env.program(&db)));
        let left = Type::int_literal(1);
        let right = Type::int_literal(2);

        assert!(!env.context().check_redundancy(&db, &env, left, right, || {
            assert!(
                nested
                    .context()
                    .check_redundancy(&db, &nested, left, right, || false)
            );
            false
        }));
        assert!(
            nested
                .context()
                .check_redundancy(&db, &nested, left, right, || true)
        );
    }
}
