use ruff_python_ast as ast;
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::call::{CallArguments, CallError};
use crate::types::context::InferContext;
use crate::types::diagnostic::report_invalid_runtime_checkable;
use crate::types::function::{FunctionDecorators, FunctionLiteral, FunctionType, KnownFunction};
use crate::types::infer::{DefinitionInference, infer_definition_types};
use crate::types::{KnownClass, Type};

pub(crate) fn check_decorator_calls<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
    decorators: &[ast::Decorator],
) {
    if decorators.is_empty() {
        return;
    }

    let db = context.db();
    let env = context.program_environment();
    let inference = infer_definition_types(db, definition);

    // Whether every decorator applied so far only sets a flag on the function (such as
    // `@overload` or `@final`), so the next decorator still receives the function itself.
    let mut receives_definition = true;

    for decorator in decorators.iter().rev() {
        let decorator_ty = inference.expression_type(&decorator.expression);
        let flags = FunctionDecorators::from_decorator_type(db, decorator_ty);
        let applied_to_definition = receives_definition;
        receives_definition &= !flags.is_empty()
            && !flags
                .intersects(FunctionDecorators::CLASSMETHOD | FunctionDecorators::STATICMETHOD);

        let input_ty = match inference.deferred_decorator_input_type(&decorator.expression) {
            Some(input_ty) => input_ty,
            // Inference applies `@classmethod` to a function definition by updating the
            // function's flags instead of calling `classmethod`, so we validate that call here.
            None if applied_to_definition
                && decorator_ty
                    .as_class_literal()
                    .is_some_and(|class| class.is_known(db, KnownClass::Classmethod))
                && let Some(input_ty) = classmethod_input_type(db, inference, definition) =>
            {
                input_ty
            }
            None => continue,
        };

        if decorator_ty
            .as_function_literal()
            .is_some_and(|function| function.is_known(db, KnownFunction::RuntimeCheckable))
            && let Some(class) = input_ty.as_class_literal()
        {
            if !class.is_protocol(db) {
                report_invalid_runtime_checkable(context, decorator, class);
            }
            continue;
        }

        let arguments = CallArguments::positional([input_ty]);
        if let Err(CallError(_, bindings)) = decorator_ty.try_call(db, env, &arguments) {
            bindings.report_diagnostics(context, decorator.into());
        }
    }
}

/// Returns the function that a `@classmethod` decorator receives when it is applied directly to
/// `definition`.
///
/// For an overloaded function, this is only the overload or implementation at `definition`, so
/// that each one is checked separately:
///
/// ```python
/// class C:
///     @overload
///     @classmethod
///     def f(cls, x: int) -> int: ...
///     @overload
///     @classmethod
///     def f(*, x: str) -> str: ...  # error: no positional parameter for the class
/// ```
fn classmethod_input_type<'db>(
    db: &'db dyn Db,
    inference: &DefinitionInference<'db>,
    definition: Definition<'db>,
) -> Option<Type<'db>> {
    let last_definition = inference
        .function_type(definition)?
        .literal(db)
        .last_definition;
    // `@no_type_check` suppresses decorator call errors, and the post-inference context does not
    // inherit that flag from the function's own inference.
    if last_definition.has_known_decorator(db, FunctionDecorators::NO_TYPE_CHECK) {
        return None;
    }
    let function = FunctionLiteral::new(db, last_definition).without_overloads();
    Some(Type::FunctionLiteral(
        FunctionType::new(db, function, None).underlying_function(db),
    ))
}
