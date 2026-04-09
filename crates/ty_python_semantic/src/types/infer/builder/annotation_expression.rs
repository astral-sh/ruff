use ruff_python_ast as ast;
use ruff_python_ast::helpers::is_dotted_name;

use super::{DeferredExpressionState, TypeInferenceBuilder};
use crate::place::TypeOrigin;
use crate::types::diagnostic::{INVALID_TYPE_FORM, REDUNDANT_FINAL_CLASSVAR};
use crate::types::infer::builder::InferenceFlags;
use crate::types::infer::builder::subscript::AnnotatedExprContext;
use crate::types::infer::nearest_enclosing_class;
use crate::types::string_annotation::parse_string_annotation;
use crate::types::{
    SpecialFormType, Type, TypeAndQualifiers, TypeContext, TypeQualifier, TypeQualifiers, todo_type,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum PEP613Policy {
    Allowed,
    Disallowed,
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
        // `DeferredExpressionState::InStringAnnotation` takes precedence over other deferred states.
        // However, if it's not a stringified annotation, we must still ensure that annotation expressions
        // are always deferred in stub files.
        let state = if deferred_state.in_string_annotation() {
            deferred_state
        } else if self.in_stub() {
            DeferredExpressionState::Deferred
        } else {
            deferred_state
        };

        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, state);
        let previous_check_unbound_typevars = self
            .inference_flags
            .replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, true);
        let annotation_ty = self.infer_annotation_expression_impl(annotation, pep_613_policy);
        self.inference_flags.set(
            InferenceFlags::CHECK_UNBOUND_TYPEVARS,
            previous_check_unbound_typevars,
        );
        self.deferred_state = previous_deferred_state;
        annotation_ty
    }

    /// Implementation of [`infer_annotation_expression`].
    ///
    /// [`infer_annotation_expression`]: TypeInferenceBuilder::infer_annotation_expression
    pub(super) fn infer_annotation_expression_impl(
        &mut self,
        annotation: &ast::Expr,
        pep_613_policy: PEP613Policy,
    ) -> TypeAndQualifiers<'db> {
        fn infer_name_or_attribute<'db>(
            ty: Type<'db>,
            annotation: &ast::Expr,
            builder: &TypeInferenceBuilder<'db, '_>,
            pep_613_policy: PEP613Policy,
        ) -> TypeAndQualifiers<'db> {
            let special_case = match ty {
                Type::SpecialForm(special_form) => match special_form {
                    SpecialFormType::TypeQualifier(qualifier) => {
                        match qualifier {
                            TypeQualifier::InitVar
                            | TypeQualifier::ReadOnly
                            | TypeQualifier::NotRequired
                            | TypeQualifier::Required => {
                                if let Some(builder) =
                                    builder.context.report_lint(&INVALID_TYPE_FORM, annotation)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "`{}` may not be used without a type argument",
                                        qualifier.name(),
                                    ));
                                }
                            }
                            TypeQualifier::ClassVar | TypeQualifier::Final => {}
                        }

                        Some(TypeAndQualifiers::new(
                            Type::unknown(),
                            TypeOrigin::Declared,
                            TypeQualifiers::from(qualifier),
                        ))
                    }
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

            special_case.unwrap_or_else(|| {
                TypeAndQualifiers::declared(
                    builder.infer_name_or_attribute_type_expression(ty, annotation),
                )
            })
        }

        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-annotation_expression
        let annotation_ty = match annotation {
            // String annotations: https://typing.python.org/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(string) => self.infer_string_annotation_expression(string),

            ast::Expr::Attribute(attribute) => {
                if !is_dotted_name(annotation) {
                    return TypeAndQualifiers::declared(self.infer_type_expression(annotation));
                }
                match attribute.ctx {
                    ast::ExprContext::Load => infer_name_or_attribute(
                        self.infer_attribute_expression(attribute),
                        annotation,
                        self,
                        pep_613_policy,
                    ),
                    ast::ExprContext::Invalid => TypeAndQualifiers::declared(Type::unknown()),
                    ast::ExprContext::Store | ast::ExprContext::Del => TypeAndQualifiers::declared(
                        todo_type!("Attribute expression annotation in Store/Del context"),
                    ),
                }
            }

            ast::Expr::Name(name) => match name.ctx {
                ast::ExprContext::Load => infer_name_or_attribute(
                    self.infer_name_expression(name),
                    annotation,
                    self,
                    pep_613_policy,
                ),
                ast::ExprContext::Invalid => TypeAndQualifiers::declared(Type::unknown()),
                ast::ExprContext::Store | ast::ExprContext::Del => TypeAndQualifiers::declared(
                    todo_type!("Name expression annotation in Store/Del context"),
                ),
            },

            ast::Expr::Subscript(subscript @ ast::ExprSubscript { value, slice, .. }) => {
                if !is_dotted_name(value) {
                    return TypeAndQualifiers::declared(self.infer_type_expression(annotation));
                }

                let slice = &**slice;
                let value_ty = self.infer_expression(value, TypeContext::default());

                match value_ty {
                    Type::SpecialForm(special_form) => match special_form {
                        SpecialFormType::Annotated => {
                            let inferred = self.parse_subscription_of_annotated_special_form(
                                subscript,
                                AnnotatedExprContext::AnnotationExpression,
                            );
                            let in_type_expression = inferred
                                .inner_type()
                                .in_type_expression(
                                    self.db(),
                                    self.scope(),
                                    None,
                                    self.inference_flags,
                                )
                                .unwrap_or_else(|err| {
                                    err.into_fallback_type(
                                        &self.context,
                                        subscript,
                                        self.inference_flags,
                                    )
                                });
                            TypeAndQualifiers::declared(in_type_expression)
                                .with_qualifier(inferred.qualifiers())
                        }
                        SpecialFormType::TypeQualifier(qualifier) => {
                            let arguments = if let ast::Expr::Tuple(tuple) = slice {
                                &*tuple.elts
                            } else {
                                std::slice::from_ref(slice)
                            };
                            let type_and_qualifiers = if let [argument] = arguments {
                                let type_and_qualifiers = self.infer_annotation_expression_impl(
                                    argument,
                                    PEP613Policy::Disallowed,
                                );

                                // Emit a diagnostic if ClassVar and Final are combined in a class that is
                                // not a dataclass, since Final already implies the semantics of ClassVar.
                                let classvar_and_final = match qualifier {
                                    TypeQualifier::Final => type_and_qualifiers
                                        .qualifiers
                                        .contains(TypeQualifiers::CLASS_VAR),
                                    TypeQualifier::ClassVar => type_and_qualifiers
                                        .qualifiers
                                        .contains(TypeQualifiers::FINAL),
                                    _ => false,
                                };
                                if classvar_and_final
                                    && nearest_enclosing_class(self.db(), self.index, self.scope())
                                        .is_none_or(|class| !class.is_dataclass_like(self.db()))
                                    && let Some(builder) = self
                                        .context
                                        .report_lint(&REDUNDANT_FINAL_CLASSVAR, subscript)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "`Combining `ClassVar` and `Final` is redundant"
                                    ));
                                }

                                if qualifier == TypeQualifier::ClassVar
                                    && type_and_qualifiers
                                        .inner_type()
                                        .has_non_self_typevar(self.db())
                                    && let Some(builder) =
                                        self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                                {
                                    builder.into_diagnostic(
                                        "`ClassVar` cannot contain type variables",
                                    );
                                }

                                // Reject nested `Required`/`NotRequired`, e.g.
                                // `Required[Required[int]]` or `Required[NotRequired[int]]`.
                                if matches!(
                                    qualifier,
                                    TypeQualifier::Required | TypeQualifier::NotRequired
                                ) && type_and_qualifiers.qualifiers.intersects(
                                    TypeQualifiers::REQUIRED | TypeQualifiers::NOT_REQUIRED,
                                ) && let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "`{qualifier}` cannot be nested inside \
                                        `Required` or `NotRequired`",
                                    ));
                                }

                                type_and_qualifiers.with_qualifier(TypeQualifiers::from(qualifier))
                            } else {
                                for element in arguments {
                                    self.infer_annotation_expression_impl(
                                        element,
                                        PEP613Policy::Disallowed,
                                    );
                                }
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                                {
                                    let num_arguments = arguments.len();
                                    builder.into_diagnostic(format_args!(
                                        "Type qualifier `{qualifier}` expected exactly 1 \
                                        argument, got {num_arguments}",
                                    ));
                                }
                                TypeAndQualifiers::declared(Type::unknown())
                            };
                            if slice.is_tuple_expr() {
                                self.store_expression_type(slice, type_and_qualifiers.inner_type());
                            }
                            type_and_qualifiers
                        }
                        _ => TypeAndQualifiers::declared(
                            self.infer_subscript_type_expression_no_store(
                                subscript, slice, value_ty,
                            ),
                        ),
                    },
                    _ => TypeAndQualifiers::declared(
                        self.infer_subscript_type_expression_no_store(subscript, slice, value_ty),
                    ),
                }
            }

            // Fallback to `infer_type_expression_no_store` for everything else
            type_expr => {
                TypeAndQualifiers::declared(self.infer_type_expression_no_store(type_expr))
            }
        };

        self.store_expression_type(annotation, annotation_ty.inner_type());
        self.store_qualifiers(annotation, annotation_ty.qualifiers());

        annotation_ty
    }

    /// Infer the type of a string annotation expression.
    fn infer_string_annotation_expression(
        &mut self,
        string: &ast::ExprStringLiteral,
    ) -> TypeAndQualifiers<'db> {
        match parse_string_annotation(&self.context, self.inference_flags, string) {
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
