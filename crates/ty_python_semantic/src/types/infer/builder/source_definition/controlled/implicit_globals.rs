//! Implicit module names selected from prepared source tables.

use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::PythonVersion;
use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use ty_python_core::scope::ScopeId;
use ty_python_core::{PlaceTable, ProgramFile, SemanticIndex, UseDefMap};

use super::{SourceAccess, SourceDefinitionEffect, SourceEffects, SourceOperation};
use crate::analysis::ImplicitNameOperation;
use crate::place::implicit_effects::{ImplicitGlobalEffects, ImplicitGlobalWork};
use crate::place::implicit_globals::is_implicit_module_global;
use crate::place::implicit_symbol::{ModuleGlobalSymbolEffects, SpecialModuleGlobal};
use crate::place::PlaceAndQualifiers;
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImplicitGlobalEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn is_stub(&self, _db: &'db dyn Db, file: File) -> RunResult<bool> {
        self.file_is_stub(file).await
    }

    async fn place_table(
        &self,
        _db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db PlaceTable> {
        self.access.place_table(scope).await
    }

    async fn use_def_map(
        &self,
        _db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db UseDefMap<'db>> {
        self.access.use_def_map(scope).await
    }

    async fn parsed_module(
        &self,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> RunResult<ParsedModuleRef> {
        self.access.parsed_module(file).await
    }

    async fn semantic_index(
        &self,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> RunResult<&'db SemanticIndex<'db>> {
        self.access.semantic_index(file).await
    }

    async fn checkpoint(&self, work: ImplicitGlobalWork) -> RunResult<()> {
        let units = match work {
            ImplicitGlobalWork::InspectModule => Some(8),
            // Include linear lookup in small tables and the declared-symbol check.
            ImplicitGlobalWork::SymbolLookup {
                symbols,
                name_bytes,
            } => name_bytes
                .checked_add(1)
                .and_then(|bytes| symbols.checked_add(1)?.checked_mul(bytes))
                .and_then(|units| units.checked_add(8)),
            // Cover each retained entry before filtering, including its AST-to-scope lookup.
            // Re-export and nonliteral reachability queries have their own checkpoints.
            ImplicitGlobalWork::ClassBindings { entries } => entries
                .checked_mul(16)
                .and_then(|units| units.checked_add(8)),
            ImplicitGlobalWork::Declarations { entries } => entries
                .checked_mul(4)
                .and_then(|units| units.checked_add(8)),
        };
        self.work(Self::checked(units)?).await
    }

    async fn module_type_body_scope(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<ScopeId<'db>>> {
        let program = self.environment_program(env).await?;
        self.access.module_type_body_scope(program).await
    }

    async fn fallback_module_type_body_scope(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<ScopeId<'db>>> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ImplicitModuleClass,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ModuleGlobalSymbolEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn symbol_checkpoint(
        &self,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> RunResult<()> {
        self.check_file_program(file).await?;
        let units = Self::checked(
            name.len()
                .checked_add(1)
                .and_then(|bytes| bytes.checked_mul(8))
                .and_then(|units| units.checked_add(8)),
        )?;
        self.work(units).await
    }

    async fn has_module_docstring(
        &self,
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ImplicitName(
            ImplicitNameOperation::ModuleGlobalDocstring,
        ))
        .await
    }

    async fn python_version_at_least(
        &self,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
        minimum: PythonVersion,
    ) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        let program = self.field(file.read_fields(fields).program()).await?;
        self.check_program(program)?;
        let environment = self
            .field(program.field_requests(fields).resolver_environment())
            .await?;
        let version = self
            .field(environment.read_fields(fields).python_version())
            .await?;
        self.local(1, 0, || version >= minimum).await
    }

    async fn special_type(
        &self,
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
        _special: SpecialModuleGlobal,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ImplicitName(
            ImplicitNameOperation::SpecialModuleGlobal,
        ))
        .await
    }

    async fn is_module_global(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> RunResult<bool> {
        let program = self
            .field(
                file.read_fields(self.access.endpoint().field_request_context())
                    .program(),
            )
            .await?;
        self.check_program(program)?;
        let env = ProgramEnvironment::from_program(program);
        let Some(scope) = ImplicitGlobalEffects::module_type_body_scope(self, db, &env).await?
        else {
            return Ok(false);
        };
        let table = self.access.place_table(scope).await?;
        let units = Self::checked(
            table
                .symbol_lookup_work(name.len())
                .and_then(|units| units.checked_add(8)),
        )?;
        // The prepared table establishes actual membership without allocating a symbol list.
        // Keep its owner outside the admitted callback until the source lookup completes.
        self.local(units, 0, || {
            table
                .symbol_id(name)
                .is_some_and(|symbol| is_implicit_module_global(table.symbol(symbol)))
        })
        .await
    }

    async fn module_global_member(
        &self,
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::ImplicitName(
            ImplicitNameOperation::ModuleGlobalMember,
        ))
        .await
    }
}
