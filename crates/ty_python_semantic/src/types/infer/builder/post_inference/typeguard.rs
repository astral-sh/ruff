use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::{
    Type, TypeIsType,
    context::InferContext,
    diagnostic::INVALID_TYPE_GUARD_DEFINITION,
    function::{FunctionType, OverloadLiteral},
    signatures::Signature,
};

/// Check that all type guard function definitions have at least one positional parameter
/// (in addition to `self`/`cls` for methods), and for `TypeIs`, that the narrowed type is
/// assignable to the declared type of that parameter.
pub(crate) fn check_type_guard_definition<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    node: &ast::StmtFunctionDef,
) {
    match check_type_guard_definition_sync(context, ty, node, &OrdinaryTypeGuardEffects) {
        Ok(()) => {}
        Err(error) => match error {},
    }
}

struct OrdinaryTypeGuardEffects;

shared_semantic_family! {
    #[synchronous(SynchronousTypeGuardEffects)]
    pub(in crate::types::infer::builder) trait TypeGuardEffects<'db> {
        type Error;
        #[operation(source)]
        async fn last_definition(&self, context: &InferContext<'db, '_>, function: FunctionType<'db>) -> Result<OverloadLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn signature(&self, context: &InferContext<'db, '_>, overload: OverloadLiteral<'db>) -> Result<Signature<'db>, Self::Error>;
        #[operation(source)]
        async fn type_is_return_type(&self, context: &InferContext<'db, '_>, type_is: TypeIsType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn check_guard(&self, context: &InferContext<'db, '_>, overload: OverloadLiteral<'db>, signature: &Signature<'db>, node: &ast::StmtFunctionDef, type_guard_form_name: &'static str, narrowed_type: Option<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn retire_signature(&self, signature: Signature<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_type_guard_definition_sync)]
    #[capabilities(effects = TypeGuardEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_type_guard_definition_with<'db, E: TypeGuardEffects<'db>>(
        context: &InferContext<'db, '_>,
        ty: Type<'db>,
        node: &ast::StmtFunctionDef,
        effects: &E,
    ) -> Result<(), E::Error> {
        let Type::FunctionLiteral(function) = ty else {
            return Ok(());
        };

        let overload = effects.last_definition(context, function).await?;
        let signature = effects.signature(context, overload).await?;
        let (type_guard_form_name, narrowed_type) = match signature.return_ty {
            Type::TypeIs(type_is) => ("TypeIs", Some(effects.type_is_return_type(context, type_is).await?)),
            Type::TypeGuard(_) => ("TypeGuard", None),
            _ => return effects.retire_signature(signature).await,
        };

        effects.check_guard(context, overload, &signature, node, type_guard_form_name, narrowed_type).await?;
        effects.retire_signature(signature).await
    }
}

impl<'db> SynchronousTypeGuardEffects<'db> for OrdinaryTypeGuardEffects {
    type Error = Infallible;

    fn last_definition(
        &self,
        context: &InferContext<'db, '_>,
        function: FunctionType<'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error> {
        Ok(function.literal(context.db()).last_definition)
    }

    fn signature(
        &self,
        context: &InferContext<'db, '_>,
        overload: OverloadLiteral<'db>,
    ) -> Result<Signature<'db>, Self::Error> {
        Ok(overload.signature(context.db()))
    }

    fn type_is_return_type(
        &self,
        context: &InferContext<'db, '_>,
        type_is: TypeIsType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(type_is.return_type(context.db()))
    }

    fn check_guard(
        &self,
        context: &InferContext<'db, '_>,
        overload: OverloadLiteral<'db>,
        signature: &Signature<'db>,
        node: &ast::StmtFunctionDef,
        type_guard_form_name: &'static str,
        narrowed_type: Option<Type<'db>>,
    ) -> Result<(), Self::Error> {
        check_guard_parameters(
            context,
            overload,
            signature,
            node,
            type_guard_form_name,
            narrowed_type,
        );
        Ok(())
    }

    fn retire_signature(&self, signature: Signature<'db>) -> Result<(), Self::Error> {
        drop(signature);
        Ok(())
    }
}

fn check_guard_parameters<'db>(
    context: &InferContext<'db, '_>,
    overload: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    node: &ast::StmtFunctionDef,
    type_guard_form_name: &'static str,
    narrowed_type: Option<Type<'db>>,
) {
    let db = context.db();
    let env = context.program_environment();

    // The return type annotation must exist since we matched `TypeIs`/`TypeGuard`.
    let Some(returns_expr) = node.returns.as_deref() else {
        return;
    };

    // Check if this is a non-static method (first parameter is implicit `self`/`cls`).
    let has_implicit_receiver = overload.has_implicit_receiver(db);

    // Find the first positional parameter to narrow (skip implicit `self`/`cls`).
    let positional_params: Vec<_> = signature.parameters().positional().collect();
    let first_narrowed_param_index = usize::from(has_implicit_receiver);
    let first_narrowed_param = positional_params.get(first_narrowed_param_index);

    let Some(first_narrowed_param) = first_narrowed_param else {
        if let Some(builder) = context.report_lint(&INVALID_TYPE_GUARD_DEFINITION, returns_expr) {
            builder.into_diagnostic(format_args!(
                "`{type_guard_form_name}` function must have a parameter to narrow"
            ));
        }
        return;
    };

    // For `TypeIs`, check that the narrowed type is assignable to the parameter type.
    if let Some(narrowed_ty) = narrowed_type {
        let param_ty = first_narrowed_param.annotated_type();
        if !narrowed_ty.is_assignable_to(db, env, param_ty)
            && let Some(builder) = context.report_lint(&INVALID_TYPE_GUARD_DEFINITION, returns_expr)
        {
            builder.into_diagnostic(format_args!(
                "Narrowed type `{narrowed}` is not assignable \
                    to the declared parameter type `{param}`",
                narrowed = narrowed_ty.display(db, env),
                param = param_ty.display(db, env)
            ));
        }
    }
}
