use std::convert::Infallible;

use ruff_db::diagnostic::Annotation;
use ruff_python_ast as ast;
use ty_python_core::definition::{Definition, DefinitionKind};

use super::super::TypeInferenceBuilder;
use crate::Db;
use crate::types::diagnostic::{INVALID_INIT_TYPE_VARIABLE, UNBOUND_TYPE_VARIABLE};
use crate::types::infer::InferenceFlags;
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, KnownInstanceType, Type, TypeVarKind, binding_type};

#[cfg(test)]
mod tests;

pub(in crate::types::infer) struct TypeVariableScopeFacts;

pub(super) struct OrdinaryTypeVariableScopeEffects<'db> {
    pub(super) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVariableScopeEffects)]
    pub(in crate::types::infer) trait TypeVariableScopeEffects<'db, 'ast> {
        type Error;

        #[operation(checkpoint)]
        async fn dispatch(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn bound_typevar(&self, typevar: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn kind(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(local)]
        async fn in_init_receiver(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn bound_owner(&self, typevar: BoundTypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(local)]
        async fn binding_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(local)]
        async fn in_type_alias(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn definition_is_annotated_assignment(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn definition_is_class(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn check_unbound(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_init_receiver(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, typevar: BoundTypeVarInstance<'db>, owner: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_alias_capture(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, typevar: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_unbound(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, typevar: TypeVarInstance<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl TypeVariableScopeFacts {
        fn is_self(&self, kind: TypeVarKind) -> bool {
            matches!(kind, TypeVarKind::TypingSelf)
        }

        fn different_binding<'db>(&self, owner: Definition<'db>, current: Option<Definition<'db>>) -> bool {
            Some(owner) != current
        }

        fn unknown<'db>(&self) -> Type<'db> {
            Type::unknown()
        }
    }

    /// Check whether a type variable can be used in the current type expression.
    ///
    /// Unbound variables fall back to `Unknown`. Bound variables retain their type so that an
    /// invalid scope does not also make `Callable[P, R]` or `tuple[*Ts]` appear malformed.
    #[synchronous(check_type_variable_scope_sync)]
    #[capabilities(effects = TypeVariableScopeEffects, facts = TypeVariableScopeFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn check_type_variable_scope_with<'db, 'ast, E: TypeVariableScopeEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
        facts: TypeVariableScopeFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.dispatch().await?;
        if let Type::TypeVar(typevar) = ty {
            let variable = effects.bound_typevar(typevar).await?;
            let kind = effects.kind(variable).await?;
            if !facts.is_self(kind)
                && effects.in_init_receiver(builder).await?
                && let Some(owner) = effects.bound_owner(typevar).await?
                && facts.different_binding(owner, effects.binding_definition(builder).await?)
            {
                effects.report_init_receiver(builder, expression, typevar, owner).await?;
            }
        }

        // Legacy aliases introduce independent type parameters. PEP 695 aliases can instead
        // capture their enclosing class's parameters.
        if let Type::TypeVar(typevar) = ty
            && effects.in_type_alias(builder).await?
            && let Some(definition) = effects.binding_definition(builder).await?
            && effects.definition_is_annotated_assignment(definition).await?
            && let Some(owner) = effects.bound_owner(typevar).await?
            && effects.definition_is_class(owner).await?
        {
            effects.report_alias_capture(builder, expression, typevar).await?;
            return Ok(ty);
        }

        if !effects.check_unbound(builder).await? {
            return Ok(ty);
        }
        if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = ty {
            effects.report_unbound(builder, expression, typevar).await?;
            Ok(facts.unknown())
        } else {
            Ok(ty)
        }
    }
}

impl<'db, 'ast> SynchronousTypeVariableScopeEffects<'db, 'ast>
    for OrdinaryTypeVariableScopeEffects<'db>
{
    type Error = Infallible;

    fn dispatch(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn bound_typevar(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(typevar.typevar(self.db))
    }

    fn kind(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarKind, Infallible> {
        Ok(typevar.kind(self.db))
    }

    fn in_init_receiver(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder
            .inference_flags()
            .contains(InferenceFlags::IN_INIT_RECEIVER_ANNOTATION))
    }

    fn bound_owner(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(typevar.binding_context(self.db).definition())
    }

    fn binding_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(builder.typevar_binding_context)
    }

    fn in_type_alias(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(builder
            .inference_flags()
            .contains(InferenceFlags::IN_TYPE_ALIAS))
    }

    fn definition_is_annotated_assignment(
        &self,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(matches!(
            definition.kind(self.db),
            DefinitionKind::AnnotatedAssignment(_)
        ))
    }

    fn definition_is_class(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(matches!(definition.kind(self.db), DefinitionKind::Class(_)))
    }

    fn check_unbound(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(builder
            .inference_flags()
            .contains(InferenceFlags::CHECK_UNBOUND_TYPEVARS))
    }

    fn report_init_receiver(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        typevar: BoundTypeVarInstance<'db>,
        owner: Definition<'db>,
    ) -> Result<(), Infallible> {
        let db = self.db;
        if let Some(builder) = builder
            .context
            .report_lint(&INVALID_INIT_TYPE_VARIABLE, expression)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "First parameter of `__init__` cannot use type variable `{}` from an outer scope",
                typevar.name(db)
            ));
            diagnostic.set_concise_message(format_args!(
                "First parameter of `__init__` cannot use type variable `{}` from an outer scope",
                typevar.name(db)
            ));
            diagnostic.set_primary_annotation_message(format_args!(
                "`{}` used in the first parameter's annotation here",
                typevar.name(db)
            ));
            let owner_span = match binding_type(db, owner) {
                Type::ClassLiteral(class) => Some(class.header_span(db)),
                Type::FunctionLiteral(function) => Some(function.spans(db).signature),
                _ => None,
            };
            if let Some(owner_span) = owner_span {
                diagnostic.annotate(Annotation::secondary(owner_span).message(format_args!(
                    "`{}` is bound to this enclosing scope",
                    typevar.name(db)
                )));
            }
            diagnostic.info(
                "Using type variables from an outer scope can make the constructed type ambiguous",
            );
            diagnostic.help(
                "Use a type variable scoped to `__init__`, or omit the first parameter's annotation",
            );
            diagnostic
                .info("See https://typing.python.org/en/latest/spec/constructors.html#init-method");
        }
        Ok(())
    }

    fn report_alias_capture(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        builder.report_invalid_type_expression(
            expression,
            format_args!(
                "Type alias cannot capture class-scoped type variable `{}`",
                typevar.name(self.db)
            ),
        );
        Ok(())
    }

    fn report_unbound(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        typevar: TypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        if let Some(builder) = builder
            .context
            .report_lint(&UNBOUND_TYPE_VARIABLE, expression)
        {
            builder.into_diagnostic(format_args!(
                "Type variable `{name}` is not bound to any outer generic context",
                name = typevar.name(self.db)
            ));
        }
        Ok(())
    }
}
