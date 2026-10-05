use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::Definition;

use crate::types::Type;
use crate::types::call::{CallArguments, CallError};
use crate::types::context::InferContext;
use crate::types::infer::{DefinitionInference, infer_definition_types};

pub(in crate::types::infer::builder) type DecoratorCursor<'a> =
    std::iter::Rev<std::slice::Iter<'a, ast::Decorator>>;

pub(crate) fn check_decorator_calls<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
    decorators: &[ast::Decorator],
) {
    match check_decorator_calls_sync(context, definition, decorators, &OrdinaryDecoratorEffects) {
        Ok(()) => {}
        Err(error) => match error {},
    }
}

struct OrdinaryDecoratorEffects;

shared_semantic_family! {
    #[synchronous(SynchronousDecoratorEffects)]
    pub(in crate::types::infer::builder) trait DecoratorEffects<'db> {
        type Error;
        #[operation(local)]
        async fn decorators_empty(&self, decorators: &[ast::Decorator]) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn canonical_definition(&self, context: &InferContext<'db, '_>, definition: Definition<'db>) -> Result<&'db DefinitionInference<'db>, Self::Error>;
        #[operation(local)]
        async fn decorator_cursor<'a>(&self, decorators: &'a [ast::Decorator]) -> Result<DecoratorCursor<'a>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_decorator<'a>(&self, cursor: &mut DecoratorCursor<'a>) -> Result<Option<&'a ast::Decorator>, Self::Error>;
        #[operation(local)]
        async fn deferred_input(&self, inference: &DefinitionInference<'db>, expression: &ast::Expr) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn decorator_type(&self, inference: &DefinitionInference<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn replay_call(&self, context: &InferContext<'db, '_>, decorator: &ast::Decorator, decorator_ty: Type<'db>, input_ty: Type<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_decorator_calls_sync)]
    #[capabilities(effects = DecoratorEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_decorator_calls_with<'db, E: DecoratorEffects<'db>>(
        context: &InferContext<'db, '_>,
        definition: Definition<'db>,
        decorators: &[ast::Decorator],
        effects: &E,
    ) -> Result<(), E::Error> {
        if effects.decorators_empty(decorators).await? {
            return Ok(());
        }
        let inference = effects.canonical_definition(context, definition).await?;
        let mut cursor = effects.decorator_cursor(decorators).await?;
        #[cursor_loop]
        while let Some(decorator) = effects.next_decorator(&mut cursor).await? {
            let Some(input_ty) = effects.deferred_input(inference, &decorator.expression).await? else {
                continue;
            };
            let decorator_ty = effects.decorator_type(inference, &decorator.expression).await?;
            effects.replay_call(context, decorator, decorator_ty, input_ty).await?;
        }
        Ok(())
    }
}

pub(in crate::types::infer::builder) fn decorator_cursor(
    decorators: &[ast::Decorator],
) -> DecoratorCursor<'_> {
    decorators.iter().rev()
}

pub(in crate::types::infer::builder) fn next_decorator<'a>(
    cursor: &mut DecoratorCursor<'a>,
) -> Option<&'a ast::Decorator> {
    cursor.next()
}

impl<'db> SynchronousDecoratorEffects<'db> for OrdinaryDecoratorEffects {
    type Error = Infallible;

    fn decorators_empty(&self, decorators: &[ast::Decorator]) -> Result<bool, Self::Error> {
        Ok(decorators.is_empty())
    }

    fn canonical_definition(
        &self,
        context: &InferContext<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Self::Error> {
        Ok(infer_definition_types(context.db(), definition))
    }

    fn decorator_cursor<'a>(
        &self,
        decorators: &'a [ast::Decorator],
    ) -> Result<DecoratorCursor<'a>, Self::Error> {
        Ok(decorator_cursor(decorators))
    }

    fn next_decorator<'a>(
        &self,
        cursor: &mut DecoratorCursor<'a>,
    ) -> Result<Option<&'a ast::Decorator>, Self::Error> {
        Ok(next_decorator(cursor))
    }

    fn deferred_input(
        &self,
        inference: &DefinitionInference<'db>,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(inference.deferred_decorator_input_type(expression))
    }

    fn decorator_type(
        &self,
        inference: &DefinitionInference<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(inference.expression_type(expression))
    }

    fn replay_call(
        &self,
        context: &InferContext<'db, '_>,
        decorator: &ast::Decorator,
        decorator_ty: Type<'db>,
        input_ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        let db = context.db();
        let env = context.program_environment();
        let arguments = CallArguments::positional([input_ty]);
        if let Err(CallError(_, bindings)) = decorator_ty.try_call(db, env, &arguments) {
            bindings.report_diagnostics(context, decorator.into());
        }
        Ok(())
    }
}
