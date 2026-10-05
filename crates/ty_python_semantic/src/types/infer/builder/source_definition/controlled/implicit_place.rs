//! Implicit names use the same source ordering as ordinary expression inference.

use ruff_python_ast::PythonVersion;
use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::KnownModule;
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::place::implicit_symbol::{
    ClassBodySymbolEffects, class_body_implicit_symbol_with,
    module_type_implicit_global_symbol_with,
};
use crate::place::source_effects::{PublicLookupEffects, SourcePlaceEffects};
use crate::place::{
    ConsideredDefinitions, PlaceAndQualifiers, RequiresExplicitReExport, symbol_with,
};
use crate::place_load::ImplicitPlaceLoad;
use crate::types::infer::DefinitionTypes;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::implicit_place::{
    ImplicitPlaceEffects, ImplicitPlaceFacts, implicit_place_with,
};
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::{KnownClass, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn infer_implicit_place(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        implicit: ImplicitPlaceLoad<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let file = self.local(1, 0, || builder.program_file()).await?;
        self.check_file_program(file).await?;
        self.work(1).await?;
        implicit_place_with(
            builder.scope(),
            implicit,
            &SourceImplicitPlaceEffects {
                source: self,
                env: builder.program_environment(),
            },
            ImplicitPlaceFacts,
        )
        .await
    }
}

struct SourceImplicitPlaceEffects<'env, 'access, 'run, 'db: 'run, A> {
    source: &'env SourceEffects<'access, 'run, 'db, A>,
    env: &'env ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImplicitPlaceEffects<'db>
    for SourceImplicitPlaceEffects<'_, '_, 'run, 'db, A>
{
    type Error = salsa::execution_probe::RunError;

    async fn original_class(&self, definition: Definition<'db>) -> RunResult<Option<Type<'db>>> {
        let file = self.source.definition_file(definition).await?;
        self.source.check_file_program(file).await?;
        let inference = self.source.access.definition(definition).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.bindings.len(),
            _ => 1,
        };
        let work = SourceEffects::<A>::checked(entries.checked_add(3))?;
        self.source
            .local(work, 0, || {
                inference.original_class_type(definition).map(Type::from)
            })
            .await
    }

    async fn class_body_symbol(&self, name: &str) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source
            .allocate_future(|| class_body_implicit_symbol_with(self.env, name, self.source))
            .await?
            .await
    }

    async fn explicit_global(
        &self,
        file: ProgramFile<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let db = self.source.db();
        let scope = SourcePlaceEffects::global_scope(self.source, db, file).await?;
        symbol_with(
            db,
            self.source,
            scope,
            name,
            RequiresExplicitReExport::No,
            ConsideredDefinitions::AllReachable,
        )
        .await
    }

    async fn module_global(
        &self,
        file: ProgramFile<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        module_type_implicit_global_symbol_with(self.source.db(), file, name, self.source).await
    }

    async fn standard_builtins_scope(&self) -> RunResult<Option<ScopeId<'db>>> {
        let db = self.source.db();
        let Some(file) = SourcePlaceEffects::resolve_known_module(
            self.source,
            db,
            self.env,
            KnownModule::Builtins,
        )
        .await?
        else {
            return Ok(None);
        };
        SourcePlaceEffects::global_scope(self.source, db, file)
            .await
            .map(Some)
    }

    async fn builtin(&self, name: &str) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source.implicit_builtins_symbol(self.env, name).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassBodySymbolEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        let work = Self::checked(
            name.len()
                .checked_add(1)
                .and_then(|bytes| bytes.checked_mul(4))
                .and_then(|work| work.checked_add(8)),
        )?;
        self.work(work).await
    }

    async fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        TupleSpecEffects::known_instance(self, env, class).await
    }

    async fn python_version_at_least(
        &self,
        env: &ProgramEnvironment<'db>,
        minimum: PythonVersion,
    ) -> RunResult<bool> {
        let version = TupleSpecEffects::python_version(self, env).await?;
        self.local(1, 0, || version >= minimum).await
    }

    async fn union_two(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        PublicLookupEffects::union_two(self, self.db(), env, first, second).await
    }
}
