use ruff_python_ast::{self as ast};

use super::TypeInferenceBuilder;
use crate::types::diagnostic::INVALID_TYPE_FORM;
use crate::types::{CycleDetector, DynamicType, KnownClass, Type, TypeContext, TypeFormType};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// In a `TypeForm` context, keep the ordinary value interpretation if it is
    /// already acceptable.
    ///
    /// For example, `value` in `x: TypeForm[str] = value` might already have type
    /// `TypeForm[str]`, and `1` in `x: TypeForm[str] | int = 1` satisfies the
    /// ordinary `int` arm. In those cases, use ordinary expression inference.
    /// Otherwise, reinterpret the expression as a type expression, as in
    /// `x: TypeForm[int | str] = int | str`, and wrap it in `TypeForm[...]`.
    pub(super) fn infer_type_form_contextual_expression(
        &mut self,
        expression: &ast::Expr,
        target: Type<'db>,
    ) -> Option<Type<'db>> {
        let non_type_form_fallback = match target.resolve_type_alias(self.db()) {
            Type::TypeForm(_) => None,
            Type::Union(union)
                if union.elements(self.db()).iter().any(|element| {
                    matches!(element.resolve_type_alias(self.db()), Type::TypeForm(_))
                }) =>
            {
                Some(target.filter_union(self.db(), |element| {
                    !matches!(element.resolve_type_alias(self.db()), Type::TypeForm(_))
                }))
            }
            _ => return None,
        };

        // Suppress contextual `TypeForm` evaluation only if ordinary inference already
        // produces a type-form value or satisfies the non-`TypeForm` arm of the union.
        let value_ty = self
            .speculate_without_diagnostics()
            .infer_maybe_standalone_expression(expression, TypeContext::default());
        if matches!(value_ty.resolve_type_alias(self.db()), Type::Never)
            || self.contains_type_form_value(expression, value_ty)
            || non_type_form_fallback
                .is_some_and(|alternative| value_ty.is_assignable_to(self.db(), alternative))
        {
            return None;
        }

        // If interpreting the root as a type expression yields no usable type,
        // try ordinary contextual inference. This lets existing bidirectional
        // inference propagate `TypeForm` through conditionals, calls, and `await`.
        if self
            .speculate_without_diagnostics()
            .infer_type_expression_no_store(expression)
            .is_unknown()
        {
            let contextual_ty = self
                .speculate_without_diagnostics()
                .infer_value_expression_impl(expression, TypeContext::new(Some(target)));
            // TODO: Remove this exception once `Unpack` produces a precise type instead of a
            // dynamic placeholder in ordinary expression inference.
            if contextual_ty.is_assignable_to(self.db(), target)
                && contextual_ty != Type::Dynamic(DynamicType::TodoUnpack)
            {
                return None;
            }
        }

        Some(TypeFormType::from_type_expression(
            self.db(),
            self.infer_type_expression_no_store(expression),
        ))
    }

    fn contains_type_form_value(&self, expression: &ast::Expr, ty: Type<'db>) -> bool {
        struct ContainsTypeFormValue;
        type ContainsTypeFormValueVisitor<'db> =
            CycleDetector<'db, ContainsTypeFormValue, Type<'db>, bool, 3>;

        fn imp<'db>(
            builder: &TypeInferenceBuilder<'db, '_>,
            expression: &ast::Expr,
            ty: Type<'db>,
            visitor: &ContainsTypeFormValueVisitor<'db>,
        ) -> bool {
            match ty {
                Type::TypeForm(_) | Type::SubclassOf(_) => true,
                // A bare class object is valid type-expression syntax and should still be
                // interpreted as a `TypeForm`. Preserve its ordinary value type only when
                // it was produced by an expression that is not itself a type expression.
                Type::ClassLiteral(_) => builder
                    .speculate_without_diagnostics()
                    .infer_type_expression_no_store(expression)
                    .is_unknown(),
                Type::NominalInstance(instance)
                    if instance.has_known_class(builder.db(), KnownClass::Type) =>
                {
                    true
                }
                Type::Union(union) => union
                    .elements(builder.db())
                    .iter()
                    .any(|element| imp(builder, expression, *element, visitor)),
                Type::Intersection(intersection) => intersection
                    .iter_positive(builder.db())
                    .any(|element| imp(builder, expression, element, visitor)),
                Type::TypeAlias(alias) => visitor.visit(builder.db(), ty, || {
                    imp(builder, expression, alias.value_type(builder.db()), visitor)
                }),
                Type::TypeVar(typevar) => visitor.visit(builder.db(), ty, || {
                    typevar
                        .typevar(builder.db())
                        .bound_or_constraints(builder.db())
                        .is_some_and(|bound_or_constraints| {
                            imp(
                                builder,
                                expression,
                                bound_or_constraints.as_type(builder.db()),
                                visitor,
                            )
                        })
                }),
                _ => false,
            }
        }

        imp(
            self,
            expression,
            ty,
            &ContainsTypeFormValueVisitor::default(),
        )
    }

    pub(super) fn infer_type_form_call_expression(
        &mut self,
        call_expression: &ast::ExprCall,
    ) -> Type<'db> {
        let arguments = &call_expression.arguments;

        if let [argument] = &*arguments.args
            && arguments.keywords.is_empty()
            && !matches!(argument, ast::Expr::Starred(_))
        {
            return TypeFormType::from_type_expression(
                self.db(),
                self.infer_type_expression(argument),
            );
        }

        for argument in &*arguments.args {
            self.infer_expression(argument, TypeContext::default());
        }
        for keyword in &*arguments.keywords {
            self.infer_expression(&keyword.value, TypeContext::default());
        }

        if let Some(builder) = self
            .context
            .report_lint(&INVALID_TYPE_FORM, call_expression)
        {
            builder.into_diagnostic(
                "`TypeForm()` expects exactly one positional-only type-expression argument",
            );
        }

        Type::unknown()
    }
}
