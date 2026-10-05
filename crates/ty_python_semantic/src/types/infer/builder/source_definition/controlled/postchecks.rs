//! Scope checks reuse canonical definitions, signatures, and declaration reduction.

use ruff_python_ast as ast;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::SemanticIndex;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::{
    AssignmentDefinitionKind, Definition, DefinitionKind, FunctionDefinitionKind,
};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::NodeWithScopeKind;
use ty_python_core::symbol::ScopedSymbolId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::TypeQualifiers;
use crate::place::{RequiresExplicitReExport, place_from_declarations_with};
use crate::types::context::InferContext;
use crate::types::function::{FunctionDecorators, FunctionType, OverloadLiteral};
use crate::types::infer::builder::post_inference::{
    decorator, dynamic_class, final_variable, function, overloaded_function, typeguard,
};
use crate::types::infer::builder::scope::SeenFunctions;
use crate::types::infer::{
    DefinitionInference, DefinitionInferenceExtra, DefinitionTypes, TypeInferenceBuilder,
};
use crate::types::signatures::{Parameter, ReturnCallableTypeVarScope, Signature};
use crate::types::{Type, TypeIsType};

struct ScopePostcheckEffects<'builder, 'access, 'run, 'db: 'run, 'ast, A> {
    source: &'builder SourceEffects<'access, 'run, 'db, A>,
    builder: &'builder TypeInferenceBuilder<'db, 'ast>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn check_scope_function_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<()> {
        let decorators = self
            .local(1, 0, || {
                function.node(builder.module()).decorator_list.as_slice()
            })
            .await?;
        self.check_scope_decorators(builder, definition, decorators)
            .await
    }

    pub(in crate::types::infer::builder) async fn check_scope_class_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<()> {
        let decorators = self
            .local(1, 0, || {
                class.node(builder.module()).decorator_list.as_slice()
            })
            .await?;
        self.check_scope_decorators(builder, definition, decorators)
            .await
    }

    async fn check_scope_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        decorators: &[ast::Decorator],
    ) -> RunResult<()> {
        decorator::check_decorator_calls_with(
            &builder.context,
            definition,
            decorators,
            &ScopePostcheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn scope_original_class_type(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let inference = self.access.definition(definition).await?;
        let work = Self::checked(inference.bindings(definition).len().checked_add(2))?;
        self.local(work, size_of::<Option<Type<'db>>>(), || {
            inference
                .original_class_type(definition)
                .map(Type::ClassLiteral)
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn check_scope_function_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        function::source_effects::check_function_definition_with(
            definition,
            function::source_effects::FunctionFacts,
            &ScopePostcheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn check_scope_overloaded_function(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        definition: Definition<'db>,
        seen: &mut SeenFunctions<'db>,
    ) -> RunResult<()> {
        let scope = self.scope_metadata(builder.scope).await?;
        let scope = self.local(1, 0, || scope.node()).await?;
        overloaded_function::check_overloaded_function_with(
            &builder.context,
            ty,
            definition,
            scope,
            builder.index,
            &mut seen.overloaded_places,
            &mut seen.public_functions,
            &ScopePostcheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn check_scope_type_guard_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<()> {
        let node = self.local(1, 0, || function.node(builder.module())).await?;
        typeguard::check_type_guard_definition_with(
            &builder.context,
            ty,
            node,
            &ScopePostcheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn check_scope_final_without_value(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<()> {
        final_variable::check_final_without_value_with(
            &builder.context,
            builder.index,
            final_variable::FinalFacts,
            &ScopePostcheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn check_scope_dynamic_class(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        dynamic_class::check_dynamic_class_definition_with(
            &builder.context,
            definition,
            &ScopePostcheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> decorator::DecoratorEffects<'db>
    for ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn decorators_empty(&self, decorators: &[ast::Decorator]) -> RunResult<bool> {
        self.source.local(1, 0, || decorators.is_empty()).await
    }

    async fn canonical_definition(
        &self,
        _context: &InferContext<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        let file = self.source.definition_file(definition).await?;
        self.source.check_file_program(file).await?;
        self.source.access.definition(definition).await
    }

    async fn decorator_cursor<'a>(
        &self,
        decorators: &'a [ast::Decorator],
    ) -> RunResult<decorator::DecoratorCursor<'a>> {
        self.source
            .local(1, 0, || decorator::decorator_cursor(decorators))
            .await
    }

    async fn next_decorator<'a>(
        &self,
        cursor: &mut decorator::DecoratorCursor<'a>,
    ) -> RunResult<Option<&'a ast::Decorator>> {
        self.source
            .local(1, 0, || decorator::next_decorator(cursor))
            .await
    }

    async fn deferred_input(
        &self,
        inference: &DefinitionInference<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Option<Type<'db>>> {
        let length = match inference.extra.as_deref() {
            Some(DefinitionInferenceExtra::Other(extra)) => {
                extra.deferred_decorator_calls.iter().len()
            }
            Some(_) | None => 0,
        };
        let work = SourceEffects::<A>::checked(length.checked_add(3))?;
        self.source
            .local(work, 0, || {
                inference.deferred_decorator_input_type(expression)
            })
            .await
    }

    async fn decorator_type(
        &self,
        inference: &DefinitionInference<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let work = SourceEffects::<A>::checked(inference.expressions.iter().len().checked_add(3))?;
        self.source
            .local(work, 0, || inference.expression_type(expression))
            .await
    }

    async fn replay_call(
        &self,
        _context: &InferContext<'db, '_>,
        _decorator: &ast::Decorator,
        _decorator_ty: Type<'db>,
        _input_ty: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckDecoratorCalls)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> dynamic_class::DynamicClassEffects<'db>
    for ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn definition_kind(
        &self,
        context: &InferContext<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        let file = self.source.definition_file(definition).await?;
        self.source.check_file_program(file).await?;
        self.source
            .field(definition.read_fields(context.db()).kind())
            .await
    }

    async fn assignment_class(
        &self,
        _context: &InferContext<'db, '_>,
        _definition: Definition<'db>,
        _assignment: &AssignmentDefinitionKind<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckDynamicClass)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> function::source_effects::FunctionEffects<'db>
    for ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn canonical_last_definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<OverloadLiteral<'db>>> {
        let file = self.source.definition_file(definition).await?;
        self.source.check_file_program(file).await?;
        let inference = self.source.access.definition(definition).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.declarations.len(),
            _ => 1,
        };
        let work = SourceEffects::<A>::checked(entries.checked_add(2))?;
        let function = self
            .source
            .local(work, 0, || inference.function_type(definition))
            .await?;
        let Some(function) = function else {
            return Ok(None);
        };
        let fields = self.source.access.endpoint().field_request_context();
        let literal = self
            .source
            .field(function.field_requests(fields).literal())
            .await?;
        Ok(Some(literal.last_definition))
    }

    async fn has_no_type_check(&self, last_definition: OverloadLiteral<'db>) -> RunResult<bool> {
        let fields = self.source.access.endpoint().field_request_context();
        let decorators = self
            .source
            .field(last_definition.field_requests(fields).decorators())
            .await?;
        self.source
            .local(1, 0, || {
                decorators.contains(FunctionDecorators::NO_TYPE_CHECK)
            })
            .await
    }

    async fn raw_public_signature(
        &self,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<Signature<'db>> {
        last_definition
            .raw_signature_with(
                self.source.db(),
                ReturnCallableTypeVarScope::Public,
                self.source,
            )
            .await
    }

    async fn function_node<'a>(
        &'a self,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<&'a ast::StmtFunctionDef> {
        let fields = self.source.access.endpoint().field_request_context();
        let scope = self
            .source
            .field(last_definition.field_requests(fields).body_scope())
            .await?;
        let file = self.source.scope_file(scope).await?;
        let file = self.source.physical_file(file).await?;
        self.source
            .local(4, 0, || {
                if file != self.builder.context.file() {
                    return Err(RunError::Contract("function node belongs to another file"));
                }
                Ok(())
            })
            .await??;
        let scope = self.source.scope_metadata(scope).await?;
        self.source
            .local(2, 0, || {
                let Some(function) = scope.node().as_function() else {
                    return Err(RunError::Contract("function scope has no function node"));
                };
                Ok(function.node(self.builder.module()))
            })
            .await?
    }

    async fn parameter_cursor<'a>(
        &self,
        parameters: &'a ast::Parameters,
        signature: &'a Signature<'db>,
    ) -> RunResult<function::source_effects::ParameterCursor<'a, 'db>> {
        self.source
            .local(1, 0, || {
                function::source_effects::parameter_cursor(parameters, signature)
            })
            .await
    }

    async fn next_parameter<'a>(
        &self,
        cursor: &mut function::source_effects::ParameterCursor<'a, 'db>,
    ) -> RunResult<Option<(ast::AnyParameterRef<'a>, &'a Parameter<'db>)>> {
        self.source
            .local(1, 0, || function::source_effects::next_parameter(cursor))
            .await
    }

    async fn report_invalid_legacy_positional(
        &self,
        _parameter: &ast::ParameterWithDefault,
        _previous: Option<&ast::ParameterWithDefault>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckFunctionLegacyPositional)
            .await
    }

    async fn check_pep695_legacy_typevars(
        &self,
        _last_definition: OverloadLiteral<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckFunctionPep695)
            .await
    }

    async fn check_legacy_typevar_defaults(
        &self,
        _last_definition: OverloadLiteral<'db>,
        _signature: &Signature<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckFunctionTypeVarDefaults)
            .await
    }

    async fn check_legacy_typevar_ordering(
        &self,
        _last_definition: OverloadLiteral<'db>,
        _signature: &Signature<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckFunctionTypeVarOrdering)
            .await
    }

    async fn retire_signature(&self, signature: Signature<'db>) -> RunResult<()> {
        self.retire_signature(signature).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn retire_signature(&self, signature: Signature<'db>) -> RunResult<()> {
        let work = SourceEffects::<A>::checked(signature.retirement_work())?;
        self.source.local(work, 0, || drop(signature)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> typeguard::TypeGuardEffects<'db>
    for ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn last_definition(
        &self,
        _context: &InferContext<'db, '_>,
        function: FunctionType<'db>,
    ) -> RunResult<OverloadLiteral<'db>> {
        let fields = self.source.access.endpoint().field_request_context();
        let literal = self
            .source
            .field(function.field_requests(fields).literal())
            .await?;
        Ok(literal.last_definition)
    }

    async fn signature(
        &self,
        context: &InferContext<'db, '_>,
        overload: OverloadLiteral<'db>,
    ) -> RunResult<Signature<'db>> {
        overload.signature_with(context.db(), self.source).await
    }

    async fn type_is_return_type(
        &self,
        _context: &InferContext<'db, '_>,
        type_is: TypeIsType<'db>,
    ) -> RunResult<Type<'db>> {
        let fields = self.source.access.endpoint().field_request_context();
        self.source
            .field(type_is.field_requests(fields).type_argument())
            .await
    }

    async fn check_guard(
        &self,
        _context: &InferContext<'db, '_>,
        _overload: OverloadLiteral<'db>,
        _signature: &Signature<'db>,
        _node: &ast::StmtFunctionDef,
        _type_guard_form_name: &'static str,
        _narrowed_type: Option<Type<'db>>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckTypeGuard)
            .await
    }

    async fn retire_signature(&self, signature: Signature<'db>) -> RunResult<()> {
        self.retire_signature(signature).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    overloaded_function::OverloadedFunctionEffects<'db>
    for ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn is_same_file(
        &self,
        context: &InferContext<'db, '_>,
        function: FunctionType<'db>,
    ) -> RunResult<bool> {
        let file = self.source.function_file(function).await?;
        let file = self.source.physical_file(file).await?;
        self.source.local(2, 0, || file == context.file()).await
    }

    async fn has_overload_decorator(
        &self,
        context: &InferContext<'db, '_>,
        function: FunctionType<'db>,
    ) -> RunResult<bool> {
        let (overloads, implementation) = function
            .overloads_and_implementation_with(context.db(), self.source)
            .await?;
        let mut definitions = overloads.iter().copied().chain(implementation);
        loop {
            let next = self.source.local(1, 0, || definitions.next()).await?;
            let Some(definition) = next else {
                return Ok(false);
            };
            let fields = self.source.access.endpoint().field_request_context();
            let decorators = self
                .source
                .field(definition.field_requests(fields).decorators())
                .await?;
            if self
                .source
                .local(1, 0, || decorators.contains(FunctionDecorators::OVERLOAD))
                .await?
            {
                return Ok(true);
            }
        }
    }

    async fn check_overloads(
        &self,
        _context: &InferContext<'db, '_>,
        _function: FunctionType<'db>,
        _definition: Definition<'db>,
        _scope: &NodeWithScopeKind,
        _index: &SemanticIndex<'db>,
        _seen_overloaded_places: &mut FxHashSet<ScopedPlaceId>,
        _seen_public_functions: &mut FxHashSet<FunctionType<'db>>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckOverloads)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> final_variable::FinalEffects<'db>
    for ScopePostcheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> RunResult<bool> {
        self.source.file_is_stub(context.file()).await
    }

    async fn in_class(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
    ) -> RunResult<bool> {
        let file_scope = self
            .source
            .field(context.scope().read_fields(context.db()).file_scope_id())
            .await?;
        self.source
            .local(2, 0, || index.scope(file_scope).kind().is_class())
            .await
    }

    async fn next_symbol(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<ScopedSymbolId>> {
        let file_scope = self
            .source
            .field(context.scope().read_fields(context.db()).file_scope_id())
            .await?;
        let work = SourceEffects::<A>::checked(cursor.checked_add(2))?;
        self.source
            .local(work, 0, || {
                final_variable::next_symbol(file_scope, index, cursor)
            })
            .await
    }

    async fn declaration(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<(TypeQualifiers, Option<Definition<'db>>)> {
        let file_scope = self
            .source
            .field(context.scope().read_fields(context.db()).file_scope_id())
            .await?;
        let declarations = self
            .source
            .local(2, 0, || {
                index
                    .use_def_map(file_scope)
                    .end_of_scope_symbol_declarations(symbol)
            })
            .await?;
        let retirement = SourceEffects::<A>::checked(declarations.traversal_len().checked_add(2))?;
        let result = place_from_declarations_with(
            context.program_environment(),
            self.source,
            declarations,
            RequiresExplicitReExport::No,
            None,
        )
        .await?;
        self.source
            .local(retirement, 0, || {
                let first_declaration = result.first_declaration;
                let (place_and_quals, _) = result.into_place_and_conflicting_declarations();
                (place_and_quals.qualifiers, first_declaration)
            })
            .await
    }

    async fn check_missing_value(
        &self,
        _context: &InferContext<'db, '_>,
        _index: &SemanticIndex<'db>,
        _symbol: ScopedSymbolId,
        _first_declaration: Option<Definition<'db>>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::PostCheckFinalValue)
            .await
    }
}
