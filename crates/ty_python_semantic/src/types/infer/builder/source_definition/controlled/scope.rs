//! Controlled scope traversal retains its builder until the complete result can be published.

use ruff_db::files::File;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult};
use ty_python_core::{AncestorsIter, ProgramFile, SemanticIndex};
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::{
    AnnotatedAssignmentDefinitionKind, Definition, DefinitionKind, DefinitionNodeKey, FunctionDefinitionKind,
};
use ty_python_core::expression::Expression;
use ty_python_core::scope::{FileScopeId, NodeWithScopeKind, Scope, ScopeId};

use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::DeferredInferenceOperation;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::infer::enclosing_class::{EnclosingClassEffects, nearest_enclosing_class_with};
use crate::ProgramEnvironment;
use crate::types::class::{
    ClassLiteral, DynamicClassAnchor, DynamicEnumAnchor, DynamicNamedTupleAnchor,
    DynamicTypedDictAnchor, StaticClassLiteral,
};
use crate::types::function::FunctionType;
use crate::types::infer::builder::function::{
    function_has_deferred_annotations, parameters_have_defaults,
};
use crate::types::infer::builder::scope::{
    ScopeEffects, ScopeFacts, SeenFunctions, finish_inferred_scope, infer_scope_with,
};
use crate::types::infer::builder::source_statement;
use crate::types::infer::complete_scope::{CompleteScopeEffects, complete_scope_with};
use crate::types::infer::{
    DefinitionInference, InferenceRegion, ScopeInference, TypeInferenceBuilder,
};
use crate::types::{Type, TypeContext};
use ruff_python_ast as ast;

