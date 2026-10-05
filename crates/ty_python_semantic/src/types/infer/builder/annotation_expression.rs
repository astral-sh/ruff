use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::Definition;

use super::{DeferredExpressionState, TypeInferenceBuilder};
use crate::place::TypeOrigin;
use crate::types::diagnostic::{INVALID_TYPE_FORM, REDUNDANT_FINAL_CLASSVAR};
use crate::types::infer::builder::subscript::AnnotatedExprContext;
use crate::types::infer::nearest_enclosing_class;
use crate::types::string_annotation::parse_string_annotation;
use crate::types::{
    SpecialFormType, Type, TypeAndQualifiers, TypeContext, TypeQualifier, TypeQualifiers, todo_type,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(in crate::types::infer) enum PEP613Policy {
    Allowed,
    Disallowed,
}

/// The semantic result of inferring an annotation and the type stored for its expression node.
pub(in crate::types::infer) struct AnnotationExpressionInference<'db> {
    /// The type and qualifiers that the annotation contributes to the declaration.
    pub(in crate::types::infer::builder) annotation_ty: TypeAndQualifiers<'db>,
    pub(in crate::types::infer::builder) storage: AnnotationStorage<'db>,
}

impl<'db> AnnotationExpressionInference<'db> {
    fn new(annotation_ty: TypeAndQualifiers<'db>) -> Self {
        Self {
            storage: AnnotationStorage::Store {
                expression_ty: annotation_ty.inner_type(),
            },
            annotation_ty,
        }
    }

    fn with_expression_type(
        annotation_ty: TypeAndQualifiers<'db>,
        expression_ty: Type<'db>,
    ) -> Self {
        Self {
            annotation_ty,
            storage: AnnotationStorage::Store { expression_ty },
        }
    }
}

/// Annotation expressions.
impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer the type of an annotation expression with the given [`DeferredExpressionState`].
    pub(super) fn infer_annotation_expression(
        &mut self,
        annotation: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> TypeAndQualifiers<'db> {
        self.infer_annotation_expression_inner(annotation, deferred_state, PEP613Policy::Disallowed)
    }

    /// Infer the type of an annotation expression with the given [`DeferredExpressionState`],
    /// allowing a PEP 613 `typing.TypeAlias` annotation.
    pub(super) fn infer_annotation_expression_allow_pep_613(
        &mut self,
        annotation: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> TypeAndQualifiers<'db> {
        self.infer_annotation_expression_inner(annotation, deferred_state, PEP613Policy::Allowed)
    }

    fn infer_annotation_expression_inner(
        &mut self,
        annotation: &ast::Expr,
        deferred_state: DeferredExpressionState,
        pep_613_policy: PEP613Policy,
    ) -> TypeAndQualifiers<'db> {
        super::local::annotation(self, annotation, deferred_state, pep_613_policy)
    }

    /// Implementation of [`infer_annotation_expression`].
    ///
    /// [`infer_annotation_expression`]: TypeInferenceBuilder::infer_annotation_expression
    pub(super) fn infer_annotation_expression_impl(
        &mut self,
        annotation: &ast::Expr,
        pep_613_policy: PEP613Policy,
    ) -> TypeAndQualifiers<'db> {
        super::local::annotation_body(self, annotation, pep_613_policy)
    }

    /// Infer the type of a string annotation expression.
    fn infer_string_annotation_expression(
        &mut self,
        string: &ast::ExprStringLiteral,
    ) -> TypeAndQualifiers<'db> {
        match parse_string_annotation(&self.context, self.inference_flags(), string) {
            Some(parsed) => {
                self.string_annotations
                    .insert(ruff_python_ast::ExprRef::StringLiteral(string).into());
                // String annotations are always evaluated in the deferred context.
                self.infer_annotation_expression(
                    parsed.expr(),
                    DeferredExpressionState::InStringAnnotation(
                        self.enclosing_node_key(string.into()),
                    ),
                )
            }
            None => TypeAndQualifiers::declared(Type::unknown()),
        }
    }
}

