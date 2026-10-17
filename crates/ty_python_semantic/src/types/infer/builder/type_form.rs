use crate::Db;
use crate::types::recursive::TypeQueryVisit;
use crate::types::recursive::{TypeInspectionKey, TypeQueryKey};
use crate::types::{CycleKind, TypeQuery, TypeQueryContext};
use ruff_python_ast::{self as ast};

use super::TypeInferenceBuilder;
use crate::types::diagnostic::INVALID_TYPE_FORM;
use crate::types::recursive::{TypeFormContext, TypeInspector};
use crate::types::{KnownClass, Type, TypeContext, TypeFormType};

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
        let db = self.db();
        let env = self.program_environment();
        let non_type_form_fallback = TypeInspector::inspect_root(
            db,
            env,
            TypeInspectionKey::InferTypeFormContextualExpression(
                self.scope(),
                ty_python_core::ExpressionNodeKey::from(expression),
            ),
            target,
            |_, _| None,
            |_, target| target.type_form_context_fallback(db),
        )?;

        // Suppress contextual `TypeForm` evaluation only if ordinary inference already
        // produces a type-form value or satisfies the non-`TypeForm` arm of the union.
        let value_ty = self
            .speculate_without_diagnostics()
            .infer_maybe_standalone_expression(expression, TypeContext::default());
        if value_ty.has_outer_type(db, env, |ty| ty.is_never())
            || self.contains_type_form_value(expression, value_ty)
            || matches!(non_type_form_fallback, TypeFormContext::WithValueAlternative(alternative)
                if value_ty.is_assignable_to(db, env, alternative))
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
                .infer_value_expression_impl(expression, TypeContext::declared(Some(target)));
            if contextual_ty.is_assignable_to(db, env, target) {
                return None;
            }
        }

        Some(TypeFormType::from_type_expression(
            self.db(),
            self.infer_type_expression_no_store(expression),
        ))
    }

    fn contains_type_form_value(&self, expression: &ast::Expr, ty: Type<'db>) -> bool {
        TypeQueryContext::with_root(
            self.program_environment(),
            ContainsTypeFormValue {
                builder: self,
                expression,
            },
            |context| context.visit(self.db(), ty),
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

struct ContainsTypeFormValue<'a, 'db, 'ast> {
    builder: &'a TypeInferenceBuilder<'db, 'ast>,
    expression: &'a ast::Expr,
}

impl<'db> TypeQuery<'db> for ContainsTypeFormValue<'_, 'db, '_> {
    fn recursion_key(&self) -> TypeQueryKey<'db> {
        TypeQueryKey::ContainsTypeFormValue(self.builder.scope(), self.expression.into())
    }

    type Output = bool;

    fn on_cycle(
        &self,
        _context: &TypeQueryVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        (): (),
    ) -> bool {
        false
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        context: &TypeQueryVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        (): (),
    ) -> bool {
        let env = context.environment();
        match ty {
            Type::TypeForm(_) | Type::SubclassOf(_) => true,
            // Class objects written as type expressions should still be interpreted as TypeForm.
            Type::ClassLiteral(_) => self
                .builder
                .speculate_without_diagnostics()
                .infer_type_expression_no_store(self.expression)
                .is_unknown(),
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                true
            }
            Type::Union(union) => union.elements(db).iter().any(|ty| context.visit(db, *ty)),
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .any(|ty| context.visit(db, ty)),
            Type::TypeVar(typevar) => typevar
                .typevar(db)
                .bound_or_constraints(db, env)
                .is_some_and(|bound| context.visit(db, bound.as_type(db, env))),
            _ => false,
        }
    }
}
