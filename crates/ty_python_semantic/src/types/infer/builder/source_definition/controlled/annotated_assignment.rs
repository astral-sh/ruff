use ruff_python_ast::{self as ast, PythonVersion};
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::{AnnotatedAssignmentDefinitionKind, Definition};
use ty_python_core::scope::ScopeKind;
use ty_python_core::ExpressionNodeKey;

use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::types::class::{ClassLiteral, CodeGeneratorKind, StaticClassLiteral, static_code_generator_with};
use crate::types::infer::builder::annotated_assignment::{
    AnnotatedAssignmentEffects, AnnotatedAssignmentOperation, QualifierCursor,
    ClassQualifierEffects, ClassQualifierFacts, ClassQualifierDiagnostic, validate_class_qualifier_with,
    infer_annotated_assignment_annotation_with, infer_annotated_assignment_value_with, AnnotatedAssignmentFacts,
};
use crate::types::infer::builder::annotation_expression::PEP613Policy;
use crate::types::infer::builder::assignment::AssignmentDefinitionEffects;
use crate::types::infer::builder::source_expression::SourceExpressionEffects;
use crate::types::infer::builder::{DeclaredAndInferredType, DeferredExpressionState, TypeInferenceBuilder, local};
use crate::types::infer::InferenceFlags;
use crate::types::callable::conversion::SubclassCallableEffects;
use crate::types::relation::source::subtyping_condition;
use crate::types::signatures::{Parameters, Signature};
use crate::types::typevar::TypeVarInstance;
use crate::types::special_form::TypeQualifier;
use crate::types::{InternedType, KnownInstanceType, SpecialFormType, Type, TypeAndQualifiers, TypeContext, TypeVarKind};

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AnnotatedAssignmentEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<()> {
        self.work(1).await
    }

    async fn target_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> RunResult<(&'ast ast::Expr, Option<&'ast ast::Expr>)> {
        self.local(8, 0, || {
            (
                assignment.target(builder.module()),
                assignment.value(builder.module()),
            )
        })
        .await
    }

    async fn annotation_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> RunResult<&'ast ast::Expr> {
        self.local(4, 0, || assignment.annotation(builder.module()))
            .await
    }

    async fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.file_is_stub(builder.file()).await
    }

    async fn defer_annotations(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<DeferredExpressionState> {
        if self
            .local(1, 0, || builder.index.has_future_annotations())
            .await?
            || self.file_is_stub(builder.file()).await?
        {
            return Ok(DeferredExpressionState::Deferred);
        }
        let program = self
            .environment_program(builder.program_environment())
            .await?;
        let fields = self.access.endpoint().field_request_context();
        let resolver = self
            .field(program.field_requests(fields).resolver_environment())
            .await?;
        let version = self
            .field(resolver.read_fields(fields).python_version())
            .await?;
        self.local(1, 0, || {
            DeferredExpressionState::from(version >= PythonVersion::PY314)
        })
        .await
    }

    async fn setup_field_specifiers(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.setup_dataclass_field_specifiers(
            builder,
            SourceOperation::AnnotatedAssignment(AnnotatedAssignmentOperation::FieldSpecifiers),
        )
        .await
    }

    async fn clear_field_specifiers(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.local(
            Self::checked(builder.dataclass_field_specifiers.len().checked_add(1))?,
            size_of_val(&builder.dataclass_field_specifiers),
            || builder.dataclass_field_specifiers.clear(),
        )
        .await
    }

    async fn annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        deferred: DeferredExpressionState,
        policy: PEP613Policy,
    ) -> RunResult<TypeAndQualifiers<'db>> {
        let declared =
            local::source::annotation(builder, annotation, deferred, policy, self).await?;
        #[cfg(test)]
        super::observations::observe(self.db(), super::observations::Event::AnnotationCompleted);
        Ok(declared)
    }

    async fn assignment_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> RunResult<TypeAndQualifiers<'db>> {
        infer_annotated_assignment_annotation_with(builder, assignment, self).await
    }

    async fn valid_receiver_target(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::ReceiverTarget,
        ))
        .await
    }

    async fn rejected_target(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _assignment: &AnnotatedAssignmentDefinitionKind,
        _definition: Definition<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::RejectedTarget,
        ))
        .await
    }

    async fn paramspec_annotation(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _annotation: &ast::Expr,
        _declared: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::ParamSpecValidation,
        ))
        .await
    }

    async fn next_qualifier(
        &self,
        qualifiers: &mut QualifierCursor,
    ) -> RunResult<Option<TypeQualifier>> {
        self.local(1, size_of::<Option<TypeQualifier>>(), || qualifiers.next())
            .await
    }

    async fn scope_kind(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<ScopeKind> {
        let scope = self.initialize_value(|| builder.scope()).await?;
        let scope = self
            .field_with_profile(
                scope.read_fields(self.db()).file_scope_id(),
                &FixedFieldCopy,
            )
            .await?;
        self.local(2, size_of::<ScopeKind>(), || {
            builder.index.scope(scope).kind()
        })
        .await
    }

    async fn class_qualifier(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        qualifier: TypeQualifier,
    ) -> RunResult<()> {
        self.allocate_future(|| {
            validate_class_qualifier_with(builder, annotation, qualifier, ClassQualifierFacts, self)
        })
        .await?
        .await
    }

    async fn invalid_module_qualifier(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _annotation: &ast::Expr,
        _qualifier: TypeQualifier,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::Diagnostic,
        ))
        .await
    }

    async fn type_checking(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _value: Option<&ast::Expr>,
        _declared: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::TypeChecking,
        ))
        .await
    }

    async fn special_form(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> RunResult<Option<SpecialFormType>> {
        AssignmentDefinitionEffects::special_form(self, builder, name).await
    }

    async fn place_invariant(&self, target: &ast::Expr) -> RunResult<()> {
        self.local(1, 0, || {
            debug_assert!(target.is_name_expr());
        })
        .await
    }

    async fn value_checkpoint(&self) -> RunResult<()> {
        // Prepay the shared driver's fixed tests, selections, and logical transfers.
        self.local(
            48,
            size_of::<Type<'db>>() * 12
                + size_of::<TypeAndQualifiers<'db>>() * 2
                + size_of::<DeclaredAndInferredType<'db>>() * 2
                + size_of::<Option<Definition<'db>>>() * 2
                + size_of::<DeferredExpressionState>() * 2
                + size_of::<Option<InternedType<'db>>>()
                + size_of::<Option<TypeVarInstance<'db>>>()
                + size_of::<Option<StaticClassLiteral<'db>>>()
                + size_of::<Option<&str>>()
                + size_of::<&ast::Expr>() * 4
                + size_of::<bool>() * 16,
            || (),
        )
        .await
    }

    async fn value_deferred_state(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<DeferredExpressionState> {
        self.initialize_value(|| builder.deferred_state).await
    }

    async fn defer_alias_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.local(2, size_of::<DeferredExpressionState>(), || {
            builder.replace_deferred_state(DeferredExpressionState::Deferred);
        })
        .await
    }

    async fn bind_value_typevars(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(2, size_of::<Option<Definition<'db>>>() * 2, || {
            builder.typevar_binding_context.replace(definition)
        })
        .await
    }

    async fn infer_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
        declared: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let context = self
            .initialize_value(|| TypeContext::new(Some(declared)))
            .await?;
        #[cfg(test)]
        local::observe_annotated_value(builder, local::AnnotatedValueEvent::RhsEntered);
        let inferred = self
            .allocate_future(|| {
                local::source::maybe_standalone_expression(builder, value, context, self)
            })
            .await?
            .await?;
        #[cfg(test)]
        local::observe_annotated_value(builder, local::AnnotatedValueEvent::RhsCompleted);
        Ok(inferred)
    }

    async fn overwrite_alias_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let (entries, capacity) = self
            .local(2, size_of::<(usize, usize)>(), || {
                (builder.expressions.len(), builder.expressions.capacity())
            })
            .await?;
        let entry_bytes = size_of::<(ExpressionNodeKey, Type<'db>)>();
        let (mut quote, retained_slots) =
            super::storage::table_merge::<(ExpressionNodeKey, Type<'db>)>(entries, capacity, 1, 0)
                .ok_or(RunError::Contract(
                    "alias expression replacement quotation overflow",
                ))?;
        if quote.bytes != 0 {
            quote.bytes = Self::checked(
                entries
                    .checked_mul(entry_bytes)
                    .and_then(|bytes| quote.bytes.checked_add(bytes)),
            )?;
        }
        quote.bytes = Self::checked(
            entry_bytes
                .checked_mul(2)
                .and_then(|bytes| quote.bytes.checked_add(bytes)),
        )?;
        quote.work = Self::checked(
            retained_slots
                .checked_mul(2)
                .and_then(|work| quote.work.checked_add(work))
                .and_then(|work| work.checked_add(2)),
        )?;
        self.local(quote.work, quote.bytes, || {
            builder.expressions.insert(value.into(), ty);
        })
        .await
    }

    async fn alias_contains_self(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        alias: InternedType<'db>,
    ) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        let inner = self
            .field_with_profile(alias.field_requests(fields).inner(), &FixedFieldCopy)
            .await?;
        self.allocate_future(|| self.contains_self_source(inner))
            .await?
            .await
    }

    async fn unknown_string_alias(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::AliasReplacement,
        ))
        .await
    }

    async fn restore_value_context(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        binding: Option<Definition<'db>>,
        deferred: DeferredExpressionState,
    ) -> RunResult<()> {
        self.local(
            2,
            size_of::<Option<Definition<'db>>>() + size_of::<DeferredExpressionState>(),
            || {
                builder.typevar_binding_context = binding;
                builder.deferred_state = deferred;
            },
        )
        .await
    }

    async fn alias_typevar(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Type<'db>> {
        let fields = self.access.endpoint().field_request_context();
        let identity = self
            .field_with_profile(typevar.field_requests(fields).identity(), &FixedFieldCopy)
            .await?;
        let name = self.field(identity.field_requests(fields).name()).await?;
        let definition = self
            .field_with_profile(
                identity.field_requests(fields).definition(),
                &FixedFieldCopy,
            )
            .await?;
        let identity = self
            .access
            .intern_typevar_identity(name, definition, TypeVarKind::Pep613Alias)
            .await?;
        let bounds = self
            .field_with_profile(
                typevar.bound_or_constraints_request(fields),
                &FixedFieldCopy,
            )
            .await?;
        let variance = self
            .field_with_profile(
                typevar.field_requests(fields).explicit_variance(),
                &FixedFieldCopy,
            )
            .await?;
        let default = self
            .field_with_profile(typevar.default_request(fields), &FixedFieldCopy)
            .await?;
        let typevar = self
            .access
            .intern_typevar_instance(identity, bounds, variance, default)
            .await?;
        self.initialize_value(|| Type::KnownInstance(KnownInstanceType::TypeVar(typevar)))
            .await
    }

    async fn top_callable(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Type<'db>> {
        let parameters = self
            .allocate_future(|| SubclassCallableEffects::top_parameters(self))
            .await?
            .await?;
        self.local(1, size_of::<Option<Parameters<'db>>>(), || ())
            .await?;
        let mut parameters = Some(parameters);
        let signature = self
            .local(
                3,
                size_of::<Signature<'db>>() + size_of::<Type<'db>>(),
                || {
                    let parameters = parameters.take().ok_or(RunError::Contract(
                        "top callable parameters already consumed",
                    ))?;
                    Ok(Signature::new(parameters, Type::object()))
                },
            )
            .await??;
        self.allocate_future(|| self.single_callable_type(signature))
            .await?
            .await
    }

    async fn value_is_subtype(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            subtyping_condition(
                self.db(),
                builder.program_environment(),
                source,
                target,
                self,
            )
        })
        .await?
        .await
    }

    async fn value_nearest_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let scope = self.initialize_value(|| builder.scope()).await?;
        self.allocate_future(|| self.nearest_enclosing_class(builder.index, scope))
            .await?
            .await
    }

    async fn value_is_enum(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            self.is_enum_class_by_inheritance_source(class, builder.program_environment())
        })
        .await?
        .await
    }

    async fn enum_ignores_name(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _name: &str,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::EnumIgnoredNames,
        ))
        .await
    }

    async fn invalid_enum_annotation(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _annotation: &ast::Expr,
        _name: &str,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::Diagnostic,
        ))
        .await
    }

    async fn value_declaration_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        definition: Definition<'db>,
        types: DeclaredAndInferredType<'db>,
    ) -> RunResult<()> {
        #[cfg(test)]
        local::observe_annotated_value(builder, local::AnnotatedValueEvent::BeforeBinding);
        self.allocate_future(|| {
            self.add_source_declaration_with_binding(builder, target.into(), definition, &types)
        })
        .await?
        .await
    }

    async fn value_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        is_pep_613_type_alias: bool,
        value: &ast::Expr,
    ) -> RunResult<()> {
        let (len, capacity, spilled) = self
            .local(3, size_of::<(usize, usize, bool)>(), || {
                let fields = &builder.dataclass_field_specifiers;
                (fields.len(), fields.capacity(), fields.spilled())
            })
            .await?;
        if len != 0 {
            return self
                .unavailable(SourceOperation::AnnotatedAssignment(
                    AnnotatedAssignmentOperation::FieldSpecifiers,
                ))
                .await;
        }

        // The RHS checkpoint starts after temporary context is installed. Keep this outer
        // checkpoint armed through validation and both final stores, and retain the actual
        // incoming empty buffer so an abort can return its allocation unchanged.
        let work = Self::checked((if spilled { capacity } else { 0 }).checked_add(16))?;
        let bytes = size_of::<local::BuilderStore<'_, 'db, 'ast>>() * 2
            + size_of_val(&builder.dataclass_field_specifiers) * 4
            + size_of::<Option<Definition<'db>>>()
            + size_of::<DeferredExpressionState>()
            + size_of::<InferenceFlags>()
            + size_of::<bool>();
        let mut transaction = self
            .local(work, bytes, || {
                let mut transaction = local::BuilderStore::new(builder);
                transaction.save_annotated_value_fields();
                transaction
            })
            .await?;
        let builder = transaction.get_mut(local::BuilderId::ROOT);
        self.allocate_future(|| {
            infer_annotated_assignment_value_with(
                builder,
                assignment,
                definition,
                declared,
                is_pep_613_type_alias,
                value,
                AnnotatedAssignmentFacts,
                self,
            )
        })
        .await?
        .await?;
        self.local(1, size_of::<bool>(), || {
            transaction.complete();
            #[cfg(test)]
            local::observe_annotated_value(
                transaction.builder(local::BuilderId::ROOT),
                local::AnnotatedValueEvent::Complete,
            );
        })
        .await
    }

    async fn missing_alias_value(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _annotation: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::Diagnostic,
        ))
        .await
    }

    async fn same_declaration_and_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        self.bind_source_qualified_declaration(builder, target.into(), definition, declared)
            .await?;
        #[cfg(test)]
        super::observations::observe(
            self.db(),
            super::observations::Event::AnnotatedDefinitionStored,
        );
        Ok(())
    }

    async fn declaration_only(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        self.add_source_declaration(builder, target.into(), definition, declared)
            .await?;
        #[cfg(test)]
        super::observations::observe(
            self.db(),
            super::observations::Event::AnnotatedDefinitionStored,
        );
        Ok(())
    }

    async fn store_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        SourceExpressionEffects::store_expression(self, builder, target, ty).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn setup_dataclass_field_specifiers<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        unavailable: SourceOperation,
    ) -> RunResult<()> {
        if self
            .local(1, 0, || !builder.dataclass_field_specifiers.is_empty())
            .await?
        {
            return self
                .unavailable(unavailable)
                .await;
        }
        let fields = self.access.endpoint().field_request_context();
        let scope = self
            .field(builder.scope().read_fields(fields).file_scope_id())
            .await?;
        let Some(class_node) = self
            .local(3, 0, || builder.index.scope(scope).node().as_class())
            .await?
        else {
            return Ok(());
        };
        let work = self
            .local(1, 0, || builder.index.definition_lookup_work())
            .await?;
        let definition = self
            .local(work, 0, || {
                builder.index.expect_single_definition(class_node)
            })
            .await?;
        let Some(Type::ClassLiteral(ClassLiteral::Static(class))) =
            self.scope_original_class_type(definition).await?
        else {
            return Ok(());
        };
        let Some(params) = self
            .field(class.field_requests(fields).dataclass_params())
            .await?
        else {
            return match static_code_generator_with(class, self).await? {
                None
                | Some(
                    CodeGeneratorKind::DataclassLike(None)
                    | CodeGeneratorKind::NamedTuple
                    | CodeGeneratorKind::TypedDict,
                ) => Ok(()),
                Some(
                    CodeGeneratorKind::DataclassLike(Some(_)) | CodeGeneratorKind::Pydantic(_),
                ) => {
                    self.unavailable(unavailable)
                    .await
                }
            };
        };
        let specifiers = self
            .field(params.field_requests(fields).field_specifiers())
            .await?;
        if !self.local(1, 0, || specifiers.is_empty()).await? {
            return self
                .unavailable(unavailable)
                .await;
        }
        let storage = self
            .local(3, size_of::<(usize, usize, bool)>(), || {
                let specifiers = &builder.dataclass_field_specifiers;
                (
                    specifiers.len(),
                    specifiers.capacity(),
                    specifiers.spilled(),
                )
            })
            .await?;
        let (len, capacity, spilled) = storage;
        let work = Self::checked(len.checked_add(if spilled { capacity } else { 0 }).and_then(|work| work.checked_add(4)))?;
        let bytes = size_of_val(&builder.dataclass_field_specifiers) * 2;
        self.local(work, bytes, || {
            builder.dataclass_field_specifiers = Default::default()
        })
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ClassQualifierEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local(
            1,
            size_of::<Option<ClassQualifierDiagnostic>>()
                + size_of::<Option<CodeGeneratorKind<'db>>>(),
            || (),
        )
        .await
    }

    async fn nearest_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let scope = self.initialize_value(|| builder.scope()).await?;
        self.nearest_enclosing_class(builder.index, scope).await
    }

    async fn code_generator(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        self.allocate_future(|| static_code_generator_with(class, self))
            .await?
            .await
    }

    async fn report(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _annotation: &ast::Expr,
        _diagnostic: ClassQualifierDiagnostic,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::Diagnostic,
        ))
        .await
    }
}