fn infer_annotation_reference_legacy<'db>(
    reference: (Type<'db>, Option<Definition<'db>>),
    annotation: &ast::Expr,
    builder: &mut TypeInferenceBuilder<'db, '_>,
    pep_613_policy: PEP613Policy,
) -> AnnotationExpressionInference<'db> {
    let (ty, definition) = reference;
    let special_case = match ty {
        Type::SpecialForm(special_form) => match special_form {
            SpecialFormType::TypeAlias if pep_613_policy == PEP613Policy::Allowed => {
                Some(TypeAndQualifiers::declared(ty))
            }
            _ => None,
        },
        // Conditional import of `typing.TypeAlias` or `typing_extensions.TypeAlias` on a
        // Python version where the former doesn't exist.
        Type::Union(union)
            if pep_613_policy == PEP613Policy::Allowed
                && union.elements(builder.db()).iter().all(|ty| {
                    matches!(
                        ty,
                        Type::SpecialForm(SpecialFormType::TypeAlias) | Type::Dynamic(_)
                    )
                }) =>
        {
            Some(TypeAndQualifiers::declared(Type::SpecialForm(
                SpecialFormType::TypeAlias,
            )))
        }
        _ => None,
    };

    let annotation_ty = special_case.unwrap_or_else(|| {
        TypeAndQualifiers::declared(
            builder.infer_name_or_attribute_type_expression(ty, definition, annotation),
        )
    });

    AnnotationExpressionInference::new(annotation_ty)
}

fn infer_annotation_subscript_legacy<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    subscript: &ast::ExprSubscript,
    value_ty: Type<'db>,
    definition: Option<Definition<'db>>,
) -> AnnotationExpressionInference<'db> {
    let db = builder.db();
    let slice = &*subscript.slice;
    let annotation_ty = match value_ty {
        Type::SpecialForm(special_form) => match special_form {
            SpecialFormType::Annotated => {
                let inferred = builder.parse_subscription_of_annotated_special_form(
                    subscript,
                    AnnotatedExprContext::AnnotationExpression,
                );
                let in_type_expression = inferred
                    .inner_type()
                    .in_type_expression(db, builder.scope(), None, builder.inference_flags())
                    .unwrap_or_else(|err| {
                        err.into_fallback_type(
                            &builder.context,
                            subscript,
                            builder.inference_flags(),
                        )
                    });
                TypeAndQualifiers::declared(in_type_expression)
                    .with_qualifier(inferred.qualifiers())
            }
            _ => {
                TypeAndQualifiers::declared(builder.infer_subscript_type_expression_no_store(
                    subscript, slice, value_ty, definition,
                ))
            }
        },
        _ => TypeAndQualifiers::declared(
            builder
                .infer_subscript_type_expression_no_store(subscript, slice, value_ty, definition),
        ),
    };

    AnnotationExpressionInference::new(annotation_ty)
}

pub(in crate::types::infer) enum AnnotationStorage<'db> {
    Store {
        /// The type exposed for the annotation expression itself, including to IDE features.
        expression_ty: Type<'db>,
    },
    AlreadyStored,
}

pub(in crate::types::infer) enum AnnotationPending<'expr> {
    Store(&'expr ast::Expr),
    AlreadyStored(&'expr ast::Expr),
}

/// A qualifier awaiting one annotation child. Invalid arity still visits every argument in order.
#[derive(Debug)]
pub(in crate::types::infer) struct QualifierPending<'expr> {
    subscript: &'expr ast::ExprSubscript,
    qualifier: TypeQualifier,
    remaining: &'expr [ast::Expr],
    argument_count: usize,
}

/// A diagnostic produced by the shared qualifier decisions.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer) enum QualifierDiagnostic {
    MissingArgument(TypeQualifier),
    RedundantFinalClassVar,
    NonSelfTypeVariable,
    NestedRequired(TypeQualifier),
    ArgumentCount { qualifier: TypeQualifier, count: usize },
}

