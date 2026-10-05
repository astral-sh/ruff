//! Dependencies of selecting declarations supplied by `types.ModuleType`.

use std::future::{Future, ready};

use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use salsa::execution_probe::FieldRequest;
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    PlaceTable, ProgramFile, SemanticIndex, UseDefMap, place_table, semantic_index, use_def_map,
};

use super::source_effects::{LegacyInlineEffects, SourcePlaceEffects};
use crate::types::KnownClass;
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug)]
pub(crate) enum ImplicitGlobalWork {
    InspectModule,
    SymbolLookup { symbols: usize, name_bytes: usize },
    ClassBindings { entries: usize },
    Declarations { entries: usize },
}

pub(crate) trait ImplicitGlobalEffects<'db>: SourcePlaceEffects<'db> {
    async fn checkpoint(&self, work: ImplicitGlobalWork) -> Result<(), Self::Error>;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn is_stub(&self, db: &'db dyn Db, file: File) -> Result<bool, Self::Error>;

    async fn place_table(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<&'db PlaceTable, Self::Error>;

    async fn use_def_map(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<&'db UseDefMap<'db>, Self::Error>;

    async fn parsed_module(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<ParsedModuleRef, Self::Error>;

    async fn semantic_index(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<&'db SemanticIndex<'db>, Self::Error>;

    /// Returns the body scope of `types.ModuleType`.
    ///
    /// `LegacyInlineEffects` and `SourceEffects` use the cached scope query.
    /// `QueuedDefinitionEffects` evaluates selection in the calling definition's transaction,
    /// which owns any definition dependencies discovered during selection.
    async fn module_type_body_scope(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ScopeId<'db>>, Self::Error>;

    /// Resolves the class when its vendored direct declaration cannot establish the body scope.
    async fn fallback_module_type_body_scope(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ScopeId<'db>>, Self::Error>;
}

impl<'db> ImplicitGlobalEffects<'db> for LegacyInlineEffects<'db> {
    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    fn is_stub(
        &self,
        db: &'db dyn Db,
        file: File,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(file.is_stub(db)))
    }

    fn place_table(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> impl Future<Output = Result<&'db PlaceTable, Self::Error>> {
        ready(Ok(place_table(db, scope)))
    }

    fn use_def_map(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> impl Future<Output = Result<&'db UseDefMap<'db>, Self::Error>> {
        ready(Ok(use_def_map(db, scope)))
    }

    fn parsed_module(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> impl Future<Output = Result<ParsedModuleRef, Self::Error>> {
        ready(Ok(parsed_module(db, file.python_file(db)).load(db)))
    }

    fn semantic_index(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> impl Future<Output = Result<&'db SemanticIndex<'db>, Self::Error>> {
        ready(Ok(semantic_index(db, file)))
    }

    fn checkpoint(
        &self,
        _work: ImplicitGlobalWork,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn module_type_body_scope(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<Option<ScopeId<'db>>, Self::Error>> {
        ready(Ok(super::implicit_globals::module_type_body_scope(db, env)))
    }

    fn fallback_module_type_body_scope(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<Option<ScopeId<'db>>, Self::Error>> {
        ready(Ok(KnownClass::ModuleType
            .try_to_class_literal(db, env)
            .map(|class| class.body_scope(db))))
    }
}
