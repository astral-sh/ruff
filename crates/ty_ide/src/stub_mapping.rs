use itertools::Either;
use ty_python_semantic::{ResolvedDefinition, map_stub_definition};

/// Maps `ResolvedDefinitions` from stub files to corresponding definitions in source files.
///
/// This mapper is used to implement "Go To Definition" functionality that navigates from
/// stub file declarations to their actual implementations in source files. It also allows
/// other language server providers (like hover, completion, and signature help) to find
/// docstrings for functions that resolve to stubs.
pub(crate) struct StubMapper<'db> {
    db: &'db dyn crate::Db,
}

impl<'db> StubMapper<'db> {
    pub(crate) fn new(db: &'db dyn crate::Db) -> Self {
        Self { db }
    }

    /// Map a `ResolvedDefinition` from a stub file to corresponding definitions in source files.
    ///
    /// If the definition is in a stub file and a corresponding source file definition exists,
    /// returns the source file definition(s). Otherwise, returns the original definition.
    pub(crate) fn map_definition(
        &self,
        def: ResolvedDefinition<'db>,
    ) -> impl Iterator<Item = ResolvedDefinition<'db>> {
        if let Some(definitions) = map_stub_definition(self.db, &def) {
            return Either::Left(definitions.into_iter());
        }
        Either::Right(std::iter::once(def))
    }

    /// Map multiple `ResolvedDefinitions`, applying stub-to-source mapping to each.
    ///
    /// This is a convenience method that applies `map_definition` to each element
    /// in the input vector and flattens the results.
    pub(crate) fn map_definitions(
        &self,
        defs: Vec<ResolvedDefinition<'db>>,
    ) -> Vec<ResolvedDefinition<'db>> {
        defs.into_iter()
            .flat_map(|def| self.map_definition(def))
            .collect()
    }
}