pub(in crate::types::infer) enum AnnotationStep<'db, 'expr> {
    Complete(AnnotationExpressionInference<'db>),
    InferAnnotation { pending: QualifierPending<'expr>, annotation: &'expr ast::Expr },
    InferType {
        pending: AnnotationPending<'expr>,
        request: super::type_expression::TypeExpressionRequest<'db, 'expr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer) struct AnnotationFacts;
pub(in crate::types::infer::builder) struct OrdinaryAnnotationEffects;

shared_semantic_family! {
    #[synchronous(SynchronousAnnotationEffects)]
    pub(in crate::types::infer) trait AnnotationEffects<'db, 'ast> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn dotted(&self, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn reference(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(Type<'db>, Option<Definition<'db>>), Self::Error>;
        #[operation(child)]
        async fn finish_receiver(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn convert_reference(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, reference: (Type<'db>, Option<Definition<'db>>)) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn legacy_reference(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, reference: (Type<'db>, Option<Definition<'db>>), policy: PEP613Policy) -> Result<AnnotationExpressionInference<'db>, Self::Error>;
        #[operation(child)]
        async fn legacy_subscript(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, value_ty: Type<'db>, definition: Option<Definition<'db>>) -> Result<AnnotationExpressionInference<'db>, Self::Error>;
        #[operation(child)]
        async fn redundant_final_classvar(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn has_non_self_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_qualifier(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, diagnostic: QualifierDiagnostic) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn store_qualifier_slice(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn string(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, string: &ast::ExprStringLiteral) -> Result<TypeAndQualifiers<'db>, Self::Error>;
    }

    #[finite_capability]
    impl AnnotationFacts {
        fn name_context(&self, name: &ast::ExprName) -> ast::ExprContext { name.ctx }
        fn attribute_context(&self, attribute: &ast::ExprAttribute) -> ast::ExprContext { attribute.ctx }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn invalid_name<'db>(&self) -> Type<'db> { todo_type!("Name expression annotation in Store/Del context") }
        fn invalid_attribute<'db>(&self) -> Type<'db> { todo_type!("Attribute expression annotation in Store/Del context") }
        fn complete<'db, 'expr>(&self, inference: AnnotationExpressionInference<'db>) -> AnnotationStep<'db, 'expr> { AnnotationStep::Complete(inference) }
        fn declared<'db>(&self, ty: Type<'db>) -> AnnotationExpressionInference<'db> { AnnotationExpressionInference::new(TypeAndQualifiers::declared(ty)) }
        fn inferred<'db>(&self, ty: TypeAndQualifiers<'db>) -> AnnotationExpressionInference<'db> { AnnotationExpressionInference::new(ty) }
        fn reference_special(&self, ty: Type<'_>, policy: PEP613Policy) -> bool {
            policy == PEP613Policy::Allowed && matches!(ty, Type::SpecialForm(SpecialFormType::TypeAlias) | Type::Union(_))
        }
        fn subscript_special(&self, ty: Type<'_>) -> bool { matches!(ty, Type::SpecialForm(SpecialFormType::Annotated)) }
        const fn qualifier(&self, ty: Type<'_>) -> Option<TypeQualifier> { match ty { Type::SpecialForm(SpecialFormType::TypeQualifier(qualifier)) => Some(qualifier), _ => None } }
        const fn requires_argument(&self, qualifier: TypeQualifier) -> bool {
            match qualifier {
                TypeQualifier::InitVar | TypeQualifier::ReadOnly | TypeQualifier::Required | TypeQualifier::NotRequired => true,
                TypeQualifier::ClassVar | TypeQualifier::Final => false,
            }
        }
        fn bare_qualifier<'db>(&self, qualifier: TypeQualifier, expression_ty: Type<'db>) -> AnnotationExpressionInference<'db> {
            let annotation_ty = TypeAndQualifiers::new(Type::unknown(), TypeOrigin::Declared, TypeQualifiers::from(qualifier));
            if qualifier == TypeQualifier::Final { AnnotationExpressionInference::with_expression_type(annotation_ty, expression_ty) }
            else { AnnotationExpressionInference::new(annotation_ty) }
        }
        fn arguments<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> &'expr [ast::Expr] {
            match &*subscript.slice { ast::Expr::Tuple(tuple) => &tuple.elts, slice => std::slice::from_ref(slice) }
        }
        const fn count(&self, arguments: &[ast::Expr]) -> usize { arguments.len() }
        const fn single_argument(&self, count: usize) -> bool { count == 1 }
        const fn argument<'expr>(&self, arguments: &'expr [ast::Expr]) -> Option<(&'expr ast::Expr, &'expr [ast::Expr])> { arguments.split_first() }
        fn qualifier_child<'db, 'expr>(&self, subscript: &'expr ast::ExprSubscript, qualifier: TypeQualifier, annotation: &'expr ast::Expr, remaining: &'expr [ast::Expr], argument_count: usize) -> AnnotationStep<'db, 'expr> {
            AnnotationStep::InferAnnotation { pending: QualifierPending { subscript, qualifier, remaining, argument_count }, annotation }
        }
        fn combined_final_classvar(&self, qualifier: TypeQualifier, child: TypeAndQualifiers<'_>) -> bool {
            match qualifier {
                TypeQualifier::Final => child.qualifiers().contains(TypeQualifiers::CLASS_VAR),
                TypeQualifier::ClassVar => child.qualifiers().contains(TypeQualifiers::FINAL),
                TypeQualifier::InitVar | TypeQualifier::ReadOnly | TypeQualifier::Required | TypeQualifier::NotRequired => false,
            }
        }
        fn classvar(&self, qualifier: TypeQualifier) -> bool { qualifier == TypeQualifier::ClassVar }
        fn nested_required(&self, qualifier: TypeQualifier, child: TypeAndQualifiers<'_>) -> bool {
            matches!(qualifier, TypeQualifier::Required | TypeQualifier::NotRequired)
                && child.qualifiers().intersects(TypeQualifiers::REQUIRED | TypeQualifiers::NOT_REQUIRED)
        }
        fn qualified<'db>(&self, child: TypeAndQualifiers<'db>, qualifier: TypeQualifier) -> TypeAndQualifiers<'db> { child.with_qualifier(TypeQualifiers::from(qualifier)) }
        fn inner<'db>(&self, child: TypeAndQualifiers<'db>) -> Type<'db> { child.inner_type() }
        fn tuple_slice(&self, subscript: &ast::ExprSubscript) -> bool { subscript.slice.is_tuple_expr() }
        fn expression_request<'db, 'expr>(&self, annotation: &'expr ast::Expr, already_stored: bool) -> AnnotationStep<'db, 'expr> {
            let (pending, mode) = if already_stored {
                (AnnotationPending::AlreadyStored(annotation), super::type_expression::TypeExpressionMode::Scoped)
            } else {
                (AnnotationPending::Store(annotation), super::type_expression::TypeExpressionMode::NoStore)
            };
            AnnotationStep::InferType { pending, request: super::type_expression::TypeExpressionRequest::Expression { expression: annotation, mode } }
        }
        fn subscript_request<'db, 'expr>(&self, annotation: &'expr ast::Expr, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, definition: Option<Definition<'db>>) -> AnnotationStep<'db, 'expr> {
            AnnotationStep::InferType { pending: AnnotationPending::Store(annotation), request: super::type_expression::TypeExpressionRequest::ResolvedSubscript { subscript, value_ty, definition } }
        }
        fn resumed<'db>(&self, pending: AnnotationPending<'_>, ty: Type<'db>) -> AnnotationExpressionInference<'db> {
            match pending {
                AnnotationPending::Store(_annotation) => AnnotationExpressionInference::new(TypeAndQualifiers::declared(ty)),
                AnnotationPending::AlreadyStored(_annotation) => AnnotationExpressionInference { annotation_ty: TypeAndQualifiers::declared(ty), storage: AnnotationStorage::AlreadyStored },
            }
        }
    }

    #[synchronous(start_annotation_impl_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values(QualifierDiagnostic::MissingArgument, QualifierDiagnostic::ArgumentCount)]
    async fn start_annotation_impl_with<'db, 'ast, 'expr, E: AnnotationEffects<'db, 'ast>>(builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &'expr ast::Expr, policy: PEP613Policy, facts: AnnotationFacts, effects: &E) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
        effects.checkpoint().await?;
        match annotation {
            ast::Expr::Name(name) => {
                let inferred = match facts.name_context(name) {
                    ast::ExprContext::Load => {
                        let reference = effects.reference(builder, annotation).await?;
                        if let Some(qualifier) = facts.qualifier(reference.0) { if facts.requires_argument(qualifier) { effects.report_qualifier(builder, annotation, QualifierDiagnostic::MissingArgument(qualifier)).await?; } facts.bare_qualifier(qualifier, reference.0) }
                        else if facts.reference_special(reference.0, policy) { effects.legacy_reference(builder, annotation, reference, policy).await? }
                        else { let ty = effects.convert_reference(builder, annotation, reference).await?; facts.declared(ty) }
                    }
                    ast::ExprContext::Invalid => facts.declared(facts.unknown()),
                    ast::ExprContext::Store | ast::ExprContext::Del => facts.declared(facts.invalid_name()),
                };
                Ok(facts.complete(inferred))
            }
            ast::Expr::Attribute(attribute) => {
                if !effects.dotted(annotation).await? { return Ok(facts.expression_request(annotation, true)); }
                let inferred = match facts.attribute_context(attribute) {
                    ast::ExprContext::Load => {
                        let reference = effects.reference(builder, annotation).await?;
                        if let Some(qualifier) = facts.qualifier(reference.0) { if facts.requires_argument(qualifier) { effects.report_qualifier(builder, annotation, QualifierDiagnostic::MissingArgument(qualifier)).await?; } facts.bare_qualifier(qualifier, reference.0) }
                        else if facts.reference_special(reference.0, policy) { effects.legacy_reference(builder, annotation, reference, policy).await? }
                        else { let ty = effects.convert_reference(builder, annotation, reference).await?; facts.declared(ty) }
                    }
                    ast::ExprContext::Invalid => facts.declared(facts.unknown()),
                    ast::ExprContext::Store | ast::ExprContext::Del => facts.declared(facts.invalid_attribute()),
                };
                Ok(facts.complete(inferred))
            }
            ast::Expr::Subscript(subscript) => {
                if !effects.dotted(&subscript.value).await? { return Ok(facts.expression_request(annotation, true)); }
                let (ty, definition) = effects.reference(builder, &subscript.value).await?;
                let ty = effects.finish_receiver(builder, &subscript.value, ty).await?;
                if let Some(qualifier) = facts.qualifier(ty) {
                    let arguments = facts.arguments(subscript);
                    let count = facts.count(arguments);
                    if let Some((argument, remaining)) = facts.argument(arguments) {
                        return Ok(facts.qualifier_child(subscript, qualifier, argument, remaining, count));
                    }
                    effects.report_qualifier(builder, annotation, QualifierDiagnostic::ArgumentCount { qualifier, count }).await?;
                    let inference = facts.declared(facts.unknown());
                    if facts.tuple_slice(subscript) { effects.store_qualifier_slice(builder, &subscript.slice, facts.inner(inference.annotation_ty)).await?; }
                    Ok(facts.complete(inference))
                } else if facts.subscript_special(ty) {
                    let inferred = effects.legacy_subscript(builder, subscript, ty, definition).await?;
                    Ok(facts.complete(inferred))
                } else { Ok(facts.subscript_request(annotation, subscript, ty, definition)) }
            }
            ast::Expr::StringLiteral(string) => {
                let ty = effects.string(builder, string).await?;
                Ok(facts.complete(facts.inferred(ty)))
            }
            _ => Ok(facts.expression_request(annotation, false)),
        }
    }

    #[synchronous(resume_qualifier_impl_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values(QualifierDiagnostic::RedundantFinalClassVar, QualifierDiagnostic::NonSelfTypeVariable, QualifierDiagnostic::NestedRequired, QualifierDiagnostic::ArgumentCount)]
    async fn resume_qualifier_impl_with<'db, 'ast, 'expr, E: AnnotationEffects<'db, 'ast>>(builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &'expr ast::Expr, pending: QualifierPending<'expr>, child: TypeAndQualifiers<'db>, facts: AnnotationFacts, effects: &E) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
        effects.checkpoint().await?;
        let QualifierPending { subscript, qualifier, remaining, argument_count } = pending;
        let inference = if facts.single_argument(argument_count) {
            // Emit a diagnostic if ClassVar and Final are combined in a class where
            // Final already implies the semantics of ClassVar. Dataclasses and
            // protocols treat an unqualified Final declaration as an instance
            // attribute, so the combination is meaningful in those classes.
            if facts.combined_final_classvar(qualifier, child) && effects.redundant_final_classvar(builder).await? {
                effects.report_qualifier(builder, annotation, QualifierDiagnostic::RedundantFinalClassVar).await?;
            }
            if facts.classvar(qualifier) && effects.has_non_self_typevar(builder, facts.inner(child)).await? {
                effects.report_qualifier(builder, annotation, QualifierDiagnostic::NonSelfTypeVariable).await?;
            }
            // Reject nested `Required`/`NotRequired`, e.g.
            // `Required[Required[int]]` or `Required[NotRequired[int]]`.
            if facts.nested_required(qualifier, child) {
                effects.report_qualifier(builder, annotation, QualifierDiagnostic::NestedRequired(qualifier)).await?;
            }
            facts.inferred(facts.qualified(child, qualifier))
        } else {
            if let Some((argument, remaining)) = facts.argument(remaining) {
                return Ok(facts.qualifier_child(subscript, qualifier, argument, remaining, argument_count));
            }
            effects.report_qualifier(builder, annotation, QualifierDiagnostic::ArgumentCount { qualifier, count: argument_count }).await?;
            facts.declared(facts.unknown())
        };
        if facts.tuple_slice(subscript) { effects.store_qualifier_slice(builder, &subscript.slice, facts.inner(inference.annotation_ty)).await?; }
        Ok(facts.complete(inference))
    }

    #[synchronous(resume_annotation_impl_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values()]
    async fn resume_annotation_impl_with<'db, 'ast, 'expr, E: AnnotationEffects<'db, 'ast>>(_builder: &mut TypeInferenceBuilder<'db, 'ast>, pending: AnnotationPending<'expr>, child_ty: Type<'db>, facts: AnnotationFacts, effects: &E) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
        effects.checkpoint().await?;
        Ok(facts.complete(facts.resumed(pending, child_ty)))
    }
}

