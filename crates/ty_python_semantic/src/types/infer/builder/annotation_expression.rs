use ruff_python_ast as ast;

use super::{DeferredExpressionState, TypeInferenceBuilder};
use crate::place::TypeOrigin;
use crate::types::diagnostic::{INVALID_TYPE_FORM, report_invalid_arguments_to_annotated};
use crate::types::string_annotation::{
    BYTE_STRING_TYPE_ANNOTATION, FSTRING_TYPE_ANNOTATION, parse_string_annotation,
};
use crate::types::{
    KnownClass, SpecialFormType, Type, TypeAndQualifiers, TypeContext, TypeQualifiers, todo_type,
};

/// Annotation expressions.
impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer the type of an annotation expression with the given [`DeferredExpressionState`].
    pub(super) fn infer_annotation_expression(
        &mut self,
        annotation: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> TypeAndQualifiers<'db> {
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, deferred_state);
        let annotation_ty = self.infer_annotation_expression_impl(annotation);
        self.deferred_state = previous_deferred_state;
        annotation_ty
    }

    /// Similar to [`infer_annotation_expression`], but accepts an optional annotation expression
    /// and returns [`None`] if the annotation is [`None`].
    ///
    /// [`infer_annotation_expression`]: TypeInferenceBuilder::infer_annotation_expression
    pub(super) fn infer_optional_annotation_expression(
        &mut self,
        annotation: Option<&ast::Expr>,
        deferred_state: DeferredExpressionState,
    ) -> Option<TypeAndQualifiers<'db>> {
        annotation.map(|expr| self.infer_annotation_expression(expr, deferred_state))
    }

    /// Implementation of [`infer_annotation_expression`].
    ///
    /// [`infer_annotation_expression`]: TypeInferenceBuilder::infer_annotation_expression
    fn infer_annotation_expression_impl(
        &mut self,
        annotation: &ast::Expr,
    ) -> TypeAndQualifiers<'db> {
        fn infer_name_or_attribute<'db>(
            ty: Type<'db>,
            annotation: &ast::Expr,
            builder: &TypeInferenceBuilder<'db, '_>,
        ) -> TypeAndQualifiers<'db> {
            match ty {
                Type::SpecialForm(SpecialFormType::ClassVar) => TypeAndQualifiers::new(
                    Type::unknown(),
                    TypeOrigin::Declared,
                    TypeQualifiers::CLASS_VAR,
                ),
                Type::SpecialForm(SpecialFormType::Final) => TypeAndQualifiers::new(
                    Type::unknown(),
                    TypeOrigin::Declared,
                    TypeQualifiers::FINAL,
                ),
                Type::SpecialForm(SpecialFormType::Required) => TypeAndQualifiers::new(
                    Type::unknown(),
                    TypeOrigin::Declared,
                    TypeQualifiers::REQUIRED,
                ),
                Type::SpecialForm(SpecialFormType::NotRequired) => TypeAndQualifiers::new(
                    Type::unknown(),
                    TypeOrigin::Declared,
                    TypeQualifiers::NOT_REQUIRED,
                ),
                Type::SpecialForm(SpecialFormType::ReadOnly) => TypeAndQualifiers::new(
                    Type::unknown(),
                    TypeOrigin::Declared,
                    TypeQualifiers::READ_ONLY,
                ),
                Type::ClassLiteral(class) if class.is_known(builder.db(), KnownClass::InitVar) => {
                    if let Some(builder) =
                        builder.context.report_lint(&INVALID_TYPE_FORM, annotation)
                    {
                        builder
                            .into_diagnostic("`InitVar` may not be used without a type argument");
                    }
                    TypeAndQualifiers::new(
                        Type::unknown(),
                        TypeOrigin::Declared,
                        TypeQualifiers::INIT_VAR,
                    )
                }
                _ => TypeAndQualifiers::declared(
                    ty.in_type_expression(
                        builder.db(),
                        builder.scope(),
                        builder.typevar_binding_context,
                    )
                    .unwrap_or_else(|error| {
                        error.into_fallback_type(
                            &builder.context,
                            annotation,
                            builder.is_reachable(annotation),
                        )
                    }),
                ),
            }
        }

        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-annotation_expression
        let annotation_ty = match annotation {
            // String annotations: https://typing.python.org/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(string) => self.infer_string_annotation_expression(string),

            // Annotation expressions also get special handling for `*args` and `**kwargs`.
            ast::Expr::Starred(starred) => {
                TypeAndQualifiers::declared(self.infer_starred_expression(starred))
            }

            ast::Expr::BytesLiteral(bytes) => {
                if let Some(builder) = self
                    .context
                    .report_lint(&BYTE_STRING_TYPE_ANNOTATION, bytes)
                {
                    builder.into_diagnostic("Type expressions cannot use bytes literal");
                }
                TypeAndQualifiers::declared(Type::unknown())
            }

            ast::Expr::FString(fstring) => {
                if let Some(builder) = self.context.report_lint(&FSTRING_TYPE_ANNOTATION, fstring) {
                    builder.into_diagnostic("Type expressions cannot use f-strings");
                }
                self.infer_fstring_expression(fstring);
                TypeAndQualifiers::declared(Type::unknown())
            }

            ast::Expr::Attribute(attribute) => match attribute.ctx {
                ast::ExprContext::Load => infer_name_or_attribute(
                    self.infer_attribute_expression(attribute),
                    annotation,
                    self,
                ),
                ast::ExprContext::Invalid => TypeAndQualifiers::declared(Type::unknown()),
                ast::ExprContext::Store | ast::ExprContext::Del => TypeAndQualifiers::declared(
                    todo_type!("Attribute expression annotation in Store/Del context"),
                ),
            },

            ast::Expr::Name(name) => match name.ctx {
                ast::ExprContext::Load => {
                    infer_name_or_attribute(self.infer_name_expression(name), annotation, self)
                }
                ast::ExprContext::Invalid => TypeAndQualifiers::declared(Type::unknown()),
                ast::ExprContext::Store | ast::ExprContext::Del => TypeAndQualifiers::declared(
                    todo_type!("Name expression annotation in Store/Del context"),
                ),
            },

            ast::Expr::Subscript(subscript @ ast::ExprSubscript { value, slice, .. }) => {
                let value_ty = self.infer_expression(value, TypeContext::default());

                let slice = &**slice;

                match value_ty {
                    Type::SpecialForm(SpecialFormType::Annotated) => {
                        // This branch is similar to the corresponding branch in `infer_parameterized_special_form_type_expression`, but
                        // `Annotated[…]` can appear both in annotation expressions and in type expressions, and needs to be handled slightly
                        // differently in each case (calling either `infer_type_expression_*` or `infer_annotation_expression_*`).
                        if let ast::Expr::Tuple(ast::ExprTuple {
                            elts: arguments, ..
                        }) = slice
                        {
                            if arguments.len() < 2 {
                                report_invalid_arguments_to_annotated(&self.context, subscript);
                            }

                            if let [inner_annotation, metadata @ ..] = &arguments[..] {
                                for element in metadata {
                                    self.infer_expression(element, TypeContext::default());
                                }

                                let inner_annotation_ty =
                                    self.infer_annotation_expression_impl(inner_annotation);

                                self.store_expression_type(slice, inner_annotation_ty.inner_type());
                                inner_annotation_ty
                            } else {
                                for argument in arguments {
                                    self.infer_expression(argument, TypeContext::default());
                                }
                                self.store_expression_type(slice, Type::unknown());
                                TypeAndQualifiers::declared(Type::unknown())
                            }
                        } else {
                            report_invalid_arguments_to_annotated(&self.context, subscript);
                            self.infer_annotation_expression_impl(slice)
                        }
                    }
                    Type::SpecialForm(
                        type_qualifier @ (SpecialFormType::ClassVar
                        | SpecialFormType::Final
                        | SpecialFormType::Required
                        | SpecialFormType::NotRequired
                        | SpecialFormType::ReadOnly),
                    ) => {
                        let arguments = if let ast::Expr::Tuple(tuple) = slice {
                            &*tuple.elts
                        } else {
                            std::slice::from_ref(slice)
                        };
                        let num_arguments = arguments.len();
                        let type_and_qualifiers = if num_arguments == 1 {
                            let mut type_and_qualifiers =
                                self.infer_annotation_expression_impl(slice);

                            match type_qualifier {
                                SpecialFormType::ClassVar => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::CLASS_VAR);
                                }
                                SpecialFormType::Final => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::FINAL);
                                }
                                SpecialFormType::Required => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::REQUIRED);
                                }
                                SpecialFormType::NotRequired => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::NOT_REQUIRED);
                                }
                                SpecialFormType::ReadOnly => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::READ_ONLY);
                                }
                                _ => unreachable!(),
                            }
                            type_and_qualifiers
                        } else {
                            for element in arguments {
                                self.infer_annotation_expression_impl(element);
                            }
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Type qualifier `{type_qualifier}` expected exactly 1 argument, \
                                    got {num_arguments}",
                                ));
                            }
                            TypeAndQualifiers::declared(Type::unknown())
                        };
                        if slice.is_tuple_expr() {
                            self.store_expression_type(slice, type_and_qualifiers.inner_type());
                        }
                        type_and_qualifiers
                    }
                    Type::ClassLiteral(class) if class.is_known(self.db(), KnownClass::InitVar) => {
                        let arguments = if let ast::Expr::Tuple(tuple) = slice {
                            &*tuple.elts
                        } else {
                            std::slice::from_ref(slice)
                        };
                        let num_arguments = arguments.len();
                        let type_and_qualifiers = if num_arguments == 1 {
                            let mut type_and_qualifiers =
                                self.infer_annotation_expression_impl(slice);
                            type_and_qualifiers.add_qualifier(TypeQualifiers::INIT_VAR);
                            type_and_qualifiers
                        } else {
                            for element in arguments {
                                self.infer_annotation_expression_impl(element);
                            }
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Type qualifier `InitVar` expected exactly 1 argument, \
                                    got {num_arguments}",
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
                        self.infer_subscript_type_expression_no_store(subscript, slice, value_ty),
                    ),
                }
            }

            // All other annotation expressions are (possibly) valid type expressions, so handle
            // them there instead.
            type_expr => {
                TypeAndQualifiers::declared(self.infer_type_expression_no_store(type_expr))
            }
        };

        self.store_expression_type(annotation, annotation_ty.inner_type());

        annotation_ty
    }

    /// Infer the type of a string annotation expression.
    fn infer_string_annotation_expression(
        &mut self,
        string: &ast::ExprStringLiteral,
    ) -> TypeAndQualifiers<'db> {
        match parse_string_annotation(&self.context, string) {
            Some(parsed) => {
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
