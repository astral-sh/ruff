use std::convert::Infallible;

use ruff_python_ast as ast;
use strum::IntoEnumIterator;
use ty_module_resolver::ImportingFile;
use ty_python_core::definition::{AnnotatedAssignmentDefinitionKind, Definition};
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::ScopeKind;

use super::annotation_expression::PEP613Policy;
use super::{
    AddBinding, DeclaredAndInferredType, DeferredExpressionState, TypeInferenceBuilder, local,
};
use crate::types::class::{ClassLiteral, CodeGeneratorKind, StaticClassLiteral};
use crate::types::diagnostic::{
    INVALID_ENUM_MEMBER_ANNOTATION, INVALID_PARAMSPEC, INVALID_TYPE_FORM,
    report_invalid_type_checking_constant,
};
use crate::types::enums::{enum_ignored_names, is_enum_class_by_inheritance};
use crate::types::infer::nearest_enclosing_class;
use crate::types::special_form::{TypeQualifier, TypeQualifierIter};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance};
use crate::types::{
    CallableType, DynamicType, InternedType, KnownClass, KnownInstanceType, ParamSpecAttrKind,
    SpecialFormType, Type, TypeAndQualifiers, TypeContext, TypeQualifiers, TypeVarKind,
};

#[cfg(feature = "experimental-analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum AnnotatedAssignmentOperation {
    Scope,
    ReceiverTarget,
    RejectedTarget,
    FieldSpecifiers,
    ParamSpecValidation,
    ClassQualifier,
    TypeChecking,
    Value,
    AliasReplacement,
    EnumIgnoredNames,
    DeclarationOnly,
    Diagnostic,
}

pub(in crate::types::infer) struct QualifierCursor(TypeQualifierIter);

