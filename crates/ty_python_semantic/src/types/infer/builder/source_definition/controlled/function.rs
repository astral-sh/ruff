//! Function identities use the original constructors and completed preceding bindings.

use std::ops::ControlFlow;

use ruff_python_ast as ast;
use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use ty_module_resolver::KnownModule;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::{FileScopeId, ProgramFile};

use super::storage::{dense_finish, sequence_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::{Place, RequiresExplicitReExport, place_from_bindings_with};
use crate::types::Type;
use crate::types::call::function_bindings::FunctionBindingEffects;
use crate::types::callable::function_descriptor::FunctionBindingEffects as DescriptorBindingEffects;
use crate::types::function::overloads::{OverloadCollectionEffects, collect_overloads_with};
use crate::types::function::{
    FunctionDecoratorKind, FunctionDecorators, FunctionIdentityEffects, FunctionLiteral,
    FunctionMetadataEffects, FunctionType, KnownFunction, KnownFunctionEffects, OverloadLiteral,
    identity_sealed,
};
use crate::types::infer::builder::function::application::{
    DecoratorApplicationFacts, DecoratorApplicationOperation, apply_function_decorator_with,
};
use crate::types::infer::builder::function::source_effects::{
    FunctionDecoratorRequest, FunctionDefinitionEffects, FunctionDefinitionWork, OverloadIdentity,
    sealed,
};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::function_decorator_ingestion as observations;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::overload_collection as overload_observations;
use crate::types::infer::{FunctionDecoratorInference, TypeInferenceBuilder};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(in crate::types::infer) fn function_decorator_test_allow_discard(&mut self) {
        self.context.defuse();
    }

    pub(in crate::types::infer) fn function_decorator_test_state(
        &self,
    ) -> observations::BuilderState<'db> {
        let (diagnostics, diagnostics_capacity, used, used_capacity) =
            self.context.retained_diagnostics().storage();
        observations::BuilderState {
            definition: match self.region {
                crate::types::infer::InferenceRegion::Definition(definition) => Some(definition),
                _ => None,
            },
            flags: self.context.inference_flags,
            expressions: (self.expressions.len(), self.expressions.capacity()),
            bindings: (self.bindings.0.len(), self.bindings.0.capacity()),
            called: (
                self.called_functions.len(),
                self.called_functions.capacity(),
            ),
            aliases: (
                self.implicit_aliases.len(),
                self.implicit_aliases.capacity(),
            ),
            diagnostics: (diagnostics, diagnostics_capacity),
            used_suppressions: (used, used_capacity),
        }
    }

    pub(in crate::types::infer) fn function_decorator_test_contents(
        &self,
    ) -> observations::BuilderContents<'db> {
        let mut diagnostics = crate::types::TypeCheckDiagnostics::default();
        diagnostics.extend(&self.context.retained_diagnostics());
        observations::BuilderContents {
            expressions: self.expressions.clone(),
            bindings: self.bindings.0.clone(),
            called: self.called_functions.iter().copied().collect(),
            aliases: self.implicit_aliases.iter().copied().collect(),
            diagnostics,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionDefinitionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: FunctionDefinitionWork<'_, 'db>) -> RunResult<()> {
        match work {
            FunctionDefinitionWork::Definition => self.work(2).await,
            FunctionDefinitionWork::ClassifyDecorator => self.work(16).await,
            FunctionDefinitionWork::DecoratorStorage(count) => {
                self.work(Self::checked(count.checked_add(1))?).await
            }
            FunctionDefinitionWork::Parameters(count) => {
                self.work(Self::checked(
                    count.checked_mul(2).and_then(|n| n.checked_add(1)),
                )?)
                .await
            }
            FunctionDefinitionWork::DecoratorsClassified {
                definition, decorators, inference_flags, has_transforming_decorators, candidates,
            } => {
                self.work(Self::checked(candidates.len().checked_add(4))?).await?;
                #[cfg(test)]
                observations::candidates(self.db(), definition, decorators, inference_flags, candidates, has_transforming_decorators);
                #[cfg(not(test))]
                let _ = (definition, decorators, inference_flags, has_transforming_decorators);
                Ok(())
            }
            FunctionDefinitionWork::ApplyDecorator => {
                self.work(1).await
            }
            FunctionDefinitionWork::OverloadStatement { definition, statement } => {
                self.local(4, 0, || {
                    #[cfg(test)]
                    observations::overload_statement(self.db(), definition, statement);
                    #[cfg(not(test))]
                    let _ = (definition, statement);
                }).await
            }
        }
    }

    async fn merge_decorator_results(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &FunctionDecoratorInference<'db>,
    ) -> RunResult<()> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::function_decorators::ingestion_results(
            inference,
        );
        self.merge_decorator_inference(builder, inference).await
    }

    async fn decorator_candidates<'ast>(
        &self,
        capacity: usize,
    ) -> RunResult<Vec<(Type<'db>, &'ast ast::Decorator)>> {
        self.work(1).await?;
        let quote = sequence_merge::<(Type<'db>, &'ast ast::Decorator)>(0, 0, capacity).ok_or(
            RunError::Contract("decorator candidate storage quotation overflow"),
        )?;
        self.local(quote.work, quote.bytes, || Vec::with_capacity(capacity))
            .await
    }

    async fn decorator_type(
        &self,
        inference: Option<&FunctionDecoratorInference<'db>>,
        decorator: &ast::Decorator,
    ) -> RunResult<Type<'db>> {
        self.work(1).await?;
        let entries = inference.map_or(0, |inference| inference.expression_types().len());
        let lookup = if entries == 0 {
            0
        } else {
            entries.ilog2() as usize + 1
        };
        self.local(
            Self::checked(lookup.checked_mul(4).and_then(|n| n.checked_add(8)))?,
            0,
            || {
                inference
                    .and_then(|inference| inference.expression_type(&decorator.expression))
                    .unwrap_or_else(Type::unknown)
            },
        )
        .await
    }

    async fn classify_decorator(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
    ) -> RunResult<FunctionDecorators> {
        self.work(4).await?;
        let kind = match ty {
            Type::FunctionLiteral(function) => FunctionDecoratorKind::from_known_function(
                FunctionBindingEffects::known(self, builder.db(), function).await?,
            ),
            Type::ClassLiteral(_) => FunctionDecoratorKind::from_known_class(
                self.known_call_class(builder.db(), ty).await?,
            ),
            _ => FunctionDecoratorKind::Unknown,
        };
        self.local(2, 0, || {
            #[cfg(test)]
            observations::classified(self.db(), builder, ty, kind);
            kind.flags()
        })
        .await
    }

    async fn check_final_decorators(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _function: &ast::StmtFunctionDef,
        final_decorator: Option<&ast::Decorator>,
        decorators: FunctionDecorators,
    ) -> RunResult<()> {
        self.work(2).await?;
        if final_decorator.is_some()
            || decorators.contains(FunctionDecorators::ABSTRACT_METHOD | FunctionDecorators::FINAL)
        {
            self.unavailable(SourceOperation::FunctionMetadata).await
        } else {
            Ok(())
        }
    }

    async fn report_useless_overload_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        _function: &ast::StmtFunctionDef,
        statement: &ast::Stmt,
    ) -> RunResult<ControlFlow<()>> {
        self.local(1, 0, || {
            #[cfg(test)]
            observations::overload_body_refused(self.db(), builder, statement);
            #[cfg(not(test))]
            let _ = (builder, statement);
        })
        .await?;
        self.unavailable(SourceOperation::FunctionMetadata).await
    }

    async fn overload_literal(
        &self,
        _db: &'db dyn Db,
        identity: OverloadIdentity<'_, 'db>,
    ) -> RunResult<OverloadLiteral<'db>> {
        self.access.overload_literal(identity).await
    }

    async fn function_type(
        &self,
        _db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> RunResult<FunctionType<'db>> {
        self.access.function_type(literal).await
    }

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> RunResult<bool> {
        self.work(1).await?;
        literal.has_separate_implementation_with(db, self).await
    }

    async fn is_overload(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> RunResult<bool> {
        self.work(1).await?;
        overload.is_overload_with(db, self).await
    }

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.record_source_deferred(builder, definition).await
    }

    async fn bind_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.bind_source_declaration(builder, function.into(), definition, ty)
            .await
    }

    async fn known_decorators(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<&'db FunctionDecoratorInference<'db>> {
        self.access.function_known_decorators(definition).await
    }
    async fn known_function(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> RunResult<Option<KnownFunction>> {
        KnownFunction::try_from_definition_and_name_with(db, definition, name, self).await
    }
    async fn function_body_scope(
        &self,
        _db: &'db dyn Db,
        program_file: ProgramFile<'db>,
        file_scope: FileScopeId,
    ) -> RunResult<ScopeId<'db>> {
        self.check_file_program(program_file).await?;
        let index = self.access.semantic_index(program_file).await?;
        self.local(1, 0, || index.scope_id(file_scope)).await
    }
    async fn function_literal(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> RunResult<FunctionLiteral<'db>> {
        let name = self.field(overload.field_requests(db).name()).await?;
        let name_work = self.local(1, 0, || name.len()).await?;
        self.work(Self::checked(name_work.checked_add(16))?).await?;
        FunctionLiteral::new_with(db, overload, self).await
    }
    async fn underlying_function(&self, _db: &'db dyn Db, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.descriptor_update_future(|| {
            DescriptorBindingEffects::underlying(self, ty)
        })
        .await?
        .await
    }
    async fn check_type_parameter_shadowing(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        self.check_function_type_parameter_shadowing_source(builder, function)
            .await
    }
    async fn apply_function_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        request: FunctionDecoratorRequest<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| {
            apply_function_decorator_with(builder, request, DecoratorApplicationFacts, self)
        })
        .await?
        .await
    }
    async fn finish_decorated_overload(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _literal: FunctionLiteral<'db>,
        _function: FunctionType<'db>,
        _inferred_ty: Type<'db>,
        _is_overload_implementation: bool,
        _is_overload: bool,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::DecoratorApplication(
            DecoratorApplicationOperation::OverloadFinalization,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> identity_sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownFunctionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        self.work(Self::checked(KnownFunction::classification_work(name))?)
            .await
    }

    async fn known_module(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<Option<KnownModule>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.access.known_module(file).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionIdentityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn names_equal(
        &self,
        left: &ast::name::Name,
        right: &ast::name::Name,
    ) -> RunResult<bool> {
        let work = Self::checked(left.len().checked_add(1))?;
        self.local(work, 0, || left == right).await
    }

    async fn definition(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> RunResult<Definition<'db>> {
        let scope = self.field(function.field_requests(db).body_scope()).await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        let scope = self.local(1, 0, || index.scope(file_scope)).await?;
        let work = self.local(1, 0, || index.definition_lookup_work()).await?;
        self.local(Self::checked(work.checked_add(2))?, 0, || {
            let Some(function) = scope.node().as_function() else {
                return Err(RunError::Contract("function scope has no function node"));
            };
            Ok(index.expect_single_definition(function))
        })
        .await?
    }

    async fn preceding_bindings(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
        definition: Definition<'db>,
    ) -> RunResult<Place<'db>> {
        let definition_file = self.definition_file(definition).await?;
        self.check_file_program(definition_file).await?;
        let body_scope = self.field(function.field_requests(db).body_scope()).await?;
        let file = self.scope_file(body_scope).await?;
        let source = self.access.prepare_existing(file).await?;
        let scope = self.definition_scope(definition).await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        let body_file_scope = self.field(body_scope.read_fields(db).file_scope_id()).await?;
        let body_scope = self.local(1, 0, || source.index.scope(body_file_scope)).await?;
        let name = self
            .local(2, 0, || {
                let Some(function) = body_scope.node().as_function() else {
                    return Err(RunError::Contract("function scope has no function node"));
                };
                Ok(&function.node(&source.module).name)
            })
            .await??;
        let index = source.index;
        let work = self.local(1, 0, || index.scoped_use_lookup_work()).await?;
        let bindings = self
            .local(Self::checked(work.checked_add(2))?, 0, || {
                let use_def = source.index.use_def_map(file_scope);
                let use_id = index.scoped_use_id(name);
                use_def.bindings_at_use(use_id)
            })
            .await?;
        let work = Self::checked(
            bindings
                .traversal_len()
                .checked_mul(4)
                .and_then(|n| n.checked_add(4)),
        )?;
        self.work(work).await?;
        let env = ProgramEnvironment::from_file(definition_file);
        Ok(
            place_from_bindings_with(&env, self, bindings, RequiresExplicitReExport::No, None)
                .await?
                .place,
        )
    }

    async fn callable_definition(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<Option<FunctionLiteral<'db>>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let inference = self.access.definition(definition).await?;
        let function = self
            .local(1, 0, || inference.function_type(definition))
            .await?;
        match function {
            Some(function) => Ok(Some(
                self.field(function.field_requests(db).literal()).await?,
            )),
            None => Ok(None),
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionMetadataEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn overloads_and_implementation(
        &self,
        _db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)> {
        let (overloads, implementation) = self.access.function_overloads(last_definition).await?;
        Ok((overloads.as_ref(), *implementation))
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_overloads(
        &self,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)> {
        let scope = self
            .field(last_definition.field_requests(self.db()).body_scope())
            .await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        #[cfg(test)]
        let _lifetime = overload_observations::OwnerLifetime::new();
        #[cfg(test)]
        overload_observations::collection_started(self.db(), last_definition);
        collect_overloads_with(self.db(), last_definition, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OverloadCollectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn append_overload(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
        overload: OverloadLiteral<'db>,
    ) -> RunResult<()> {
        self.work(2).await?;
        let mut quote =
            sequence_merge::<OverloadLiteral<'db>>(overloads.len(), overloads.capacity(), 1)
                .ok_or(RunError::Contract(
                    "overload collection growth quotation overflow",
                ))?;
        // Each appended entry also funds its retirement if a later child request stops collection.
        quote.work = Self::checked(quote.work.checked_add(1))?;
        #[cfg(test)]
        overload_observations::before_append(self.db(), overloads.len(), overloads.capacity());
        self.local(quote.work, quote.bytes, || {
            overloads.push(overload);
            #[cfg(test)]
            overload_observations::appended(self.db(), overloads.len(), overloads.capacity());
        })
        .await
    }

    async fn reverse_overloads(&self, overloads: &mut Vec<OverloadLiteral<'db>>) -> RunResult<()> {
        self.work(1).await?;
        let work = Self::checked(overloads.len().checked_add(1))?;
        #[cfg(test)]
        overload_observations::reversing(self.db(), overloads.len(), overloads.capacity());
        self.local(work, 0, || {
            overloads.reverse();
            #[cfg(test)]
            overload_observations::reversed(self.db(), overloads.len(), overloads.capacity());
        })
        .await
    }

    async fn finish_overloads(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
    ) -> RunResult<Box<[OverloadLiteral<'db>]>> {
        self.work(2).await?;
        let quote = dense_finish::<OverloadLiteral<'db>>(overloads.len(), overloads.capacity())
            .ok_or(RunError::Contract(
                "overload collection finalization quotation overflow",
            ))?;
        #[cfg(test)]
        overload_observations::finalizing(self.db(), overloads.len(), overloads.capacity());
        // On refusal or cancellation, the execution driver drops child tasks before their parent
        // futures. Keeping the vector in the collector's future until admission succeeds makes
        // it outlive those child tasks.
        self.local(quote.work, quote.bytes, || {
            let overloads = std::mem::take(overloads).into_boxed_slice();
            #[cfg(test)]
            overload_observations::finalized(self.db());
            overloads
        })
        .await
    }
}
