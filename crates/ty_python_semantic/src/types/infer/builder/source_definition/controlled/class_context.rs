//! Generic-context selection reads class metadata before requesting declaration-owned queries.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
#[cfg(test)]
use salsa::plumbing::AsId;
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::{FixedFieldCopy, SourceAccess, SourceEffects};
use crate::types::class::context::inherited::{
    AsyncInheritedContextEffects, InheritedContextBaseCursor, InheritedContextWork,
    inherited_context_async_with,
};
#[cfg(test)]
use crate::types::class::context::pep695::observations as header_observations;
use crate::types::class::context::pep695::{ClassHeaderContextEffects, class_header_context_with};
use crate::types::class::context::{
    AsyncClassContextEffects, ClassContextBaseCursor, ClassContextSourceEffects, ClassContextWork,
    explicit_class_bases_with, generic_context_async_with, inherited_legacy_generic_context_with,
    legacy_generic_context_async_with, pep695_generic_context_with, sealed,
};
use crate::types::class::{
    ApplyClassSpecializationEffects, ClassDefaultSpecializationEffects,
    apply_class_specialization_with,
};
use crate::types::generics::defaults::default_specialization_with_effects;
use crate::types::legacy_typevars::find_legacy_typevars_with_effects;
use crate::types::local_transfer::generated_field_quote;
use crate::types::{
    BoundTypeVarInstance, ClassType, GenericContext, KnownClass, Specialization,
    StaticClassLiteral, Type,
};
use crate::{FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn apply_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
        initial_context: GenericContext<'db>,
        supplied: &[Option<Type<'db>>],
    ) -> RunResult<ClassType<'db>> {
        apply_class_specialization_with(class, (initial_context, supplied), self).await
    }

    pub(in crate::types::infer) async fn infer_class_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        generic_context_async_with(self.db(), class, self).await
    }

    /// Builds the canonical class context while the prepared module owns the borrowed header AST.
    pub(in crate::types::infer) async fn infer_pep695_class_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let scope_read = self
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |class: StaticClassLiteral<'db>, context| class.field_requests(context),
                    |class: StaticClassLiteral<'db>, context| {
                        class.field_requests(context).body_scope()
                    },
                ),
                || {
                    self.access.endpoint().read_field(
                        class
                            .field_requests(self.access.endpoint().field_request_context())
                            .body_scope(),
                        &FixedFieldCopy,
                    )
                },
            )
            .await?;
        let scope = scope_read.await;
        let file = self
            .type_parameter_future(|| self.scope_file(scope))
            .await?
            .await?;
        self.type_parameter_future(|| self.check_file_program(file))
            .await?
            .await?;
        let prepared = self
            .type_parameter_future(|| self.access.prepare_existing(file))
            .await?
            .await?;
        #[cfg(test)]
        let _source_lifetime = header_observations::SourceLifetime::new(class.as_id());
        let file_scope_read = self
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |scope: ScopeId<'db>, context| scope.read_fields(context),
                    |scope: ScopeId<'db>, context| scope.read_fields(context).file_scope_id(),
                ),
                || {
                    self.access.endpoint().read_field(
                        scope
                            .read_fields(self.access.endpoint().field_request_context())
                            .file_scope_id(),
                        &FixedFieldCopy,
                    )
                },
            )
            .await?;
        let file_scope = file_scope_read.await;
        let node = self
            .local_with_fixed_transfers(8, 0, || {
                if prepared.file != file {
                    return Err(RunError::Contract("prepared class context file is foreign"));
                }
                Ok(prepared
                    .index
                    .scope(file_scope)
                    .node()
                    .expect_class()
                    .node(&prepared.module))
            })
            .await??;
        let effects = self
            .local_with_fixed_transfers(3, 0, || ClassHeaderContextSource {
                source: self,
                index: prepared.index,
            })
            .await?;
        self.type_parameter_future(|| async {
            let header = class_header_context_with(node, &effects);
            #[cfg(test)]
            let header = header_observations::header(class.as_id(), header);
            header.await
        })
        .await?
        .await
    }

    pub(in crate::types::infer) async fn infer_inherited_class_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        inherited_context_async_with(self.db(), class, self).await
    }
}

