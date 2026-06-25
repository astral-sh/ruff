pub use ty_python_core::environment::{AnalysisFile, InferenceEnvironment, InferenceSettings};

/// Explicit environment for ordinary type-system helpers that are not Salsa queries.
#[derive(Clone, Copy)]
pub struct TypingContext<'db> {
    db: &'db dyn crate::Db,
    environment: InferenceEnvironment,
}

impl<'db> TypingContext<'db> {
    pub fn new(db: &'db dyn crate::Db, environment: InferenceEnvironment) -> Self {
        Self { db, environment }
    }

    pub fn db(self) -> &'db dyn crate::Db {
        self.db
    }

    pub fn environment(self) -> InferenceEnvironment {
        self.environment
    }
}
