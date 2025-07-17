use ty_python_semantic::ResolvedDefinition;

/// Maps `ResolvedDefinitions` from stub files to corresponding definitions in source files.
///
/// This mapper is used to implement "Go To Definition" functionality that navigates from
/// stub file declarations to their actual implementations in source files. It also allows
/// other language server providers (like hover, completion, and signature help) to find
/// docstrings for functions that resolve to stubs.
pub(crate) struct StubMapper<'db> {
    #[allow(dead_code)] // Will be used when implementation is added
    db: &'db dyn crate::Db,
}

impl<'db> StubMapper<'db> {
    #[allow(dead_code)] // Will be used in the future
    pub(crate) fn new(db: &'db dyn crate::Db) -> Self {
        Self { db }
    }

    /// Map a `ResolvedDefinition` from a stub file to corresponding definitions in source files.
    ///
    /// If the definition is in a stub file and a corresponding source file definition exists,
    /// returns the source file definition(s). Otherwise, returns the original definition.
    #[allow(dead_code)] // Will be used when implementation is added
    #[allow(clippy::unused_self)] // Will use self when implementation is added
    pub(crate) fn map_definition(
        &self,
        def: ResolvedDefinition<'db>,
    ) -> Vec<ResolvedDefinition<'db>> {
        // TODO: Implement stub-to-source mapping logic
        // For now, just return the original definition
        vec![def]
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