impl<'args, 'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    ApplyClassSpecializationEffects<'db, (GenericContext<'db>, &'args [Option<Type<'db>>])>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.class_generic_context(class).await
    }

    async fn specialize(
        &self,
        _context: GenericContext<'db>,
        (initial_context, supplied): (GenericContext<'db>, &'args [Option<Type<'db>>]),
    ) -> RunResult<Specialization<'db>> {
        // The ordinary callback captures the context used to validate the supplied arguments.
        // The repeated lookup decides whether to invoke it without replacing that context.
        self.specialize_supplied(initial_context, supplied).await
    }

    async fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<ClassType<'db>> {
        Ok(ClassType::Generic(
            self.access
                .intern_generic_alias(class, specialization)
                .await?,
        ))
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> AsyncClassContextEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: ClassContextWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn is_version_info(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        Ok(self.field(class.field_requests(self.db()).known()).await?
            == Some(KnownClass::VersionInfo))
    }

    async fn pep695_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        pep695_generic_context_with(class, self).await
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        explicit_class_bases_with(class, self).await
    }

    async fn legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        legacy_generic_context_async_with(class, self).await
    }

    async fn next_base(
        &self,
        cursor: &mut ClassContextBaseCursor<'db>,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.local(1, 0, || cursor.next_base()).await
    }

    async fn inherited_legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        inherited_legacy_generic_context_with(class, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassContextSourceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn has_type_params(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(class.field_requests(self.db()).has_type_params())
            .await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(class.field_requests(self.db()).has_explicit_bases())
            .await
    }

    async fn pep695_generic_context_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.pep695_class_context(class).await
    }

    async fn explicit_bases_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.access.explicit_bases(class).await
    }

    async fn inherited_legacy_generic_context_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.inherited_class_context(class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> AsyncInheritedContextEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: InheritedContextWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn definition(&self, class: StaticClassLiteral<'db>) -> RunResult<Definition<'db>> {
        Ok(self.prepare_class_base_source(class).await?.definition)
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        explicit_class_bases_with(class, self).await
    }

    async fn new_variables(&self) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        self.new_legacy_variables().await
    }

    async fn next_base(
        &self,
        cursor: &mut InheritedContextBaseCursor<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(1, 0, || cursor.next_base()).await
    }

    async fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<()> {
        find_legacy_typevars_with_effects(self.db(), env, base, Some(definition), variables, self)
            .await
    }

    async fn discard_variables(
        &self,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<()> {
        // Disposal was admitted when the owner was created and grown, including error and
        // cancellation paths.
        drop(variables);
        Ok(())
    }

    async fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        self.context_from_legacy_variables(env, variables).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassDefaultSpecializationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.class_generic_context(class).await
    }

    async fn defaults(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<Specialization<'db>> {
        let known = self
            .field(
                class
                    .field_requests(self.access.endpoint().field_request_context())
                    .known(),
            )
            .await?;
        default_specialization_with_effects(self.db(), context, known, self).await
    }

    async fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<ClassType<'db>> {
        Ok(ClassType::Generic(
            self.access
                .intern_generic_alias(class, specialization)
                .await?,
        ))
    }
}

/// Borrows the retained class source while the shared algorithm requests its parameter declarations.
struct ClassHeaderContextSource<'source, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
    index: &'db SemanticIndex<'db>,
}

impl<'ast, 'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassHeaderContextEffects<'db, 'ast>
    for ClassHeaderContextSource<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn parameters(&self, class: &'ast ast::StmtClassDef) -> RunResult<Option<&'ast ast::TypeParams>> {
        // Prepay the shared Some/None branch and its copied result as well as selecting the list.
        self.source.local_with_fixed_transfers(
            6,
            3 * size_of::<Option<GenericContext<'db>>>(),
            || class.type_params.as_deref(),
        ).await
    }

    async fn context(&self, class: &'ast ast::StmtClassDef, parameters: &'ast ast::TypeParams) -> RunResult<GenericContext<'db>> {
        let definition = self.source.local_with_fixed_transfers(3, 0, || {
            self.index.expect_single_definition(class)
        }).await?;
        self.source.type_parameter_future(|| {
            self.source.pep695_context_source(self.index, definition, parameters)
        }).await?.await
    }
}