enum ClassScopeSource<'db> {
    Scope(ScopeId<'db>),
    Definition(Definition<'db>),
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CompleteScopeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next_scope(
        &self,
        pending: &mut Option<ScopeId<'db>>,
    ) -> RunResult<Option<ScopeId<'db>>> {
        self.local(1, 0, || pending.take()).await
    }

    async fn accepts_type_context(&self, scope: ScopeId<'db>) -> RunResult<bool> {
        let metadata = self.scope_metadata(scope).await?;
        self.local(1, 0, || metadata.accepts_type_context()).await
    }

    async fn parent_scope(&self, scope: ScopeId<'db>) -> RunResult<Option<ScopeId<'db>>> {
        let file = self.scope_file(scope).await?;
        let index = self.access.semantic_index(file).await?;
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        self.local(2, 0, || {
            index
                .parent_scope_id(file_scope)
                .map(|parent| index.scope_id(parent))
        })
        .await
    }

    async fn infer_scope(&self, scope: ScopeId<'db>) -> RunResult<&'db ScopeInference<'db>> {
        self.access.scope(scope, TypeContext::default()).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Walks ancestors with canonical definition children, retaining only a borrowed cursor.
    pub(in crate::types::infer) async fn nearest_enclosing_class(
        &self,
        index: &SemanticIndex<'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        self.local(1, size_of::<Option<StaticClassLiteral<'db>>>(), || ())
            .await?;
        let scope = self
            .field_with_profile(
                scope.read_fields(self.db()).file_scope_id(),
                &FixedFieldCopy,
            )
            .await?;
        self.allocate_future(|| nearest_enclosing_class_with(index, scope, self))
            .await?
            .await
    }

    pub(in crate::types::infer) async fn complete_scope_types(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db ScopeInference<'db>> {
        complete_scope_with(scope, self).await
    }

    pub(super) async fn definition_scope(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<ScopeId<'db>> {
        self.field_with_profile(
            definition.read_fields(self.db()).scope_id(),
            &FixedFieldCopy,
        )
        .await
    }

    pub(super) async fn expression_scope(
        &self,
        expression: Expression<'db>,
    ) -> RunResult<ScopeId<'db>> {
        self.field(expression.read_fields(self.db()).scope_id())
            .await
    }

    pub(in crate::types::infer) async fn scope_file(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        self.field_with_profile(scope.read_fields(self.db()).program_file(), &FixedFieldCopy)
            .await
    }

    pub(super) async fn scope_metadata(&self, scope: ScopeId<'db>) -> RunResult<&'db Scope> {
        let file = self.scope_file(scope).await?;
        let index = self.access.semantic_index(file).await?;
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        self.local(1, size_of::<&'db Scope>(), || index.scope(file_scope)).await
    }

    pub(super) async fn physical_file(&self, file: ProgramFile<'db>) -> RunResult<File> {
        let fields = self.access.endpoint().field_request_context();
        let python_file = self.field(file.read_fields(fields).python_file()).await?;
        self.field(python_file.read_fields(fields).file()).await
    }

    pub(super) async fn context_file(
        &self,
        file: ProgramFile<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<File> {
        let physical_file = self.physical_file(file).await?;
        let environment_program = self.environment_program(env).await?;
        let fields = self.access.endpoint().field_request_context();
        let file_program = self.field(file.read_fields(fields).program()).await?;
        if environment_program != file_program {
            return Err(RunError::Contract(
                "builder environment belongs to another program",
            ));
        }
        Ok(physical_file)
    }

    pub(in crate::types::infer) async fn definition_file(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        self.scope_file(self.definition_scope(definition).await?)
            .await
    }

    pub(in crate::types::infer) async fn expression_file(
        &self,
        expression: Expression<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        self.scope_file(self.expression_scope(expression).await?)
            .await
    }

    pub(in crate::types::infer) async fn static_class_file(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        let fields = self.access.endpoint().field_request_context();
        let scope = self
            .field_with_profile(class.field_requests(fields).body_scope(), &FixedFieldCopy)
            .await?;
        self.scope_file(scope).await
    }

    pub(in crate::types::infer) async fn class_file(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        let fields = self.access.endpoint().field_request_context();
        let source = match class {
            ClassLiteral::Static(class) => ClassScopeSource::Scope(
                self.field(class.field_requests(fields).body_scope())
                    .await?,
            ),
            ClassLiteral::Dynamic(class) => {
                let anchor = self.field(class.field_requests(fields).anchor()).await?;
                self.local(1, 0, || match anchor {
                    DynamicClassAnchor::Definition(definition) => {
                        ClassScopeSource::Definition(*definition)
                    }
                    DynamicClassAnchor::ScopeOffset { scope, .. } => {
                        ClassScopeSource::Scope(*scope)
                    }
                })
                .await?
            }
            ClassLiteral::DynamicNamedTuple(class) => {
                let anchor = self.field(class.field_requests(fields).anchor()).await?;
                self.local(1, 0, || match anchor {
                    DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
                    | DynamicNamedTupleAnchor::TypingDefinition(definition) => {
                        ClassScopeSource::Definition(*definition)
                    }
                    DynamicNamedTupleAnchor::ScopeOffset { scope, .. } => {
                        ClassScopeSource::Scope(*scope)
                    }
                })
                .await?
            }
            ClassLiteral::DynamicTypedDict(class) => {
                let anchor = self.field(class.field_requests(fields).anchor()).await?;
                self.local(1, 0, || match anchor {
                    DynamicTypedDictAnchor::Definition(definition) => {
                        ClassScopeSource::Definition(*definition)
                    }
                    DynamicTypedDictAnchor::ScopeOffset { scope, .. } => {
                        ClassScopeSource::Scope(*scope)
                    }
                })
                .await?
            }
            ClassLiteral::DynamicEnum(class) => {
                let anchor = self.field(class.field_requests(fields).anchor()).await?;
                self.local(1, 0, || match anchor {
                    DynamicEnumAnchor::Definition { definition, .. } => {
                        ClassScopeSource::Definition(*definition)
                    }
                    DynamicEnumAnchor::ScopeOffset { scope, .. } => ClassScopeSource::Scope(*scope),
                })
                .await?
            }
        };
        let scope = match source {
            ClassScopeSource::Scope(scope) => scope,
            ClassScopeSource::Definition(definition) => self.definition_scope(definition).await?,
        };
        self.scope_file(scope).await
    }

    pub(in crate::types::infer) async fn function_file(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        let fields = self.access.endpoint().field_request_context();
        let literal = self
            .field(function.field_requests(fields).literal())
            .await?;
        let scope = self
            .field(literal.last_definition.field_requests(fields).body_scope())
            .await?;
        self.scope_file(scope).await
    }

    pub(in crate::types::infer) async fn infer_scope(
        &self,
        scope: ScopeId<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<ScopeInference<'db>> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract("prepared scope file is foreign"));
        }
        let env = ProgramEnvironment::from_file(source.file);
        let mut owner = self
            .empty_builder(&source, &env, InferenceRegion::Scope(scope, context))
            .await?;
        infer_scope_with(&mut owner.builder, scope, context, ScopeFacts, self).await?;
        let quote = self.scope_finalization_quote(&owner.builder).await?;
        // Refusal leaves the complete unpublished owner in the suspended source frame.
        // Consume it only after admitting both result construction and discarded fields.
        let mut owner = Some(owner);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(quote.work)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quote.bytes,
                })?;
                endpoint.check_completion()?;
                let owner = owner
                    .take()
                    .ok_or(RunError::Contract("scope owner already consumed"))?;
                Ok(finish_inferred_scope(owner.builder))
            })
            .await)
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ScopeEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn scope_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db NodeWithScopeKind> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let file_scope = self
            .field(scope.read_fields(builder.db()).file_scope_id())
            .await?;
        self.local(2, 0, || builder.index.scope(file_scope).node())
            .await
    }

    async fn infer_module(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> RunResult<()> {
        self.infer_module_source(builder).await
    }

    async fn infer_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &AstNodeRef<ast::StmtFunctionDef>,
    ) -> RunResult<()> {
        let function = self.local(1, 0, || function.node(builder.module())).await?;
        self.infer_function_body_source(builder, function).await
    }

    async fn infer_lambda(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _lambda: &AstNodeRef<ast::ExprLambda>,
        _tcx: TypeContext<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<()> {
        let class = self.local(1, 0, || class.node(builder.module())).await?;
        source_statement::infer_body_with(builder, &class.body, self).await
    }

    async fn infer_class_type_parameters(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_function_type_parameters(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &AstNodeRef<ast::StmtFunctionDef>,
    ) -> RunResult<()> {
        let function = self
            .local_with_fixed_transfers(1, 0, || function.node(builder.module()))
            .await?;
        self.function_annotation_future(|| {
            self.infer_function_type_parameter_annotations(builder, function)
        })
        .await?
        .await
    }

    async fn infer_type_alias_type_parameters(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _type_alias: &AstNodeRef<ast::StmtTypeAlias>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_type_alias(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _type_alias: &AstNodeRef<ast::StmtTypeAlias>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_list_comprehension(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _comprehension: &AstNodeRef<ast::ExprListComp>,
        _tcx: TypeContext<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_set_comprehension(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _comprehension: &AstNodeRef<ast::ExprSetComp>,
        _tcx: TypeContext<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_dict_comprehension(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _comprehension: &AstNodeRef<ast::ExprDictComp>,
        _tcx: TypeContext<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn infer_generator(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _generator: &AstNodeRef<ast::ExprGenerator>,
        _tcx: TypeContext<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopeBody).await
    }

    async fn take_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Vec<Definition<'db>>> {
        self.local(1, 0, || std::mem::take(&mut builder.deferred.0))
            .await
    }

    async fn next_deferred(
        &self,
        definitions: &[Definition<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(1, 0, || {
            let next = definitions.get(*cursor).copied();
            if next.is_some() {
                *cursor += 1;
            }
            next
        })
        .await
    }

    async fn definition_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.field(definition.read_fields(builder.db()).kind())
            .await
    }

    async fn has_deferred_annotations(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<bool> {
        let function = function.node(builder.module());
        let work = Self::checked(
            function
                .parameters
                .len()
                .checked_mul(2)
                .and_then(|count| count.checked_add(4)),
        )?;
        self.local_with_fixed_transfers(work, 0, || function_has_deferred_annotations(function))
            .await
    }

    async fn has_parameter_defaults(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<bool> {
        let parameters = &function.node(builder.module()).parameters;
        let work = Self::checked(
            parameters
                .len()
                .checked_mul(2)
                .and_then(|count| count.checked_add(4)),
        )?;
        self.local_with_fixed_transfers(work, 0, || parameters_have_defaults(parameters))
            .await
    }

    async fn function_default_types(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::FunctionDefaults,
        ))
        .await
    }

    async fn deferred_types(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        self.access.deferred_definition(definition).await
    }

    async fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        inferred: &DefinitionInference<'db>,
    ) -> RunResult<()> {
        self.merge_definition(builder, definition, inferred).await
    }

    async fn check_deferred_empty(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.local(1, 0, || {
            if builder.deferred.is_empty() {
                Ok(())
            } else {
                Err(RunError::Contract(
                    "scope deferred inference added deferred definitions",
                ))
            }
        })
        .await?
    }

    async fn should_check_file(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<bool> {
        self.access.should_check_file(builder.file()).await
    }

    async fn seen_functions(&self) -> RunResult<SeenFunctions<'db>> {
        self.local(2, 0, || SeenFunctions {
            overloaded_places: FxHashSet::default(),
            public_functions: FxHashSet::default(),
        })
        .await
    }

    async fn next_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        cursor: &mut usize,
    ) -> RunResult<Option<(Definition<'db>, Type<'db>)>> {
        self.local(1, 0, || {
            let next = builder
                .declarations
                .0
                .get(*cursor)
                .map(|(definition, ty)| (*definition, ty.inner_type()));
            if next.is_some() {
                *cursor += 1;
            }
            next
        })
        .await
    }

    async fn function_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<()> {
        self.check_scope_function_decorators(builder, definition, function)
            .await
    }

    async fn function_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.check_scope_function_definition(builder, definition)
            .await
    }

    async fn overloaded_function(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        definition: Definition<'db>,
        seen: &mut SeenFunctions<'db>,
    ) -> RunResult<()> {
        self.check_scope_overloaded_function(builder, ty, definition, seen)
            .await
    }

    async fn type_guard_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<()> {
        self.check_scope_type_guard_definition(builder, ty, function)
            .await
    }

    async fn class_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<()> {
        self.check_scope_class_decorators(builder, definition, class)
            .await
    }

    async fn original_class_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.scope_original_class_type(definition).await
    }

    async fn static_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<()> {
        self.check_scope_static_class(builder, ty, class).await
    }

    async fn annotation_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ScopePostcheck).await
    }

    async fn mark_implicit_alias(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _definition: Definition<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ScopePostcheck).await
    }

    async fn dynamic_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.check_scope_dynamic_class(builder, definition).await
    }

    async fn next_called_function(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        cursor: &mut usize,
    ) -> RunResult<Option<FunctionType<'db>>> {
        self.local(1, 0, || {
            let next = builder.called_functions.get_index(*cursor).copied();
            if next.is_some() {
                *cursor += 1;
            }
            next
        })
        .await
    }

    async fn function_definition_id(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<Definition<'db>> {
        function.definition_with(builder.db(), self).await
    }

    async fn final_without_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.check_scope_final_without_value(builder).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EnclosingClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> RunResult<AncestorsIter<'index>> {
        TypeVarBindingEffects::ancestors(self, index, scope).await
    }

    async fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> RunResult<Option<(FileScopeId, &'index Scope)>> {
        TypeVarBindingEffects::next_ancestor(self, ancestors).await
    }

    async fn class_key(&self, scope: &Scope) -> RunResult<Option<DefinitionNodeKey>> {
        self.local(1, size_of::<Option<DefinitionNodeKey>>(), || {
            scope.node().as_class().map(Into::into)
        })
        .await
    }

    async fn definition(
        &self,
        index: &SemanticIndex<'db>,
        key: DefinitionNodeKey,
    ) -> RunResult<Definition<'db>> {
        TypeVarBindingEffects::definition(self, index, key).await
    }

    async fn original_class(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<ClassLiteral<'db>>> {
        let original = self.scope_original_class_type(definition).await?;
        self.local(
            1,
            size_of::<Option<ClassLiteral<'db>>>(),
            || match original {
                Some(Type::ClassLiteral(class)) => Some(class),
                _ => None,
            },
        )
        .await
    }
}
