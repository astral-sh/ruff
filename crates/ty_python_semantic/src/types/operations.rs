//! Shared state for semantic type operations.

use std::ops::Deref;
use std::rc::Rc;

#[cfg(any(test, feature = "testing"))]
use ty_python_core::Program;

use super::TypeRecursionContext;
use crate::{Db, ProgramEnvironment};

/// Access to semantic operations within one active computation.
///
/// Helpers borrow this object so nested operations retain their caller's recursion state.
/// Transformation caches remain local to each mapping.
#[derive(Clone)]
pub struct TypeOperations<'db> {
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    context: Rc<TypeRecursionContext<'db>>,
}

impl<'db> Deref for TypeOperations<'db> {
    type Target = ProgramEnvironment<'db>;

    fn deref(&self) -> &Self::Target {
        &self.env
    }
}

impl<'db> TypeOperations<'db> {
    /// Start an independent source query.
    pub(crate) fn new_root(db: &'db dyn Db, env: ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env,
            context: Rc::new(TypeRecursionContext::default()),
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn for_test_program(db: &'db dyn Db, program: Program<'db>) -> Self {
        Self::new_root(db, ProgramEnvironment::from_program(program))
    }

    /// Read source metadata from another environment while retaining the active guards.
    pub(crate) fn in_environment(&self, env: ProgramEnvironment<'db>) -> Self {
        Self {
            db: self.db,
            env,
            context: Rc::clone(&self.context),
        }
    }

    pub(super) fn context(&self) -> &TypeRecursionContext<'db> {
        &self.context
    }
}
