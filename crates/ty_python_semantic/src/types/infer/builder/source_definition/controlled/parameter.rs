//! Canonical parameter inference and admitted ownership of its binding storage.

use ruff_python_ast::{self as ast, AnyNodeRef};
use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::{
    Definition, DefinitionKind, DefinitionNodeKey, FunctionDefinitionKind,
};
use ty_python_core::scope::FileScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{DeclarationsIterator, ImportedFinalCandidatesIterator};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::{
    PlaceAndQualifiers, PlaceFromDeclarationsResult, RequiresExplicitReExport,
    module_type_implicit_global_declaration_with, place_from_declarations_with,
};
use crate::reachability::ReachabilityEvaluationCache;
use crate::types::context::InferContext;
use crate::types::function::FunctionDecorators;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::AddBinding;
use crate::types::infer::builder::function::MethodReceiverKind;
use crate::types::infer::builder::function::receiver::{
    MethodReceiverEffects, infer_method_receiver_with,
};
use crate::types::infer::builder::source_binding::{
    BindingWriteOperation, BindingWriteWork, SourceBindingEffects, sealed,
};
use crate::types::infer::builder::source_parameter::{
    ParameterEffects, ParameterFacts, infer_parameter_definition_with, special_first_parameter_with,
};
use crate::types::subclass_of::{SubclassConstructionFacts, subclass_from_with};
use crate::types::{BoundTypeVarInstance, ClassLiteral, SubclassOfInner, Type};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn infer_parameter_source<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.work(4).await?;
        infer_parameter_definition_with(builder, parameter, definition, ParameterFacts, self).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ParameterEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn annotated(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _parameter: &'ast ast::ParameterWithDefault,
        _definition: Definition<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ParameterAnnotation).await
    }

    async fn default_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _default: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ParameterDefault).await
    }

    async fn receiver_type(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &ast::Parameter,
    ) -> RunResult<Option<Type<'db>>> {
        self.work(6).await?;
        special_first_parameter_with(builder, parameter, ParameterFacts, self).await
    }

    async fn bind(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::Parameter,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        builder
            .add_binding_with(self, parameter.into(), definition)
            .await?
            .insert_with(builder, self, ty)
            .await?;
        Ok(())
    }

    async fn receiver_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<FileScopeId> {
        self.field(builder.scope().read_fields(self.db()).file_scope_id())
            .await
    }

    async fn method_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &ast::Parameter,
        function: &AstNodeRef<ast::StmtFunctionDef>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| {
            infer_method_receiver_with(builder, parameter, function, class, self)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> RunResult<bool> {
        self.file_is_stub(context.file()).await
    }

    async fn checkpoint(&self, work: BindingWriteWork) -> RunResult<()> {
        match work {
            BindingWriteWork::Prepare => {
                self.local(24, size_of::<AddBinding<'db, '_>>(), || ()).await
            }
            BindingWriteWork::InspectPreviousBinding => self.work(2).await,
            BindingWriteWork::StoreBinding { .. } => {
                self.unavailable(SourceOperation::BindingStorage).await
            }
            BindingWriteWork::ReportConflictingDeclarations { .. } => {
                self.unavailable(SourceOperation::BindingDiagnostic).await
            }
        }
    }

    async fn reachability_cache<'builder>(
        &self,
        builder: &'builder TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<&'builder ReachabilityEvaluationCache<'db>> {
        if let Some(cache) = builder.reachability_cache.get() {
            return self.local(1, 0, || cache.as_ref()).await;
        }
        if align_of::<ReachabilityEvaluationCache<'db>>() > align_of::<usize>() {
            return Err(RunError::Contract(
                "reachability cache Rc alignment is unsupported",
            ));
        }
        let bytes = Self::checked(
            size_of::<ReachabilityEvaluationCache<'db>>().checked_add(2 * size_of::<usize>()),
        )?;
        let scope = self
            .field(builder.scope().read_fields(self.db()).file_scope_id())
            .await?;
        // The containers start empty; each later insertion admits its backing and disposal.
        self.local(8, bytes, || builder.reachability_cache_for_scope(scope))
            .await
    }

    async fn store_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        binding: Definition<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let count = builder.bindings.0.len();
        let capacity = builder.bindings.0.capacity();
        let bytes =
            if count == capacity {
                Self::checked(count.checked_add(1).and_then(|count| {
                    count.checked_mul(size_of::<(Definition<'db>, Type<'db>)>())
                }))?
            } else {
                0
            };
        self.local(
            Self::checked(
                capacity
                    .checked_add(count)
                    .and_then(|count| count.checked_add(4)),
            )?,
            bytes,
            || {
                builder.bindings.0.reserve_exact(1);
                builder.bindings.insert(binding, ty);
                #[cfg(test)]
                super::observations::observe(self.db(), super::observations::Event::BindingStored);
            },
        )
        .await
    }

    async fn legacy_operation<T>(
        &self,
        operation: BindingWriteOperation,
        _body: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.unavailable(match operation {
            BindingWriteOperation::AssignmentValidation => SourceOperation::AssignmentValidation(
                crate::analysis::AssignmentValidationOperation::Legacy,
            ),
            BindingWriteOperation::ConflictingDeclarations
            | BindingWriteOperation::FinalReassignment => SourceOperation::BindingDiagnostic,
        })
        .await
    }

    async fn declarations(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        self.work(Self::checked(
            declarations
                .traversal_len()
                .checked_mul(4)
                .and_then(|count| count.checked_add(1)),
        )?)
        .await?;
        place_from_declarations_with(
            env,
            self,
            declarations,
            RequiresExplicitReExport::No,
            Some(cache),
        )
        .await
    }

    async fn imported_final(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: PlaceFromDeclarationsResult<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        self.work(Self::checked(
            candidates
                .traversal_len()
                .checked_mul(4)
                .and_then(|count| count.checked_add(1)),
        )?)
        .await?;
        declared
            .with_imported_final_with(
                env,
                self,
                candidates,
                RequiresExplicitReExport::No,
                Some(cache),
                true,
            )
            .await
    }

    async fn forwarded_assignment_owner(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
        symbol: ScopedSymbolId,
    ) -> RunResult<Option<(FileScopeId, ScopedSymbolId)>> {
        let local = self
            .local(2, 0, || {
                scope.is_global() || builder.index.place_table(scope).symbol(symbol).is_local()
            })
            .await?;
        if !local {
            return self.unavailable(SourceOperation::BindingOwner).await;
        }
        self.local(2, 0, || builder.forwarded_assignment_owner(scope, symbol))
            .await
    }

    async fn implicit_module_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        prior: PlaceAndQualifiers<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let db = builder.db();
        let env = builder.program_environment();
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        match prior.into_lookup_result_with(db, &env, self).await? {
            Ok(place) => Ok(Ok(place).into()),
            Err(error) => {
                let fallback =
                    module_type_implicit_global_declaration_with(db, &env, self, name).await?;
                Ok(error
                    .or_fall_back_to_with(db, &env, self, fallback)
                    .await?
                    .into())
            }
        }
    }

    async fn fallback_member_declared_type(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _node: AnyNodeRef<'_>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::BindingMember).await
    }

    async fn validate_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> RunResult<bool> {
        self.validate_assignment_source(builder, node, binding, declaration, target, value)
            .await
    }

    async fn attribute_assignment_transforms_value(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _value: &ast::Expr,
        _attribute: &str,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::BindingMember).await
    }

    async fn safe_subscript_assignment(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _value: &ast::Expr,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::BindingMember).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> MethodReceiverEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn initialize_value<T>(&self, make: impl FnOnce() -> T) -> RunResult<T> {
        SourceEffects::initialize_value(self, make).await
    }

    async fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        key: DefinitionNodeKey,
    ) -> RunResult<Definition<'db>> {
        let file = self.scope_file(builder.scope()).await?;
        self.check_file_program(file).await?;
        let work = Self::checked(builder.index.definition_lookup_work().checked_add(2))?;
        self.local(work, size_of::<Definition<'db>>(), || {
            builder.index.expect_single_definition(key)
        })
        .await
    }

    async fn kind(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        self.field(
            definition
                .read_fields(self.access.endpoint().field_request_context())
                .kind(),
        )
        .await
    }

    async fn function_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> RunResult<&'ast ast::StmtFunctionDef> {
        self.local(2, size_of::<&'ast ast::StmtFunctionDef>(), || {
            function.node(builder.module())
        })
        .await
    }

    async fn parameter_index(
        &self,
        function: &ast::StmtFunctionDef,
        parameter: &ast::Parameter,
    ) -> RunResult<Option<usize>> {
        let parameters = &function.parameters;
        let count = Self::checked(
            parameters
                .posonlyargs
                .len()
                .checked_add(parameters.args.len())
                .and_then(|count| count.checked_add(parameters.kwonlyargs.len())),
        )?;
        let comparison = Self::checked(parameter.name().len().checked_add(2))?;
        let work = Self::checked(
            count
                .checked_mul(comparison)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, size_of::<Option<usize>>(), || {
            parameters.index(parameter.name())
        })
        .await
    }

    async fn in_class_scope(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let scope = self.definition_scope(definition).await?;
        let metadata = self.scope_metadata(scope).await?;
        self.initialize_value(|| metadata.kind().is_class()).await
    }

    async fn known_decorators(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<FunctionDecorators> {
        let inference = self.access.function_known_decorators(definition).await?;
        self.initialize_value(|| inference.known_decorators()).await
    }

    async fn classify(
        &self,
        function: &ast::StmtFunctionDef,
        decorators: FunctionDecorators,
    ) -> RunResult<Option<MethodReceiverKind>> {
        let work = Self::checked(
            function
                .name
                .id
                .len()
                .checked_add(1)
                .and_then(|length| length.checked_mul(4))
                .and_then(|work| work.checked_add(6)),
        )?;
        self.local(work, size_of::<Option<MethodReceiverKind>>(), || {
            MethodReceiverKind::from_decorators(function, decorators)
        })
        .await
    }

    async fn original_class(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Option<ClassLiteral<'db>>> {
        let original = self.scope_original_class_type(definition).await?;
        self.initialize_value(|| original.and_then(Type::as_class_literal))
            .await
    }

    async fn typing_self(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.typing_self_for_method(builder.scope(), definition, class)
            .await
    }

    async fn subclass(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Type<'db>> {
        let inner = self
            .initialize_value(|| SubclassOfInner::TypeVar(variable))
            .await?;
        subclass_from_with(inner, SubclassConstructionFacts, self).await
    }
}