pub(in crate::types::infer) async fn start_annotation_with<
    'db,
    'ast,
    'expr,
    E: AnnotationEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    annotation: &'expr ast::Expr,
    policy: PEP613Policy,
    effects: &E,
) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
    start_annotation_impl_with(builder, annotation, policy, AnnotationFacts, effects).await
}
pub(in crate::types::infer) fn start_annotation_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousAnnotationEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    annotation: &'expr ast::Expr,
    policy: PEP613Policy,
    effects: &E,
) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
    start_annotation_impl_sync(builder, annotation, policy, AnnotationFacts, effects)
}
pub(in crate::types::infer) async fn resume_annotation_with<
    'db,
    'ast,
    'expr,
    E: AnnotationEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    pending: AnnotationPending<'expr>,
    child_ty: Type<'db>,
    effects: &E,
) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
    resume_annotation_impl_with(builder, pending, child_ty, AnnotationFacts, effects).await
}
pub(in crate::types::infer) fn resume_annotation_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousAnnotationEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    pending: AnnotationPending<'expr>,
    child_ty: Type<'db>,
    effects: &E,
) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
    resume_annotation_impl_sync(builder, pending, child_ty, AnnotationFacts, effects)
}

/// Resumes a qualifier with the child's complete annotation, including its origin and provenance.
pub(in crate::types::infer) async fn resume_qualifier_with<'db, 'ast, 'expr, E: AnnotationEffects<'db, 'ast>>(builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &'expr ast::Expr, pending: QualifierPending<'expr>, child: TypeAndQualifiers<'db>, effects: &E) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
    resume_qualifier_impl_with(builder, annotation, pending, child, AnnotationFacts, effects).await
}

