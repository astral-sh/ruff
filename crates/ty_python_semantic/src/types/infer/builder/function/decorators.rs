use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::class::ClassLiteral;
use crate::types::class::static_literal::decorators::DecoratorExpressionCursor;
use crate::types::function::{
    FunctionDecoratorKind, FunctionDecorators, FunctionType, KnownFunction,
};
use crate::types::infer::InferenceFlags;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::{KnownClass, Type, TypeContext};

pub(in crate::types::infer) struct FunctionDecoratorFacts;
pub(in crate::types::infer::builder) struct OrdinaryFunctionDecoratorEffects;

#[derive(Clone, Copy)]
pub(in crate::types::infer) struct FunctionDecoratorClassification {
    pub(in crate::types::infer) known_decorators: FunctionDecorators,
    pub(in crate::types::infer) has_unknown_decorators: bool,
}

impl Default for FunctionDecoratorClassification {
    fn default() -> Self {
        Self {
            known_decorators: FunctionDecorators::empty(),
            has_unknown_decorators: true,
        }
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousFunctionDecoratorEffects)]
    pub(in crate::types::infer) trait FunctionDecoratorEffects<'db, 'ast> {
        type Error;

        #[operation(local)]
        async fn expression_cursor<'source>(&self, function: &'source ast::StmtFunctionDef) -> Result<DecoratorExpressionCursor<'source>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_expression<'source>(&self, cursor: &mut DecoratorExpressionCursor<'source>) -> Result<Option<&'source ast::Expr>, Self::Error>;
        #[operation(source)]
        async fn expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn expression_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn known_function(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<Option<KnownFunction>, Self::Error>;
        #[operation(source)]
        async fn known_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(local)]
        async fn suppress_diagnostics(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl FunctionDecoratorFacts {
        fn non_function(&self) -> FunctionDecoratorClassification { FunctionDecoratorClassification::default() }
        fn undecorated(&self) -> FunctionDecoratorClassification {
            FunctionDecoratorClassification { known_decorators: FunctionDecorators::empty(), has_unknown_decorators: false }
        }
        fn function<'db>(&self, ty: Type<'db>) -> Option<FunctionType<'db>> { ty.as_function_literal() }
        fn class<'db>(&self, ty: Type<'db>) -> Option<ClassLiteral<'db>> { ty.as_class_literal() }
        fn no_type_check(&self, known: Option<KnownFunction>) -> bool { known == Some(KnownFunction::NoTypeCheck) }
        fn known_function(&self, known: Option<KnownFunction>) -> FunctionDecoratorKind { FunctionDecoratorKind::from_known_function(known) }
        fn known_class(&self, known: Option<KnownClass>) -> FunctionDecoratorKind { FunctionDecoratorKind::from_known_class(known) }
        fn unknown(&self) -> FunctionDecoratorKind { FunctionDecoratorKind::Unknown }
        fn include(&self, classification: FunctionDecoratorClassification, decorator: FunctionDecoratorKind) -> FunctionDecoratorClassification {
            FunctionDecoratorClassification {
                known_decorators: classification.known_decorators | decorator.flags(),
                has_unknown_decorators: classification.has_unknown_decorators || decorator.is_unknown(),
            }
        }
    }

    #[synchronous(function_decorators_sync)]
    #[capabilities(effects = FunctionDecoratorEffects, facts = FunctionDecoratorFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn function_decorators_with<'db, 'ast, E: FunctionDecoratorEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, function: Option<&ast::StmtFunctionDef>, facts: FunctionDecoratorFacts, effects: &E,
    ) -> Result<FunctionDecoratorClassification, E::Error> {
        let Some(function) = function else { return Ok(facts.non_function()); };
        let mut cursor = effects.expression_cursor(function).await?;
        #[cursor_loop]
        while let Some(expression) = effects.next_expression(&mut cursor).await? {
            let ty = effects.expression(builder, expression).await?;
            if let Some(function) = facts.function(ty)
                && facts.no_type_check(effects.known_function(builder, function).await?)
            {
                // Match `infer_function_definition`: suppress diagnostics that follow
                // `@no_type_check`, including later decorators.
                effects.suppress_diagnostics(builder).await?;
            }
        }

        // Classification reads the completed expression region, including the builder's
        // cycle-recovery fallback for missing entries.
        #[passive_state]
        let mut classification = facts.undecorated();
        let mut cursor = effects.expression_cursor(function).await?;
        #[cursor_loop]
        while let Some(expression) = effects.next_expression(&mut cursor).await? {
            let ty = effects.expression_type(builder, expression).await?;
            let decorator = if let Some(function) = facts.function(ty) {
                facts.known_function(effects.known_function(builder, function).await?)
            } else if let Some(class) = facts.class(ty) {
                facts.known_class(effects.known_class(builder, class).await?)
            } else {
                facts.unknown()
            };
            classification = facts.include(classification, decorator);
        }
        Ok(classification)
    }
}

impl<'db, 'ast> SynchronousFunctionDecoratorEffects<'db, 'ast>
    for OrdinaryFunctionDecoratorEffects
{
    type Error = Infallible;

    fn expression_cursor<'source>(
        &self,
        function: &'source ast::StmtFunctionDef,
    ) -> Result<DecoratorExpressionCursor<'source>, Infallible> {
        Ok(DecoratorExpressionCursor::new(&function.decorator_list))
    }

    fn next_expression<'source>(
        &self,
        cursor: &mut DecoratorExpressionCursor<'source>,
    ) -> Result<Option<&'source ast::Expr>, Infallible> {
        Ok(cursor.next())
    }

    fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_expression(expression, TypeContext::default()))
    }

    fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.expression_type(expression))
    }

    fn known_function(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> Result<Option<KnownFunction>, Infallible> {
        Ok(function.known(builder.db()))
    }

    fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(builder.db()))
    }

    fn suppress_diagnostics(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Infallible> {
        builder.context.inference_flags |= InferenceFlags::IN_NO_TYPE_CHECK;
        Ok(())
    }
}
