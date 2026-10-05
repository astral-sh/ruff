mod starred_cost;

use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};
use super::class_selection::FixedFieldCopy;
use super::storage::{table_merge, table_merge_preparation_quote};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::{Definition, DefinitionKind};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::local_transfer::generated_field_quote;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::annotation_expression::{
    AnnotationEffects, AnnotationExpressionInference, AnnotationStep, PEP613Policy, QualifierDiagnostic, QualifierPending,
};
use crate::types::infer::builder::attribute::{AttributeFacts, infer_attribute_load_with};
use crate::types::infer::builder::type_expression::variable_scope::{
    TypeVariableScopeEffects, TypeVariableScopeFacts, check_type_variable_scope_with,
};
use crate::types::infer::builder::type_expression::{
    self, DottedNamePart, TypeExpressionEffects, TypeExpressionFacts, TypeExpressionPending,
    TypeExpressionRequest, TypeExpressionStep,
};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::type_expression_conversion::{
    TypeExpressionConversionEffects, normalize_subclass_argument_with,
};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance};
use crate::types::type_expression_conversion::special_form::SpecialFormConversionEffects;
use crate::types::visitor::runtime::{RuntimeTypeSearch, RuntimeTypeSearchWith, RuntimeTypeWalk};
use crate::types::infer::builder::source_expression::SourceExpressionEffects;
use crate::types::visitor::{TypeSearchMode, TypeWalkFacts, search_type_with};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, GenericContext, InvalidTypeExpressionError, KnownClass,
    SubclassOfType, Type, TypeAndQualifiers, TypeContext, TypeVarKind, UnionBuilder,
};

/// Matches bound type variables other than `typing.Self`, using admitted identity and kind reads.
struct NonSelfTypeVariable<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RuntimeTypeSearchWith<'run, 'db> for NonSelfTypeVariable<'_, '_, 'run, 'db, A> {
    async fn predicate(&self, _endpoint: &TaskEndpoint<'run, 'db>, ty: Type<'db>) -> RunResult<bool> {
        let variable = self.source.initialize_value(|| ty.as_typevar()).await?;
        let Some(variable) = variable else { return self.source.initialize_value(|| false).await; };
        let fields = self.source.access.endpoint().field_request_context();
        let identity = self.source.field_with_profile(variable.identity_request(fields), &FixedFieldCopy).await?;
        let kind = self.source.field_with_profile(identity.identity.field_requests(fields).kind(), &FixedFieldCopy).await?;
        self.source.initialize_value(|| !matches!(kind, TypeVarKind::TypingSelf)).await
    }
}

struct AliasReferenceShape;

