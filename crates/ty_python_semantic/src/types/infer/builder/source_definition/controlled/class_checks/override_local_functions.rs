use ruff_db::files::FileRange;
use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::ClassCheckEffects;
use crate::types::Type;
use crate::types::function::{
    FunctionDecorators, FunctionIdentityEffects, FunctionType, KnownFunction, OverloadLiteral,
};
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects};
use crate::types::list_members::Member;
use crate::types::overrides::LocalOverrideDefinition;
use crate::types::overrides::local_functions::{LocalOverrideEffects, OverrideDecoratorSource};
use crate::types::storage_quote::buffer_push_quote;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LocalOverrideEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = SourceEffects::<A>::checked(work)
            .and_then(|work| SourceEffects::<A>::checked(bytes).map(|bytes| (work, bytes)));
        self.source.local_quoted(quote, action).await
    }

    async fn local_functions(
        &self,
        member: &Member<'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<SmallVec<[FunctionType<'db>; 1]>> {
        self.source.local_member_functions(member, scope).await
    }

    async fn in_stub(&self) -> RunResult<bool> {
        let file = self.source.initialize_value(|| self.builder.file()).await?;
        let in_stub = self.source.file_is_stub(file).await?;
        self.source.initialize_value(|| in_stub).await
    }

    async fn overloads(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)> {
        let file = self.source.function_file(function).await?;
        self.source.check_file_program(file).await?;
        function
            .overloads_and_implementation_with(self.source.db(), self.source)
            .await
    }

    async fn first_overload(&self, function: FunctionType<'db>) -> RunResult<OverloadLiteral<'db>> {
        let (overloads, implementation) = LocalOverrideEffects::overloads(self, function).await?;
        self.source
            .local(3, size_of::<RunResult<OverloadLiteral<'db>>>(), || {
                overloads
                    .first()
                    .copied()
                    .or(implementation)
                    .ok_or(RunError::Contract(
                        "override function has no overload or implementation",
                    ))
            })
            .await?
    }

    async fn focus_range(&self, function: OverloadLiteral<'db>) -> RunResult<FileRange> {
        let fields = self.source.access.endpoint().field_request_context();
        let scope = self
            .source
            .field(function.field_requests(fields).body_scope())
            .await?;
        let file = self.source.scope_file(scope).await?;
        self.source.check_file_program(file).await?;
        let physical_file = self.source.physical_file(file).await?;
        self.source
            .local(1, size_of::<RunResult<()>>(), || {
                if physical_file != self.builder.file() {
                    return Err(RunError::Contract("override focus belongs to another file"));
                }
                Ok(())
            })
            .await??;
        let index = self.source.access.semantic_index(file).await?;
        let file_scope = self
            .source
            .field(scope.read_fields(fields).file_scope_id())
            .await?;
        self.source
            .local(4, size_of::<RunResult<FileRange>>(), || {
                let node = index
                    .scope(file_scope)
                    .node()
                    .as_function()
                    .ok_or(RunError::Contract("override focus has no function node"))?;
                Ok(FileRange::new(
                    physical_file,
                    node.node(self.builder.module()).name.range,
                ))
            })
            .await?
    }

    async fn overload_decorators(
        &self,
        function: OverloadLiteral<'db>,
    ) -> RunResult<FunctionDecorators> {
        self.source
            .field(
                function
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .decorators(),
            )
            .await
    }

    async fn decorator_source(
        &self,
        function: OverloadLiteral<'db>,
    ) -> RunResult<OverrideDecoratorSource<'db>> {
        let definition =
            FunctionIdentityEffects::definition(self.source, self.source.db(), function).await?;
        let file = self.source.definition_file(definition).await?;
        self.source.check_file_program(file).await?;
        let physical_file = self.source.physical_file(file).await?;
        let module = self.source.access.parsed_module(file).await?;
        let fields = self.source.access.endpoint().field_request_context();
        let scope = self
            .source
            .field(function.field_requests(fields).body_scope())
            .await?;
        let index = self.source.access.semantic_index(file).await?;
        let file_scope = self
            .source
            .field(scope.read_fields(fields).file_scope_id())
            .await?;
        let node = self
            .source
            .local(
                3,
                size_of::<RunResult<&AstNodeRef<ast::StmtFunctionDef>>>(),
                || {
                    index
                        .scope(file_scope)
                        .node()
                        .as_function()
                        .ok_or(RunError::Contract(
                            "override decorator source has no function node",
                        ))
                },
            )
            .await??;
        self.source
            .initialize_value(|| OverrideDecoratorSource {
                definition,
                file: physical_file,
                module,
                node,
            })
            .await
    }

    async fn decorator_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| {
                self.source
                    .definition_expression_type(definition, expression)
            })
            .await?
            .await
    }

    async fn known_override(&self, ty: Type<'db>) -> RunResult<bool> {
        let function = self
            .source
            .local(1, size_of::<Option<FunctionType<'db>>>(), || {
                ty.as_function_literal()
            })
            .await?;
        let Some(function) = function else {
            return self.source.initialize_value(|| false).await;
        };
        let fields = self.source.access.endpoint().field_request_context();
        let literal = self
            .source
            .field(function.field_requests(fields).literal())
            .await?;
        let known = self
            .source
            .field(literal.last_definition.field_requests(fields).known())
            .await?;
        self.source
            .local(1, size_of::<bool>(), || {
                known == Some(KnownFunction::Override)
            })
            .await
    }

    async fn append_metadata(
        &self,
        definitions: &mut SmallVec<[LocalOverrideDefinition; 1]>,
        definition: LocalOverrideDefinition,
    ) -> RunResult<()> {
        let storage = self
            .source
            .local(3, size_of::<(usize, usize, bool)>(), || {
                (
                    definitions.len(),
                    definitions.capacity(),
                    definitions.spilled(),
                )
            })
            .await?;
        // override_decorator_span_with constructs spans with Span::from(source.file), retaining a
        // Ty file handle with no owned SourceFile payload. This quote covers each record's retirement.
        let quote = buffer_push_quote::<LocalOverrideDefinition>(storage)
            .ok_or(RunError::Contract(
                "override metadata storage quote overflow",
            ))
            .map(|quote| (quote.work, quote.bytes));
        self.source
            .local_quoted(quote, || definitions.push(definition))
            .await
    }
}
