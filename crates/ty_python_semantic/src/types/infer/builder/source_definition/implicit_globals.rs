//! Implicit module declarations resolved by the owning definition transaction.

use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use salsa::execution_probe::FieldRequest;
use ty_module_resolver::KnownModule;
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    PlaceTable, ProgramFile, SemanticIndex, UseDefMap, place_table, semantic_index, use_def_map,
};

use super::{QueuedDefinitionEffects, SourceDefinitionEffect, unsupported};
use crate::place::implicit_effects::{ImplicitGlobalEffects, ImplicitGlobalWork};
use crate::place::{Place, known_module_symbol_with, module_type_body_scope_with};
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::{ClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

impl<'db> ImplicitGlobalEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn is_stub(&self, db: &'db dyn Db, file: File) -> Result<bool, Self::Error> {
        self.work(1).await?;
        Ok(file.is_stub(db))
    }

    async fn place_table(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<&'db PlaceTable, Self::Error> {
        self.work(1).await?;
        Ok(place_table(db, scope))
    }

    async fn use_def_map(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<&'db UseDefMap<'db>, Self::Error> {
        self.work(1).await?;
        Ok(use_def_map(db, scope))
    }

    async fn parsed_module(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<ParsedModuleRef, Self::Error> {
        self.work(1).await?;
        Ok(parsed_module(db, file.python_file(db)).load(db))
    }

    async fn semantic_index(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<&'db SemanticIndex<'db>, Self::Error> {
        self.work(1).await?;
        Ok(semantic_index(db, file))
    }

    async fn checkpoint(&self, work: ImplicitGlobalWork) -> Result<(), Self::Error> {
        let units = match work {
            ImplicitGlobalWork::InspectModule => Some(1),
            // Small symbol tables use linear search. This also covers the hash/name work for
            // larger tables and the declared-symbol predicate after a successful lookup.
            ImplicitGlobalWork::SymbolLookup {
                symbols,
                name_bytes,
            } => name_bytes
                .checked_add(1)
                .and_then(|bytes| symbols.checked_add(1)?.checked_mul(bytes))
                .and_then(|units| units.checked_add(1)),
            // Reserve every retained entry before the filtered iterator advances. Re-export
            // and nonliteral reachability queries remain separate semantic dependencies.
            ImplicitGlobalWork::ClassBindings { entries } => entries
                .checked_mul(8)
                .and_then(|units| units.checked_add(1)),
            ImplicitGlobalWork::Declarations { entries } => entries
                .checked_mul(4)
                .and_then(|units| units.checked_add(1)),
        };
        self.work(units.ok_or(Boundary::CostOverflow)?).await
    }

    async fn module_type_body_scope(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ScopeId<'db>>, Self::Error> {
        module_type_body_scope_with(db, env, self).await
    }

    async fn fallback_module_type_body_scope(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ScopeId<'db>>, Self::Error> {
        let symbol =
            known_module_symbol_with(db, env, self, KnownModule::Types, "ModuleType").await?;
        self.work(1).await?;
        match symbol.place {
            // KnownClass::try_to_class_literal also recovers the class when it is possibly
            // undefined. The shared source lookup has already established that definedness.
            Place::Defined(defined) => match defined.ty {
                Type::ClassLiteral(ClassLiteral::Static(class)) => Ok(Some(class.body_scope(db))),
                _ => Err(unsupported(SourceDefinitionEffect::ImplicitModuleClass)),
            },
            Place::Undefined => Ok(None),
        }
    }
}