impl QualifierCursor {
    pub(in crate::types::infer) fn next(&mut self) -> Option<TypeQualifier> {
        self.0.next()
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct AnnotatedAssignmentFacts;

pub(super) struct OrdinaryAnnotatedAssignmentEffects;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousAnnotatedAssignmentEffects)]
    pub(in crate::types::infer) trait AnnotatedAssignmentEffects<'db, 'ast> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn target_value(&self, builder: &TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind) -> Result<(&'ast ast::Expr, Option<&'ast ast::Expr>), Self::Error>;
        #[operation(source)]
        async fn annotation_node(&self, builder: &TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind) -> Result<&'ast ast::Expr, Self::Error>;
        #[operation(source)]
        async fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn defer_annotations(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<DeferredExpressionState, Self::Error>;
        #[operation(child)]
        async fn setup_field_specifiers(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn clear_field_specifiers(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn annotation(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, deferred: DeferredExpressionState, policy: PEP613Policy) -> Result<TypeAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn assignment_annotation(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind) -> Result<TypeAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn valid_receiver_target(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn rejected_target(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn paramspec_annotation(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, declared: TypeAndQualifiers<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_qualifier(&self, qualifiers: &mut QualifierCursor) -> Result<Option<TypeQualifier>, Self::Error>;
        #[operation(source)]
        async fn scope_kind(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<ScopeKind, Self::Error>;
        #[operation(child)]
        async fn class_qualifier(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, qualifier: TypeQualifier) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn invalid_module_qualifier(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, qualifier: TypeQualifier) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn type_checking(&self, builder: &TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, value: Option<&ast::Expr>, declared: TypeAndQualifiers<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn special_form(&self, builder: &TypeInferenceBuilder<'db, 'ast>, name: &str) -> Result<Option<SpecialFormType>, Self::Error>;
        #[operation(local)]
        async fn place_invariant(&self, target: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(checkpoint)]
        async fn value_checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn value_deferred_state(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<DeferredExpressionState, Self::Error>;
        #[operation(local)]
        async fn defer_alias_value(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn bind_value_typevars(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(child)]
        async fn infer_value(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, value: &ast::Expr, declared: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn overwrite_alias_value(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, value: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn alias_contains_self(&self, builder: &TypeInferenceBuilder<'db, 'ast>, alias: InternedType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn unknown_string_alias(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn restore_value_context(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, binding: Option<Definition<'db>>, deferred: DeferredExpressionState) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn alias_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, typevar: TypeVarInstance<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn top_callable(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn value_is_subtype(&self, builder: &TypeInferenceBuilder<'db, 'ast>, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn value_nearest_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn value_is_enum(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn enum_ignores_name(&self, builder: &TypeInferenceBuilder<'db, 'ast>, name: &str) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn invalid_enum_annotation(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, name: &str) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn value_declaration_binding(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, definition: Definition<'db>, types: DeclaredAndInferredType<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn value_assignment(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind, definition: Definition<'db>, declared: TypeAndQualifiers<'db>, is_pep_613_type_alias: bool, value: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn missing_alias_value(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn same_declaration_and_binding(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, definition: Definition<'db>, declared: TypeAndQualifiers<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn declaration_only(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, definition: Definition<'db>, declared: TypeAndQualifiers<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn store_target(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl AnnotatedAssignmentFacts {
        fn is_name(&self, target: &ast::Expr) -> bool { target.is_name_expr() }
        fn paramspec_candidate(&self, annotation: &ast::Expr, declared: TypeAndQualifiers<'_>) -> bool {
            matches!(declared.inner_type(), Type::TypeVar(_))
                || matches!(annotation, ast::Expr::Attribute(attribute) if matches!(attribute.attr.as_str(), "args" | "kwargs"))
        }
        fn is_type_alias(&self, declared: TypeAndQualifiers<'_>) -> bool { declared.inner_type().is_typealias_special_form() }
        fn has_qualifiers(&self, declared: TypeAndQualifiers<'_>) -> bool { !declared.qualifiers.is_empty() }
        fn qualifiers(&self) -> QualifierCursor { QualifierCursor(TypeQualifier::iter()) }
        fn has_qualifier(&self, declared: TypeAndQualifiers<'_>, qualifier: TypeQualifier) -> bool { declared.qualifiers.contains(TypeQualifiers::from(qualifier)) }
        fn is_class(&self, scope: ScopeKind) -> bool { scope == ScopeKind::Class }
        fn is_final(&self, qualifier: TypeQualifier) -> bool { qualifier == TypeQualifier::Final }
        fn is_type_checking(&self, target: &ast::Expr) -> bool { target.as_name_expr().is_some_and(|name| &name.id == "TYPE_CHECKING") }
        fn name<'expr>(&self, target: &'expr ast::Expr) -> Option<&'expr str> { target.as_name_expr().map(|name| name.id.as_str()) }
        fn with_inner<'db>(&self, mut declared: TypeAndQualifiers<'db>, ty: Type<'db>) -> TypeAndQualifiers<'db> { declared.inner = ty; declared }
        fn inner<'db>(&self, declared: TypeAndQualifiers<'db>) -> Type<'db> { declared.inner_type() }

        fn is_typing_self(&self, ty: Type<'_>) -> bool { matches!(ty, Type::SpecialForm(SpecialFormType::TypingSelf)) }
        fn literal_string_alias<'db>(&self, ty: Type<'db>) -> Option<InternedType<'db>> {
            if let Type::KnownInstance(KnownInstanceType::LiteralStringAlias(alias)) = ty { Some(alias) } else { None }
        }
        fn typevar_instance<'db>(&self, ty: Type<'db>) -> Option<TypeVarInstance<'db>> {
            if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = ty { Some(typevar) } else { None }
        }
        fn is_ellipsis(&self, value: &ast::Expr) -> bool { value.is_ellipsis_literal_expr() }
        fn enum_candidate_name<'expr>(&self, target: &'expr ast::Expr) -> Option<&'expr str> {
            let name = target.as_name_expr()?.id.as_str();
            if name.starts_with("__") || matches!(name, "_ignore_" | "_value_" | "_name_") { None } else { Some(name) }
        }
        fn is_bare_final(&self, declared: TypeAndQualifiers<'_>) -> bool {
            // Bare Final is allowed on enum members.
            declared.qualifiers.contains(TypeQualifiers::FINAL)
                && matches!(declared.inner_type(), Type::Dynamic(DynamicType::Unknown))
        }
    }

    /// Infers the assigned value with its declaration as context, restoring temporary state
    /// before validation on success, and stores the selected binding and target expression types.
    /// The caller must restore temporary state if a child returns an error or is cancelled.
    #[synchronous(infer_annotated_assignment_value_sync)]
    #[capabilities(effects = AnnotatedAssignmentEffects, facts = AnnotatedAssignmentFacts)]
    #[passive_values(Type::bool_literal, Type::unknown, TypeAndQualifiers::declared, DeclaredAndInferredType::AreTheSame, DeclaredAndInferredType::MightBeDifferent)]
    pub(in crate::types::infer) async fn infer_annotated_assignment_value_with<'db, 'ast, E: AnnotatedAssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        is_pep_613_type_alias: bool,
        value: &ast::Expr,
        facts: AnnotatedAssignmentFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.value_checkpoint().await?;
        let (target, _) = effects.target_value(builder, assignment).await?;
        let annotation = effects.annotation_node(builder, assignment).await?;
        effects.setup_field_specifiers(builder).await?;

        // We defer the r.h.s. of PEP-613 `TypeAlias` assignments in stub files.
        let previous_deferred_state = effects.value_deferred_state(builder).await?;
        if is_pep_613_type_alias && effects.in_stub(builder).await? {
            effects.defer_alias_value(builder).await?;
        }

        // This might be a PEP-613 type alias (`OptionalList: TypeAlias = list[T] | None`). Use
        // the definition of `OptionalList` as the binding context while inferring the
        // RHS (`list[T] | None`), in order to bind `T` to `OptionalList`.
        let previous_typevar_binding_context = effects.bind_value_typevars(builder, definition).await?;
        let inferred_ty = effects.infer_value(builder, value, facts.inner(declared)).await?;
        let inferred_ty = if is_pep_613_type_alias && facts.is_name(target) {
            // Alias type inference emits the diagnostic, but this runtime value is
            // retained as the alias binding.
            if facts.is_typing_self(inferred_ty) {
                effects.overwrite_alias_value(builder, value, Type::unknown()).await?;
                Type::unknown()
            } else if let Some(alias) = facts.literal_string_alias(inferred_ty)
                && effects.alias_contains_self(builder, alias).await?
            {
                effects.unknown_string_alias(builder).await?
            } else {
                inferred_ty
            }
        } else {
            inferred_ty
        };

        effects.restore_value_context(builder, previous_typevar_binding_context, previous_deferred_state).await?;
        effects.clear_field_specifiers(builder).await?;

        let inferred_ty = if facts.is_type_checking(target) {
            Type::bool_literal(true)
        } else if effects.in_stub(builder).await? && facts.is_ellipsis(value) {
            facts.inner(declared)
        } else {
            inferred_ty
        };

        if is_pep_613_type_alias {
            let binding_ty = if let Some(typevar) = facts.typevar_instance(inferred_ty) {
                effects.alias_typevar(builder, typevar).await?
            } else {
                inferred_ty
            };
            effects.value_declaration_binding(builder, target, definition, DeclaredAndInferredType::AreTheSame(TypeAndQualifiers::declared(binding_ty))).await?;
        } else {
            // Check for annotated enum members. The typing spec states that enum
            // members should not have explicit type annotations.
            if let Some(name) = facts.enum_candidate_name(target)
                && !facts.is_bare_final(declared)
            {
                // Value type would be an enum member at runtime (exclude callables,
                // which are never members)
                let callable = effects.top_callable(builder).await?;
                if !effects.value_is_subtype(builder, inferred_ty, callable).await?
                    && facts.is_class(effects.scope_kind(builder).await?)
                    && let Some(class) = effects.value_nearest_class(builder).await?
                    && effects.value_is_enum(builder, class).await?
                    && !effects.enum_ignores_name(builder, name).await?
                {
                    effects.invalid_enum_annotation(builder, annotation, name).await?;
                }
            }
            effects.value_declaration_binding(builder, target, definition, DeclaredAndInferredType::MightBeDifferent { declared_ty: declared, inferred_ty }).await?;
        }

        effects.store_target(builder, target, inferred_ty).await
    }

    #[synchronous(infer_annotated_assignment_annotation_sync)]
    #[capabilities(effects = AnnotatedAssignmentEffects)]
    #[passive_values(PEP613Policy::Allowed)]
    pub(in crate::types::infer) async fn infer_annotated_assignment_annotation_with<'db, 'ast, E: AnnotatedAssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind, effects: &E,
    ) -> Result<TypeAndQualifiers<'db>, E::Error> {
        let annotation = effects.annotation_node(builder, assignment).await?;
        // Pydantic supports field specifiers in annotations via `Annotated[T, Field(...)]`.
        effects.setup_field_specifiers(builder).await?;
        let deferred = effects.defer_annotations(builder).await?;
        let result = effects.annotation(builder, annotation, deferred, PEP613Policy::Allowed).await;
        effects.clear_field_specifiers(builder).await?;
        result
    }

    #[synchronous(infer_annotated_assignment_definition_step_sync)]
    #[capabilities(effects = AnnotatedAssignmentEffects, facts = AnnotatedAssignmentFacts)]
    #[passive_values(Type::bool_literal, Type::SpecialForm, Type::unknown)]
    async fn infer_annotated_assignment_definition_step_with<'db, 'ast, E: AnnotatedAssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>, facts: AnnotatedAssignmentFacts, effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint(builder).await?;
        let (target, value) = effects.target_value(builder, assignment).await?;
        if !facts.is_name(target) && !effects.valid_receiver_target(builder, target).await? {
            return effects.rejected_target(builder, assignment, definition).await;
        }
        let annotation = effects.annotation_node(builder, assignment).await?;
        #[passive_state]
        let mut declared = effects.assignment_annotation(builder, assignment).await?;
        if facts.paramspec_candidate(annotation, declared) {
            effects.paramspec_annotation(builder, annotation, declared).await?;
        }
        let is_pep_613_type_alias = facts.is_type_alias(declared);
        if facts.has_qualifiers(declared) {
            let mut qualifiers = facts.qualifiers();
            #[cursor_loop]
            while let Some(qualifier) = effects.next_qualifier(&mut qualifiers).await? {
                if !facts.has_qualifier(declared, qualifier) { continue; }
                if !facts.is_class(effects.scope_kind(builder).await?) {
                    if !facts.is_final(qualifier) {
                        effects.invalid_module_qualifier(builder, annotation, qualifier).await?;
                    }
                    continue;
                }
                effects.class_qualifier(builder, annotation, qualifier).await?;
            }
        }
        if facts.is_type_checking(target) {
            effects.type_checking(builder, target, value, declared).await?;
            declared = facts.with_inner(declared, Type::bool_literal(true));
        }
        // Handle various singletons.
        if let Some(name) = facts.name(target)
            && let Some(special_form) = effects.special_form(builder, name).await?
        {
            declared = facts.with_inner(declared, Type::SpecialForm(special_form));
        }
        // If the target of an assignment is not one of the place expressions we support,
        // then they are not definitions, so we can only be here if the target is in a form supported as a place expression.
        // In this case, we can simply store types in `target` below, instead of calling `infer_expression` (which would return `Never`).
        effects.place_invariant(target).await?;
        if let Some(value) = value {
            effects.value_assignment(builder, assignment, definition, declared, is_pep_613_type_alias, value).await?;
        } else {
            if is_pep_613_type_alias {
                effects.missing_alias_value(builder, annotation).await?;
                declared = facts.with_inner(declared, Type::unknown());
            }
            if effects.in_stub(builder).await? {
                effects.same_declaration_and_binding(builder, target, definition, declared).await?;
            } else {
                effects.declaration_only(builder, target, definition, declared).await?;
            }
            effects.store_target(builder, target, facts.inner(declared)).await?;
        }
        Ok(())
    }
}

pub(in crate::types::infer) async fn infer_annotated_assignment_definition_with<
    'db,
    'ast,
    E: AnnotatedAssignmentEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    assignment: &'db AnnotatedAssignmentDefinitionKind,
    definition: Definition<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    infer_annotated_assignment_definition_step_with(
        builder,
        assignment,
        definition,
        AnnotatedAssignmentFacts,
        effects,
    )
    .await
}

pub(super) fn infer_annotated_assignment_definition_sync<
    'db,
    'ast,
    E: SynchronousAnnotatedAssignmentEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    assignment: &'db AnnotatedAssignmentDefinitionKind,
    definition: Definition<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    infer_annotated_assignment_definition_step_sync(
        builder,
        assignment,
        definition,
        AnnotatedAssignmentFacts,
        effects,
    )
}

impl<'db, 'ast> SynchronousAnnotatedAssignmentEffects<'db, 'ast>
    for OrdinaryAnnotatedAssignmentEffects
{
    type Error = Infallible;
    fn checkpoint(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<(), Infallible> {
        Ok(())
    }
    fn target_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> Result<(&'ast ast::Expr, Option<&'ast ast::Expr>), Infallible> {
        Ok((
            assignment.target(builder.module()),
            assignment.value(builder.module()),
        ))
    }
    fn annotation_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> Result<&'ast ast::Expr, Infallible> {
        Ok(assignment.annotation(builder.module()))
    }
    fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(builder.in_stub())
    }
    fn defer_annotations(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<DeferredExpressionState, Infallible> {
        Ok(DeferredExpressionState::from(builder.defer_annotations()))
    }
    fn setup_field_specifiers(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Infallible> {
        builder.setup_dataclass_field_specifiers();
        Ok(())
    }
    fn clear_field_specifiers(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Infallible> {
        builder.dataclass_field_specifiers.clear();
        Ok(())
    }
    fn annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        deferred: DeferredExpressionState,
        policy: PEP613Policy,
    ) -> Result<TypeAndQualifiers<'db>, Infallible> {
        Ok(local::annotation(builder, annotation, deferred, policy))
    }
    fn assignment_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> Result<TypeAndQualifiers<'db>, Infallible> {
        infer_annotated_assignment_annotation_sync(builder, assignment, self)
    }
    fn valid_receiver_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
    ) -> Result<bool, Infallible> {
        Ok(builder.is_valid_receiver_annotation_target(target))
    }
    fn rejected_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) -> Result<(), Infallible> {
        builder.infer_rejected_annotated_assignment_target(assignment, definition);
        Ok(())
    }
    fn paramspec_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        declared: TypeAndQualifiers<'db>,
    ) -> Result<(), Infallible> {
        builder.validate_annotated_assignment_paramspec(annotation, declared);
        Ok(())
    }
    fn next_qualifier(
        &self,
        qualifiers: &mut QualifierCursor,
    ) -> Result<Option<TypeQualifier>, Infallible> {
        Ok(qualifiers.next())
    }
    fn scope_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<ScopeKind, Infallible> {
        Ok(builder
            .index
            .scope(builder.scope().file_scope_id(builder.db()))
            .kind())
    }
    fn class_qualifier(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        qualifier: TypeQualifier,
    ) -> Result<(), Infallible> {
        builder.validate_annotated_assignment_class_qualifier(annotation, qualifier);
        Ok(())
    }
    fn invalid_module_qualifier(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        qualifier: TypeQualifier,
    ) -> Result<(), Infallible> {
        builder.validate_annotated_assignment_module_qualifier(annotation, qualifier);
        Ok(())
    }
    fn type_checking(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        value: Option<&ast::Expr>,
        declared: TypeAndQualifiers<'db>,
    ) -> Result<(), Infallible> {
        builder.validate_annotated_assignment_type_checking(target, value, declared);
        Ok(())
    }
    fn special_form(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> Result<Option<SpecialFormType>, Infallible> {
        Ok(SpecialFormType::try_from_file_and_name(
            builder.db(),
            ImportingFile::File(
                builder.file(),
                builder
                    .program_environment()
                    .resolver_environment(builder.db()),
            ),
            name,
        ))
    }
    fn place_invariant(&self, target: &ast::Expr) -> Result<(), Infallible> {
        debug_assert!(PlaceExpr::try_from_expr(target).is_some());
        Ok(())
    }
    fn value_checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn value_deferred_state(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<DeferredExpressionState, Infallible> {
        Ok(builder.deferred_state)
    }

    fn defer_alias_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Infallible> {
        builder.replace_deferred_state(DeferredExpressionState::Deferred);
        Ok(())
    }

    fn bind_value_typevars(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(builder.typevar_binding_context.replace(definition))
    }

    fn infer_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
        declared: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_maybe_standalone_expression(value, TypeContext::new(Some(declared))))
    }

    fn overwrite_alias_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.expressions.insert(value.into(), ty);
        Ok(())
    }

    fn alias_contains_self(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        alias: InternedType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(alias
            .inner(builder.db())
            .contains_self(builder.db(), builder.program_environment()))
    }

    fn unknown_string_alias(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::KnownInstance(KnownInstanceType::LiteralStringAlias(
            InternedType::new(builder.db(), Type::unknown()),
        )))
    }

    fn restore_value_context(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        binding: Option<Definition<'db>>,
        deferred: DeferredExpressionState,
    ) -> Result<(), Infallible> {
        builder.typevar_binding_context = binding;
        builder.deferred_state = deferred;
        Ok(())
    }

    fn alias_typevar(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Type<'db>, Infallible> {
        let identity = TypeVarIdentity::new(
            builder.db(),
            typevar.identity(builder.db()).name(builder.db()),
            typevar.identity(builder.db()).definition(builder.db()),
            TypeVarKind::Pep613Alias,
        );
        Ok(Type::KnownInstance(KnownInstanceType::TypeVar(
            typevar.with_identity(builder.db(), identity),
        )))
    }

    fn top_callable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::Callable(CallableType::top(builder.db())))
    }

    fn value_is_subtype(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.is_subtype_of(builder.db(), builder.program_environment(), target))
    }

    fn value_nearest_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(nearest_enclosing_class(
            builder.db(),
            builder.index,
            builder.scope(),
        ))
    }

    fn value_is_enum(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(is_enum_class_by_inheritance(
            builder.db(),
            builder.program_environment(),
            class,
        ))
    }

    fn enum_ignores_name(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(enum_ignored_names(builder.db(), builder.scope()).contains(name))
    }

    fn invalid_enum_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        name: &str,
    ) -> Result<(), Infallible> {
        if let Some(builder) = builder
            .context
            .report_lint(&INVALID_ENUM_MEMBER_ANNOTATION, annotation)
        {
            let mut diag = builder.into_diagnostic(format_args!(
                "Type annotation on enum member `{}` is not allowed",
                name
            ));
            diag.info("See: https://typing.python.org/en/latest/spec/enums.html#enum-members");
        }
        Ok(())
    }

    fn value_declaration_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        definition: Definition<'db>,
        types: DeclaredAndInferredType<'db>,
    ) -> Result<(), Infallible> {
        builder.add_declaration_with_binding(target.into(), definition, &types);
        Ok(())
    }

    fn value_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        is_pep_613_type_alias: bool,
        value: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.infer_annotated_assignment_value(
            assignment,
            definition,
            declared,
            is_pep_613_type_alias,
            value,
        );
        Ok(())
    }
    fn missing_alias_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
    ) -> Result<(), Infallible> {
        if let Some(builder) = builder.context.report_lint(&INVALID_TYPE_FORM, annotation) {
            builder
                .into_diagnostic("`TypeAlias` must be assigned a value in annotated assignments");
        }
        Ok(())
    }
    fn same_declaration_and_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
    ) -> Result<(), Infallible> {
        builder.add_declaration_with_binding(
            target.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(declared),
        );
        Ok(())
    }
    fn declaration_only(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
    ) -> Result<(), Infallible> {
        builder.add_declaration(target.into(), definition, declared);
        Ok(())
    }
    fn store_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.store_expression_type(target, ty);
        Ok(())
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    fn infer_rejected_annotated_assignment_target(
        &mut self,
        assignment: &AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) {
        let target = assignment.target(self.module());
        let value = assignment.value(self.module());
        // Omit this definition from `self.declarations`; declaration lookup treats an absent
        // inferred declaration as rejected.
        if !definition
            .kind(self.db())
            .category(self.in_stub(), self.module())
            .is_binding()
        {
            return;
        }

        let node = target.into();
        let add = AddBinding {
            declared_ty: self.fallback_member_declared_type(node),
            declaration: None,
            binding: definition,
            node,
            qualifiers: TypeQualifiers::empty(),
            is_local: true,
            has_final_declaration: false,
        };
        let target_ty = if let Some(value) = value {
            // Infer the value as an ordinary assignment without using the rejected annotation
            // as its declared type.
            let value_ty = self.infer_maybe_standalone_expression(value, add.type_context());
            Self::stub_placeholder_binding_type(self.in_stub(), value).unwrap_or(value_ty)
        } else {
            // Annotation-only definitions are bindings in stubs.
            add.declared_ty.unwrap_or(Type::unknown())
        };
        self.store_expression_type(target, target_ty);
        add.insert(self, target_ty);
    }

    fn validate_annotated_assignment_paramspec(
        &mut self,
        annotation: &ast::Expr,
        declared: TypeAndQualifiers<'db>,
    ) {
        // P.args and P.kwargs are only valid as annotations on *args and **kwargs,
        // not as variable annotations. Check both resolved type and AST form.
        if let Type::TypeVar(typevar) = declared.inner_type()
            && typevar.is_paramspec(self.db())
            && let Some(attr) = typevar.paramspec_attr(self.db())
        {
            let name = typevar.name(self.db());
            let (attr_name, variadic) = match attr {
                ParamSpecAttrKind::Args => ("args", "*args"),
                ParamSpecAttrKind::Kwargs => ("kwargs", "**kwargs"),
            };
            if let Some(builder) = self.context.report_lint(&INVALID_PARAMSPEC, annotation) {
                builder.into_diagnostic(format_args!(
                    "`{name}.{attr_name}` is only valid \
                    for annotating `{variadic}` function parameters",
                ));
            }
        } else if let ast::Expr::Attribute(attr_expr) = annotation
            && matches!(attr_expr.attr.as_str(), "args" | "kwargs")
        {
            // Also check the AST form for cases where P isn't bound (e.g., class body
            // annotations). In this case, the type might not resolve to a TypeVar.
            let value_ty = self.expression_type(&attr_expr.value);
            if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = value_ty
                && typevar.is_paramspec(self.db())
            {
                let name = typevar.name(self.db());
                let attr_name = &attr_expr.attr;
                let variadic = if attr_name == "args" {
                    "*args"
                } else {
                    "**kwargs"
                };
                if let Some(builder) = self.context.report_lint(&INVALID_PARAMSPEC, annotation) {
                    builder.into_diagnostic(format_args!(
                        "`{name}.{attr_name}` is only valid \
                        for annotating `{variadic}` function parameters",
                    ));
                }
            }
        }
    }

    fn validate_annotated_assignment_module_qualifier(
        &self,
        annotation: &ast::Expr,
        qualifier: TypeQualifier,
    ) {
        match qualifier {
            TypeQualifier::Final => {}
            TypeQualifier::ClassVar => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, annotation) {
                    builder.into_diagnostic("`ClassVar` is only allowed in class bodies");
                }
            }
            TypeQualifier::InitVar => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, annotation) {
                    builder.into_diagnostic("`InitVar` is only allowed in dataclass fields");
                }
            }
            TypeQualifier::NotRequired | TypeQualifier::ReadOnly | TypeQualifier::Required => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, annotation) {
                    builder.into_diagnostic(format_args!(
                        "`{name}` is only allowed in TypedDict fields",
                        name = qualifier.name()
                    ));
                }
            }
        }
    }

    fn validate_annotated_assignment_class_qualifier(
        &self,
        annotation: &ast::Expr,
        qualifier: TypeQualifier,
    ) {
        match validate_class_qualifier_sync(
            self,
            annotation,
            qualifier,
            ClassQualifierFacts,
            &OrdinaryAnnotatedAssignmentEffects,
        ) {
            Ok(()) => (),
            Err(never) => match never {},
        }
    }

    fn validate_annotated_assignment_type_checking(
        &self,
        target: &ast::Expr,
        value: Option<&ast::Expr>,
        declared: TypeAndQualifiers<'db>,
    ) {
        let db = self.db();
        let env = self.program_environment();
        if !KnownClass::Bool
            .to_instance(db, env)
            .is_assignable_to(db, env, declared.inner_type())
        {
            // annotation not assignable from `bool` is an error
            report_invalid_type_checking_constant(&self.context, target.into());
        } else if self.in_stub()
            && value
                .as_ref()
                .is_none_or(|value| value.is_ellipsis_literal_expr())
        {
            // stub file assigning nothing or `...` is fine
        } else if !matches!(
            value
                .as_ref()
                .and_then(|value| value.as_boolean_literal_expr()),
            Some(ast::ExprBooleanLiteral { value: false, .. })
        ) {
            // otherwise, assigning something other than `False` is an error
            report_invalid_type_checking_constant(&self.context, target.into());
        }
    }

    fn infer_annotated_assignment_value(
        &mut self,
        assignment: &AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        is_pep_613_type_alias: bool,
        value: &ast::Expr,
    ) {
        let Ok(()) = infer_annotated_assignment_value_sync(
            self,
            assignment,
            definition,
            declared,
            is_pep_613_type_alias,
            value,
            AnnotatedAssignmentFacts,
            &OrdinaryAnnotatedAssignmentEffects,
        );
    }

}

