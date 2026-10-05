//! Builtin namespace selection shared by ordinary and suspended source inference.

use std::convert::Infallible;

use ty_module_resolver::{KnownModule, ModuleName, resolve_module_confident};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, global_scope};

use super::{
    ConsideredDefinitions, LookupError, LookupResult, Place, PlaceAndQualifiers,
    RequiresExplicitReExport, module_type_implicit_global_symbol, symbol_impl,
};
use crate::types::may_exist_at_runtime;
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinVisibility {
    All,
    RuntimeOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinNamespace {
    Project,
    Standard,
}

#[derive(Clone, Copy)]
pub(crate) struct BuiltinLookupFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousBuiltinLookupEffects)]
    pub(crate) trait BuiltinLookupEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, name: &str) -> Result<(), Self::Error>;

        #[operation(child)]
        async fn namespace_symbol(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            name: &str,
            visibility: BuiltinVisibility,
            namespace: BuiltinNamespace,
        ) -> Result<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>, Self::Error>;

        #[operation(source)]
        async fn resolve_namespace(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            namespace: BuiltinNamespace,
        ) -> Result<Option<ProgramFile<'db>>, Self::Error>;

        #[operation(source)]
        async fn global_scope(
            &self,
            db: &'db dyn Db,
            file: ProgramFile<'db>,
        ) -> Result<ScopeId<'db>, Self::Error>;

        #[operation(child)]
        async fn symbol(
            &self,
            db: &'db dyn Db,
            scope: ScopeId<'db>,
            name: &str,
            reexport: RequiresExplicitReExport,
            considered: ConsideredDefinitions,
        ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

        #[operation(child)]
        async fn lookup_result(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            symbol: PlaceAndQualifiers<'db>,
        ) -> Result<LookupResult<'db>, Self::Error>;

        #[operation(child)]
        async fn implicit_global_symbol(
            &self,
            db: &'db dyn Db,
            file: ProgramFile<'db>,
            name: &str,
        ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

        #[operation(child)]
        async fn combine_fallback(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            prior: LookupError<'db>,
            fallback: PlaceAndQualifiers<'db>,
        ) -> Result<LookupResult<'db>, Self::Error>;

        #[operation(source)]
        async fn runtime_visibility(
            &self,
            definition: Definition<'db>,
        ) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl BuiltinLookupFacts {
        fn place<'db>(&self, result: LookupResult<'db>) -> PlaceAndQualifiers<'db> {
            result.into()
        }

        fn exists(&self, symbol: PlaceAndQualifiers<'_>) -> bool {
            symbol.ignore_possibly_undefined().is_some()
        }

        fn definition<'db>(&self, symbol: PlaceAndQualifiers<'db>) -> Option<Definition<'db>> {
            match symbol.place {
                Place::Defined(defined) => defined.provenance.definition(),
                Place::Undefined => None,
            }
        }
    }

    #[synchronous(builtins_symbol_sync)]
    #[capabilities(effects = BuiltinLookupEffects)]
    #[passive_values(BuiltinNamespace::Project, BuiltinNamespace::Standard)]
    pub(crate) async fn builtins_symbol_with<'db, E: BuiltinLookupEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        visibility: BuiltinVisibility,
        effects: &E,
    ) -> Result<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>, E::Error> {
        // Project-level builtins can override any standard builtin. A hidden typing-only
        // definition does not prevent the standard namespace from supplying the name.
        if let Some(found) = effects
            .namespace_symbol(db, env, name, visibility, BuiltinNamespace::Project)
            .await?
        {
            return Ok(Some(found));
        }
        effects
            .namespace_symbol(db, env, name, visibility, BuiltinNamespace::Standard)
            .await
    }

    #[synchronous(builtin_namespace_symbol_sync)]
    #[capabilities(effects = BuiltinLookupEffects, facts = BuiltinLookupFacts)]
    #[passive_values(RequiresExplicitReExport::Yes, ConsideredDefinitions::EndOfScope)]
    pub(crate) async fn builtin_namespace_symbol_with<'db, E: BuiltinLookupEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        visibility: BuiltinVisibility,
        namespace: BuiltinNamespace,
        facts: BuiltinLookupFacts,
        effects: &E,
    ) -> Result<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>, E::Error> {
        effects.checkpoint(name).await?;
        let Some(file) = effects.resolve_namespace(db, env, namespace).await? else {
            return Ok(None);
        };
        let scope = effects.global_scope(db, file).await?;
        let prior = effects
            .symbol(
                db,
                scope,
                name,
                RequiresExplicitReExport::Yes,
                ConsideredDefinitions::EndOfScope,
            )
            .await?;
        let result = match effects.lookup_result(db, env, prior).await? {
            Ok(found) => Ok(found),
            Err(prior) => {
                // This is a lookup in the builtins namespace, so use the globals supplied
                // by `types.ModuleType`, rather than its additional imported attributes.
                let fallback = effects.implicit_global_symbol(db, file, name).await?;
                effects.combine_fallback(db, env, prior, fallback).await?
            }
        };
        let found = facts.place(result);
        if !facts.exists(found) {
            return Ok(None);
        }
        if let BuiltinVisibility::RuntimeOnly = visibility
            && let Some(definition) = facts.definition(found)
            && !effects.runtime_visibility(definition).await?
        {
            return Ok(None);
        }
        Ok(Some((scope, found)))
    }
}

pub(crate) struct InlineBuiltinLookupEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineBuiltinLookupEffects<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousBuiltinLookupEffects<'db> for InlineBuiltinLookupEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &str) -> Result<(), Infallible> {
        Ok(())
    }

    fn namespace_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        visibility: BuiltinVisibility,
        namespace: BuiltinNamespace,
    ) -> Result<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>, Infallible> {
        builtin_namespace_symbol_sync(
            db,
            env,
            name,
            visibility,
            namespace,
            BuiltinLookupFacts,
            self,
        )
    }

    fn resolve_namespace(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        namespace: BuiltinNamespace,
    ) -> Result<Option<ProgramFile<'db>>, Infallible> {
        let name = match namespace {
            BuiltinNamespace::Project => ModuleName::new_static("__builtins__").unwrap(),
            BuiltinNamespace::Standard => KnownModule::Builtins.name(),
        };
        Ok(
            resolve_module_confident(db, env.resolver_environment(db), &name)
                .and_then(|module| Some(ProgramFile::new(db, module.file(db)?, env.program(db)))),
        )
    }

    fn global_scope(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<ScopeId<'db>, Infallible> {
        Ok(global_scope(db, file))
    }

    fn symbol(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        name: &str,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(symbol_impl(db, scope, name, reexport, considered))
    }

    fn lookup_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        symbol: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(symbol.into_lookup_result(db, env))
    }

    fn implicit_global_symbol(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(module_type_implicit_global_symbol(db, file, name))
    }

    fn combine_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(prior.or_fall_back_to(db, env, fallback))
    }

    fn runtime_visibility(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(may_exist_at_runtime(self.db, definition))
    }
}
