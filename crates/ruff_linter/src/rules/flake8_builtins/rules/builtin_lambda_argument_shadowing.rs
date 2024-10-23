use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprLambda;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_builtins::helpers::shadows_builtin;

/// ## What it does
/// Checks for lambda arguments that use the same names as Python builtins.
///
/// ## Why is this bad?
/// Reusing a builtin name for the name of a lambda argument increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the variable for the
/// builtin and vice versa.
///
/// Builtins can be marked as exceptions to this rule via the
/// [`lint.flake8-builtins.builtins-ignorelist`] configuration option.
///
/// ## Options
/// - `lint.flake8-builtins.builtins-ignorelist`
#[violation]
pub struct BuiltinLambdaArgumentShadowing {
    name: String,
}

impl Violation for BuiltinLambdaArgumentShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinLambdaArgumentShadowing { name } = self;
        format!("Lambda argument `{name}` is shadowing a Python builtin")
    }
}

/// A006
pub(crate) fn builtin_lambda_argument_shadowing(checker: &mut Checker, lambda: &ExprLambda) {
    let Some(parameters) = lambda.parameters.as_ref() else {
        return;
    };
    for param in parameters.iter_non_variadic_params() {
        let name = &param.parameter.name;
        if shadows_builtin(
            name.as_ref(),
            checker.source_type,
            &checker.settings.flake8_builtins.builtins_ignorelist,
            checker.settings.target_version,
        ) {
            checker.diagnostics.push(Diagnostic::new(
                BuiltinLambdaArgumentShadowing {
                    name: name.to_string(),
                },
                name.range(),
            ));
        }
    }
}