/// Records a rejected qualifier without allocating a message or constructing a diagnostic guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ClassQualifierDiagnostic {
    NotAllowed {
        qualifier: TypeQualifier,
        field_kind: &'static str,
    },
    OnlyTypedDict(TypeQualifier),
    InitVarOnlyDataclass,
}

/// Maps class kind and qualifier to the ordinary field-annotation restrictions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) struct ClassQualifierFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Resolves the enclosing class and its generator before applying field-annotation restrictions.
    #[synchronous(SynchronousClassQualifierEffects)]
    pub(in crate::types::infer) trait ClassQualifierEffects<'db, 'ast> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn nearest_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn code_generator(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: StaticClassLiteral<'db>) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
        #[operation(source)]
        async fn report(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, diagnostic: ClassQualifierDiagnostic) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ClassQualifierFacts {
        /// Returns the report selected by the field-annotation matrix, or accepts the qualifier.
        const fn diagnostic(&self, kind: Option<CodeGeneratorKind<'_>>, qualifier: TypeQualifier) -> Option<ClassQualifierDiagnostic> {
            match kind {
                Some(CodeGeneratorKind::TypedDict) => match qualifier {
                    TypeQualifier::ReadOnly | TypeQualifier::Required | TypeQualifier::NotRequired => None,
                    TypeQualifier::ClassVar | TypeQualifier::Final | TypeQualifier::InitVar => Some(ClassQualifierDiagnostic::NotAllowed { qualifier, field_kind: "TypedDict" }),
                },
                Some(kind @ (CodeGeneratorKind::DataclassLike(_) | CodeGeneratorKind::Pydantic(_))) => match qualifier {
                    TypeQualifier::ReadOnly | TypeQualifier::Required | TypeQualifier::NotRequired => Some(ClassQualifierDiagnostic::NotAllowed { qualifier, field_kind: kind.name() }),
                    TypeQualifier::ClassVar | TypeQualifier::Final | TypeQualifier::InitVar => None,
                },
                Some(CodeGeneratorKind::NamedTuple) | None => match qualifier {
                    TypeQualifier::ReadOnly | TypeQualifier::Required | TypeQualifier::NotRequired => Some(ClassQualifierDiagnostic::OnlyTypedDict(qualifier)),
                    TypeQualifier::InitVar => Some(ClassQualifierDiagnostic::InitVarOnlyDataclass),
                    TypeQualifier::ClassVar | TypeQualifier::Final => None,
                },
            }
        }
    }

    /// Checks one qualifier in class-body order and reports a rejected combination before the
    /// enclosing annotated-assignment driver stores its declaration.
    #[synchronous(validate_class_qualifier_sync)]
    #[capabilities(effects = ClassQualifierEffects, facts = ClassQualifierFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn validate_class_qualifier_with<'db, 'ast, E: ClassQualifierEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, qualifier: TypeQualifier,
        facts: ClassQualifierFacts, effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint().await?;
        let kind = if let Some(class) = effects.nearest_class(builder).await? {
            effects.code_generator(builder, class).await?
        } else { None };
        if let Some(diagnostic) = facts.diagnostic(kind, qualifier) {
            effects.report(builder, annotation, diagnostic).await?;
        }
        Ok(())
    }
}