impl<'db> RuntimeTypeSearch<'db> for AliasReferenceShape {
    fn predicate(&self, ty: Type<'db>) -> bool {
        type_expression::may_hide_recursive_alias(ty)
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> TypeExpressionEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local(
            1,
            size_of::<TypeExpressionStep<'db, '_>>()
                + size_of::<
                    Option<crate::types::infer::builder::local::callable_annotation::Request<'_>>,
                >(),
            || (),
        )
        .await
    }
    async fn subclass_checkpoint(&self) -> RunResult<()> {
        // Bound receiver dispatch and result storage, including their fixed result forwarding.
        // Child operations and the local driver's frame/owner transfers are admitted separately.
        type Carriers<'db, 'expr> = (
            [TypeExpressionPending<'db, 'expr>; 2],
            [TypeExpressionRequest<'db, 'expr>; 2],
            [TypeExpressionStep<'db, 'expr>; 6],
            [RunResult<TypeExpressionStep<'db, 'expr>>; 6],
            [Type<'db>; 2],
        );
        self.local_with_fixed_transfers(32, size_of::<Carriers<'db, '_>>(), || ())
            .await
    }
    async fn starred_checkpoint(&self) -> RunResult<()> {
        // Each phase funds the shared branch and its finite argument/result carriers.
        // The local driver separately admits the retained operand continuation.
        self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Phase) }, || ())
            .await
    }
    async fn store_unpack_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::ExprStarred,
        flag: TypeExpressionFlags,
    ) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(const { table_merge_preparation_quote() }, || ())
            .await?;
        let quote = self
            .local_quoted_with_fixed_transfers(
                const { starred_cost::quote(starred_cost::Operation::MapPreparation) },
                || {
                    let (mut quote, slots) = table_merge::<(ExpressionNodeKey, TypeExpressionFlags)>(
                        builder.type_expression_flags.len(),
                        builder.type_expression_flags.capacity(),
                        1,
                        0,
                    )?;
                    // The builder keeps the map after this callback, including on refusal.
                    // Prepay disposal of the replacement backing and the inserted Copy entry.
                    quote.work = quote.work.checked_add(slots)?;
                    quote.bytes = quote.bytes.checked_add(
                        size_of::<(ExpressionNodeKey, TypeExpressionFlags)>().checked_mul(4)?,
                    )?;
                    Some(quote)
                },
            )
            .await?
            .ok_or(RunError::Contract("starred expression flags quotation overflow"))?;
        self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::MapInsertion) }, || ())
            .await?;
        self.local_with_fixed_transfers(quote.work, quote.bytes, || {
            builder.store_type_expression_flags(ast::ExprRef::from(expression), flag);
        })
        .await
    }
    async fn enter_starred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Enter) }, || {
            builder.context.inference_flags.replace(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, true)
        })
        .await
    }
    async fn restore_starred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, previous: bool) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Restore) }, || {
            builder.context.inference_flags.set(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, previous);
        })
        .await
    }
    async fn resolve_starred(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.boxed_future_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Child) }, || {
            NominalSelectionEffects::resolve_alias(self, ty)
        })
        .await?
        .await
    }
    async fn is_unpackable(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> RunResult<bool> {
        let exact_tuple = self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::ExactTuple) }, || {
            ty.as_nominal_instance().and_then(|instance| instance.exact_tuple()).is_some()
        }).await?;
        if exact_tuple {
            return self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Return) }, || true).await;
        }
        let variable = self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::TypeVariable) }, || ty.as_typevar()).await?;
        let Some(variable) = variable else {
            return self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Return) }, || false).await;
        };
        let fields = self.access.endpoint().field_request_context();
        let identity = self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |variable: BoundTypeVarInstance<'db>, fields| variable.identity_request(fields),
                |variable: BoundTypeVarInstance<'db>, fields| variable.identity_request(fields),
            ),
            || self.field_with_profile(variable.identity_request(fields), &FixedFieldCopy),
        ).await?.await?;
        let kind = self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |identity: TypeVarIdentity<'db>, fields| identity.field_requests(fields),
                |identity: TypeVarIdentity<'db>, fields| identity.field_requests(fields).kind(),
            ),
            || self.field_with_profile(identity.identity.field_requests(fields).kind(), &FixedFieldCopy),
        ).await?.await?;
        self.local_quoted_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Kind) }, || kind.is_typevartuple()).await
    }
    async fn invalid_starred(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::ExprStarred) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionInvalid).await
    }
    async fn unknown_tuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<Type<'db>> {
        self.boxed_future_with_fixed_transfers(const { starred_cost::quote(starred_cost::Operation::Child) }, || {
            SpecialFormConversionEffects::homogeneous_tuple(
                self,
                builder.program_environment(),
                Type::unknown(),
            )
        })
        .await?
        .await
    }
    async fn next_dotted<'expr>(
        &self,
        cursor: &mut Option<&'expr ast::Expr>,
    ) -> RunResult<Option<DottedNamePart>> {
        self.local(2, 0, || type_expression::next_dotted_name(cursor))
            .await
    }
    async fn dotted(&self, expression: &ast::Expr) -> RunResult<bool> {
        type_expression::dotted_name_with(expression, self).await
    }
    async fn reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<(Type<'db>, Option<Definition<'db>>)> {
        match expression {
            ast::Expr::Name(name) if name.ctx.is_load() => {
                builder
                    .infer_name_load_with_definition_with(self, name)
                    .await
            }
            ast::Expr::Attribute(attribute) if attribute.ctx.is_load() => {
                let result =
                    infer_attribute_load_with(builder, attribute, AttributeFacts, self).await?;
                self.local(2, 0, || {
                    let resolved = match result {
                        Ok(resolved) | Err(resolved) => resolved,
                    };
                    (resolved.inner_type(), resolved.provenance().definition())
                })
                .await
            }
            _ => {
                self.unavailable(SourceOperation::TypeExpressionLegacy)
                    .await
            }
        }
    }
    async fn finish_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        builder
            .finish_expression_type_with(self, expression, ty, TypeContext::default())
            .await
    }
    async fn enter_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<bool>> {
        self.local(3, 0, || {
            type_expression::enter_subscript_unpack(builder, ty)
        })
        .await
    }
    async fn restore_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        previous: Option<bool>,
    ) -> RunResult<()> {
        self.local(2, 0, || {
            type_expression::restore_subscript_unpack(builder, previous)
        })
        .await
    }
    async fn convert_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        reference: (Type<'db>, Option<Definition<'db>>),
    ) -> RunResult<Type<'db>> {
        type_expression::convert_reference_with(
            builder,
            expression,
            reference,
            TypeExpressionFacts,
            self,
        )
        .await
    }
    async fn recursive_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        definition: Option<Definition<'db>>,
    ) -> RunResult<Option<(Type<'db>, Option<GenericContext<'db>>)>> {
        type_expression::recursive_alias_reference_with(
            builder,
            ty,
            definition,
            TypeExpressionFacts,
            self,
        )
        .await
    }
    async fn has_alias_shape(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(builder.program_environment())
            .await?;
        let mut walk = RuntimeTypeWalk {
            db: self.db(),
            endpoint: self.access.endpoint(),
            query: AliasReferenceShape,
            unavailable: self,
        };
        search_type_with(
            ty,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut walk,
        )
        .await
    }
    async fn resolve_recursive(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _definition: Definition<'db>,
    ) -> RunResult<Option<(Type<'db>, Option<GenericContext<'db>>)>> {
        self.unavailable(SourceOperation::TypeExpressionRecursiveAlias)
            .await
    }
    async fn specialize_recursive(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _alias: Type<'db>,
        _parameters: GenericContext<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionRecursiveAlias)
            .await
    }
    async fn recursive_subscript(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _alias: Type<'db>,
        _parameters: GenericContext<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionRecursiveAlias)
            .await
    }
    async fn known_class(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<KnownClass>> {
        TypeExpressionConversionEffects::class_known(self, class).await
    }
    async fn class_subscript<'expr>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<TypeExpressionStep<'db, 'expr>> {
        type_expression::class_subscript_with(
            builder,
            subscript,
            value_ty,
            class,
            TypeExpressionFacts,
            self,
        )
        .await
    }
    async fn class_generic_context(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        crate::types::infer::builder::subscript::SubscriptEffects::class_generic_context(
            self, builder, class,
        )
        .await
    }
    async fn in_string_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<bool> {
        self.local(1, 0, || builder.in_string_annotation()).await
    }
    async fn non_generic_class_slice(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _slice: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionSubscript)
            .await
    }
    async fn invalid_class_subscript(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _class: ClassLiteral<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionInvalid)
            .await
    }
    async fn tuple<'expr>(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &'expr ast::ExprSubscript,
        mode: crate::types::infer::builder::local::tuple_annotation::ResultMode,
    ) -> RunResult<TypeExpressionStep<'db, 'expr>> {
        self.local_with_fixed_transfers(
            4,
            size_of::<(
                crate::types::infer::builder::local::tuple_annotation::Request<'_>,
                [crate::types::infer::builder::local::tuple_annotation::ResultMode; 2],
            )>(),
            || TypeExpressionStep::Tuple(crate::types::infer::builder::local::tuple_annotation::Request { subscript, mode }),
        ).await
    }
    async fn subscript(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _value_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionSubscript)
            .await
    }
    async fn none(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<Type<'db>> {
        TypeExpressionConversionEffects::none(self, builder.program_environment()).await
    }
    async fn annotation_is_deferred(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<bool> {
        self.local(1, 0, || builder.deferred_state.is_deferred()).await
    }
    async fn annotation_in_stub(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<bool> {
        self.file_is_stub(builder.file()).await
    }
    async fn annotation_in_type_checking_block(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        binary: &ast::ExprBinOp,
    ) -> RunResult<bool> {
        let scope = builder.scope();
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let scope = self
            .field(
                scope
                    .read_fields(self.access.endpoint().field_request_context())
                    .file_scope_id(),
            )
            .await?;
        let work = self.type_checking_range_work(&builder.index, scope).await?;
        self.local(work, 0, || {
            builder.index.is_in_type_checking_block(scope, binary.range())
        })
        .await
    }
    async fn union_runtime_validation(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _binary: &ast::ExprBinOp,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionUnionRuntimeValidation)
            .await
    }
    async fn union(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let env = builder.program_environment();
        self.environment_program(env).await?;
        let union = PairUnionEffects::new_union(self, env).await?;
        let mut union = self
            .local(size_of::<UnionBuilder<'db>>() * 2 + 1, 0, || {
                union.unpack_aliases(false)
            })
            .await?;
        PairUnionEffects::union_add(self, &mut union, left).await?;
        PairUnionEffects::union_add(self, &mut union, right).await?;
        PairUnionEffects::union_build(self, union).await
    }
    async fn legacy(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionLegacy)
            .await
    }
    async fn legacy_subclass(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _slice: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionSubclassArgument)
            .await
    }
    async fn resolved_subclass_subscript(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _slice: &ast::Expr,
        _subscript: &ast::ExprSubscript,
        _value_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionSubclassArgument)
            .await
    }
    async fn store_subclass(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        SourceExpressionEffects::store_expression(self, builder, slice, ty).await
    }
    async fn convert_subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        type_expression::convert_subclass_argument_with(
            builder,
            slice,
            ty,
            TypeExpressionFacts,
            self,
        )
        .await
    }
    async fn paramspec_attribute(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::TypeExpressionParamSpecAttribute)
            .await
    }
    async fn missing_arguments(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        expression: &ast::Expr,
    ) -> RunResult<()> {
        crate::types::diagnostic::report_missing_type_arguments_with(
            &builder.context,
            ty,
            expression,
            self,
        )
        .await
    }
    async fn default_specialize(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        ty.default_specialize_with(self.db(), builder.program_environment(), self)
            .await
    }
    async fn in_type_expression(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        let (scope, binding, flags) = self
            .local(3, 0, || {
                (
                    builder.scope(),
                    builder.typevar_binding_context,
                    builder.inference_flags(),
                )
            })
            .await?;
        ty.in_type_expression_with(self.db(), scope, binding, flags, self)
            .await
    }
    async fn invalid(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _error: InvalidTypeExpressionError<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionInvalid)
            .await
    }
    async fn variable_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        check_type_variable_scope_with(builder, expression, ty, TypeVariableScopeFacts, self).await
    }
    async fn normalize_subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        normalize_subclass_argument_with(self.db(), builder.program_environment(), ty, self).await
    }
    async fn subclass_from_instance(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Result<Type<'db>, Type<'db>>> {
        SubclassOfType::try_from_instance_with(self.db(), builder.program_environment(), ty, self)
            .await
    }
    async fn invalid_subclass(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _slice: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeExpressionSubclassArgument)
            .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> TypeVariableScopeEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn dispatch(&self) -> RunResult<()> {
        self.work(32).await
    }

    async fn bound_typevar(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        TypeVarBindingEffects::bound_typevar(self, typevar).await
    }

    async fn kind(&self, typevar: TypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        TypeVarBindingEffects::kind(self, typevar).await
    }

    async fn in_init_receiver(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.local(2, 0, || {
            builder
                .inference_flags()
                .contains(InferenceFlags::IN_INIT_RECEIVER_ANNOTATION)
        })
        .await
    }

    async fn bound_owner(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        let identity = TypeVarBindingEffects::bound_identity(self, typevar).await?;
        self.local(1, 0, || identity.binding_context.definition())
            .await
    }

    async fn binding_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(1, 0, || builder.typevar_binding_context).await
    }

    async fn in_type_alias(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.local(2, 0, || {
            builder
                .inference_flags()
                .contains(InferenceFlags::IN_TYPE_ALIAS)
        })
        .await
    }

    async fn definition_is_annotated_assignment(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let kind = self
            .field(
                definition
                    .read_fields(self.access.endpoint().field_request_context())
                    .kind(),
            )
            .await?;
        self.local(1, 0, || {
            matches!(kind, DefinitionKind::AnnotatedAssignment(_))
        })
        .await
    }

    async fn definition_is_class(&self, definition: Definition<'db>) -> RunResult<bool> {
        TypeVarBindingEffects::definition_is_class(self, definition).await
    }

    async fn check_unbound(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.local(2, 0, || {
            builder
                .inference_flags()
                .contains(InferenceFlags::CHECK_UNBOUND_TYPEVARS)
        })
        .await
    }

    async fn report_init_receiver(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _typevar: BoundTypeVarInstance<'db>,
        _owner: Definition<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionInitTypeVariableReport)
            .await
    }

    async fn report_alias_capture(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionAliasTypeVariableReport)
            .await
    }

    async fn report_unbound(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _typevar: TypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionUnboundTypeVariableReport)
            .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AnnotationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // A shared entry creates at most one pending record, child result, diagnostic and
        // completion record before returning its step. Reserve their fixed representations.
        self.local(8,
            size_of::<AnnotationStep<'db, '_>>()
                + size_of::<QualifierPending<'_>>()
                + size_of::<TypeAndQualifiers<'db>>()
                + size_of::<AnnotationExpressionInference<'db>>()
                + size_of::<QualifierDiagnostic>(),
            || (),
        ).await
    }
    async fn dotted(&self, expression: &ast::Expr) -> RunResult<bool> {
        type_expression::dotted_name_with(expression, self).await
    }
    async fn reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<(Type<'db>, Option<Definition<'db>>)> {
        TypeExpressionEffects::reference(self, builder, expression).await
    }
    async fn finish_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        TypeExpressionEffects::finish_receiver(self, builder, expression, ty).await
    }
    async fn convert_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        reference: (Type<'db>, Option<Definition<'db>>),
    ) -> RunResult<Type<'db>> {
        type_expression::convert_reference_with(
            builder,
            expression,
            reference,
            TypeExpressionFacts,
            self,
        )
        .await
    }
    async fn legacy_reference(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        reference: (Type<'db>, Option<Definition<'db>>),
        _policy: PEP613Policy,
    ) -> RunResult<AnnotationExpressionInference<'db>> {
        match reference.0 {
            Type::Union(_) => {
                self.unavailable(SourceOperation::AnnotationConditionalAlias)
                    .await
            }
            _ => self.unavailable(SourceOperation::AnnotationQualifier).await,
        }
    }
    async fn legacy_subscript(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _value_ty: Type<'db>,
        _definition: Option<Definition<'db>>,
    ) -> RunResult<AnnotationExpressionInference<'db>> {
        self.unavailable(SourceOperation::AnnotationQualifier).await
    }
    async fn redundant_final_classvar(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.unavailable(SourceOperation::AnnotationQualifier).await
    }

    async fn has_non_self_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> RunResult<bool> {
        self.environment_program(builder.program_environment()).await?;
        self.allocate_future(|| async {
            let mut walk = RuntimeTypeWalk { db: self.db(), endpoint: self.access.endpoint(), query: NonSelfTypeVariable { source: self }, unavailable: self };
            search_type_with(ty, TypeSearchMode::SkipLazyAttributes, TypeWalkFacts, &mut walk).await
        }).await?.await
    }

    async fn report_qualifier(&self, _builder: &mut TypeInferenceBuilder<'db, 'ast>, _annotation: &ast::Expr, _diagnostic: QualifierDiagnostic) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotationQualifier).await
    }

    async fn store_qualifier_slice(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, ty: Type<'db>) -> RunResult<()> {
        SourceExpressionEffects::store_expression(self, builder, slice, ty).await
    }

    async fn string(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _string: &ast::ExprStringLiteral,
    ) -> RunResult<TypeAndQualifiers<'db>> {
        self.unavailable(SourceOperation::AnnotationString).await
    }
}