pub(in crate::types::infer) fn resume_qualifier_sync<'db, 'ast, 'expr, E: SynchronousAnnotationEffects<'db, 'ast>>(builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &'expr ast::Expr, pending: QualifierPending<'expr>, child: TypeAndQualifiers<'db>, effects: &E) -> Result<AnnotationStep<'db, 'expr>, E::Error> {
    resume_qualifier_impl_sync(builder, annotation, pending, child, AnnotationFacts, effects)
}

impl<'db, 'ast> SynchronousAnnotationEffects<'db, 'ast> for OrdinaryAnnotationEffects {
    type Error = Infallible;
    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn dotted(&self, expression: &ast::Expr) -> Result<bool, Infallible> {
        super::type_expression::dotted_name_sync(
            expression,
            &super::type_expression::OrdinaryTypeExpressionEffects,
        )
    }
    fn reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(Type<'db>, Option<Definition<'db>>), Infallible> {
        Ok(builder.infer_type_expression_reference(expression))
    }
    fn finish_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.finish_expression_type(expression, ty, TypeContext::default()))
    }
    fn convert_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        reference: (Type<'db>, Option<Definition<'db>>),
    ) -> Result<Type<'db>, Infallible> {
        super::type_expression::convert_reference_sync(
            builder,
            expression,
            reference,
            super::type_expression::TypeExpressionFacts,
            &super::type_expression::OrdinaryTypeExpressionEffects,
        )
    }
    fn legacy_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        reference: (Type<'db>, Option<Definition<'db>>),
        policy: PEP613Policy,
    ) -> Result<AnnotationExpressionInference<'db>, Infallible> {
        Ok(infer_annotation_reference_legacy(
            reference, expression, builder, policy,
        ))
    }
    fn legacy_subscript(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<AnnotationExpressionInference<'db>, Infallible> {
        Ok(infer_annotation_subscript_legacy(
            builder, subscript, value_ty, definition,
        ))
    }
    fn redundant_final_classvar(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(nearest_enclosing_class(builder.db(), builder.index, builder.scope()).is_none_or(|class| !class.is_dataclass_like(builder.db()) && !class.is_protocol(builder.db())))
    }

    fn has_non_self_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty.has_non_self_typevar(builder.db(), builder.program_environment()))
    }

    fn report_qualifier(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, annotation: &ast::Expr, diagnostic: QualifierDiagnostic) -> Result<(), Infallible> {
        let lint = match diagnostic { QualifierDiagnostic::RedundantFinalClassVar => &REDUNDANT_FINAL_CLASSVAR, QualifierDiagnostic::MissingArgument(_) | QualifierDiagnostic::NonSelfTypeVariable | QualifierDiagnostic::NestedRequired(_) | QualifierDiagnostic::ArgumentCount { .. } => &INVALID_TYPE_FORM };
        let Some(builder) = builder.context.report_lint(lint, annotation) else { return Ok(()); };
        match diagnostic {
            QualifierDiagnostic::MissingArgument(qualifier) => { builder.into_diagnostic(format_args!("`{}` may not be used without a type argument", qualifier.name())); }
            QualifierDiagnostic::RedundantFinalClassVar => { builder.into_diagnostic(format_args!("`Combining `ClassVar` and `Final` is redundant")); }
            QualifierDiagnostic::NonSelfTypeVariable => { builder.into_diagnostic("`ClassVar` cannot contain type variables"); }
            QualifierDiagnostic::NestedRequired(qualifier) => { builder.into_diagnostic(format_args!("`{qualifier}` cannot be nested inside `Required` or `NotRequired`")); }
            QualifierDiagnostic::ArgumentCount { qualifier, count } => { builder.into_diagnostic(format_args!("Type qualifier `{qualifier}` expected exactly 1 argument, got {count}")); }
        }
        Ok(())
    }

    fn store_qualifier_slice(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, ty: Type<'db>) -> Result<(), Infallible> {
        builder.store_expression_type(slice, ty);
        Ok(())
    }

    fn string(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        string: &ast::ExprStringLiteral,
    ) -> Result<TypeAndQualifiers<'db>, Infallible> {
        Ok(builder.infer_string_annotation_expression(string))
    }
}