impl<'db, 'ast> SynchronousClassQualifierEffects<'db, 'ast> for OrdinaryAnnotatedAssignmentEffects {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn nearest_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(nearest_enclosing_class(
            builder.db(),
            builder.index,
            builder.scope(),
        ))
    }

    fn code_generator(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Infallible> {
        Ok(CodeGeneratorKind::from_class(
            builder.db(),
            ClassLiteral::Static(class),
        ))
    }

    fn report(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: &ast::Expr,
        diagnostic: ClassQualifierDiagnostic,
    ) -> Result<(), Infallible> {
        let Some(report) = builder.context.report_lint(&INVALID_TYPE_FORM, annotation) else {
            return Ok(());
        };
        match diagnostic {
            ClassQualifierDiagnostic::NotAllowed {
                qualifier,
                field_kind,
            } => {
                report.into_diagnostic(format_args!(
                    "`{name}` is not allowed in {field_kind} fields",
                    name = qualifier.name()
                ));
            }
            ClassQualifierDiagnostic::OnlyTypedDict(qualifier) => {
                report.into_diagnostic(format_args!(
                    "`{name}` is only allowed in TypedDict fields",
                    name = qualifier.name()
                ));
            }
            ClassQualifierDiagnostic::InitVarOnlyDataclass => {
                report.into_diagnostic("`InitVar` is only allowed in dataclass fields");
            }
        }
        Ok(())
    }
}
