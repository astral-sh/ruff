//! Builtin lookups use prepared modules and canonical runtime-visibility results.

use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::{KnownModule, ModuleName};
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::{SourceAccess, SourceEffects};
use crate::place::builtin_lookup::{
    BuiltinLookupEffects, BuiltinLookupFacts, BuiltinNamespace, BuiltinVisibility,
    builtin_namespace_symbol_with, builtins_symbol_with,
};
use crate::place::implicit_symbol::module_type_implicit_global_symbol_with;
use crate::place::source_effects::SourcePlaceEffects;
use crate::place::{
    ConsideredDefinitions, LookupError, LookupResult, PlaceAndQualifiers, RequiresExplicitReExport,
    symbol_with,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn implicit_builtins_symbol(
        &self,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        Ok(
            builtins_symbol_with(self.db(), &env, name, BuiltinVisibility::RuntimeOnly, self)
                .await?
                .map(|(_, symbol)| symbol)
                .unwrap_or_default(),
        )
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BuiltinLookupEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        // Cover fixed branch/provenance checks and the name comparisons in `symbol_with`.
        self.work(Self::checked(name.len().checked_add(16))?).await
    }

    async fn namespace_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        visibility: BuiltinVisibility,
        namespace: BuiltinNamespace,
    ) -> RunResult<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        builtin_namespace_symbol_with(
            db,
            &env,
            name,
            visibility,
            namespace,
            BuiltinLookupFacts,
            self,
        )
        .await
    }

    async fn resolve_namespace(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        namespace: BuiltinNamespace,
    ) -> RunResult<Option<ProgramFile<'db>>> {
        let program = self.environment_program(env).await?;
        if namespace == BuiltinNamespace::Standard {
            let env = ProgramEnvironment::from_program(program);
            return SourcePlaceEffects::resolve_known_module(self, db, &env, KnownModule::Builtins)
                .await;
        }
        let name = self
            .local("__builtins__".len() + 1, 0, || {
                ModuleName::new_static("__builtins__")
            })
            .await?
            .ok_or(RunError::Contract("invalid project builtin module name"))?;
        let module = self.access.resolve_module(program, &name, None).await?;
        let file = match module {
            Some(module) => module.file_with(self.access.endpoint()).await?,
            None => None,
        };
        let Some(file) = file else {
            return Ok(None);
        };
        Ok(Some(self.access.prepare_file(file, program).await?.file))
    }

    async fn global_scope(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> RunResult<ScopeId<'db>> {
        SourcePlaceEffects::global_scope(self, db, file).await
    }

    async fn symbol(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        name: &str,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        symbol_with(db, self, scope, name, reexport, considered).await
    }

    async fn lookup_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        symbol: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        symbol.into_lookup_result_with(db, &env, self).await
    }

    async fn implicit_global_symbol(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.check_file_program(file).await?;
        module_type_implicit_global_symbol_with(db, file, name, self).await
    }

    async fn combine_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        prior.or_fall_back_to_with(db, &env, self, fallback).await
    }

    async fn runtime_visibility(&self, definition: Definition<'db>) -> RunResult<bool> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.access.runtime_visibility(definition).await
    }
}
