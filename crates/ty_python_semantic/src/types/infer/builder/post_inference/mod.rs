//! A home for deferred checks that must be done after the `TypeInferenceBuilder` has done an initial
//! inference pass over the whole scope.

use ruff_python_ast as ast;
use ty_python_core::definition::Definition;

use crate::types::call::{CallArguments, CallError};
use crate::types::context::InferContext;
use crate::types::infer::infer_definition_types;

pub(super) mod dynamic_class;
pub(super) mod final_variable;
pub(super) mod function;
pub(super) mod overloaded_function;
pub(super) mod pep_613_alias;
pub(super) mod static_class;
pub(super) mod type_param_validation;
pub(super) mod typed_dict;
pub(super) mod typeguard;

pub(super) fn check_decorator_calls<'db>(
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
    for decorator in decorators.iter().rev() {
        let Some(input_ty) = inference.deferred_decorator_input_type(&decorator.expression) else {
            continue;
        };
        let decorator_ty = inference.expression_type(&decorator.expression);
        let arguments = CallArguments::positional([input_ty]);
        if let Err(CallError(_, bindings)) = decorator_ty.try_call(db, env, &arguments) {
            bindings.report_diagnostics(context, decorator.into());
        }
    }
}
