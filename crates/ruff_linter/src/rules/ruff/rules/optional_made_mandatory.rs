use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::{analyze, Binding, BindingKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Force an optional keyword argument to be mandatory on a specified fully
/// qualified function or method name.
///
/// Projects may want to ensure that certain arguments are always included on
/// certain functions, even if the functions come from third-party libraries
/// that cannot be easily changed, and where wrapping them internally is
/// inconvenient.
///
/// Some edge cases:
/// 1. The argument is supplied as a positional argument.
/// 2. The argument is supplied as part of a star argument.
/// 3. Limitations on type checking/inference.
///
/// (1) and (2) it not safe to provide any fix edits.
///
/// ## Options
/// - `lint.ruff.optional-made-mandatory`
#[derive(ViolationMetadata)]
pub(crate) struct OptionalMadeMandatory;

impl Violation for OptionalMadeMandatory {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Optional argument has been specified to be mandatory".to_string()
    }
}

fn handle_binding_as_argument_to_function(
    checker: &Checker,
    binding: &Binding,
    attr: &ast::Identifier,
) -> Option<String> {
    // If the binding is a function argument, try to get a type annotation and resolve
    // the qualified name for the relevant annotation type.
    let Some(ast::Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. })) =
        binding.statement(checker.semantic())
    else {
        return None;
    };

    let annotation = analyze::typing::find_parameter(parameters, binding)?
        .parameter
        .annotation
        .as_deref()?;
    checker
        .semantic()
        .resolve_qualified_name(annotation)
        .map(|annotation_qualified_name| format!("{}.{}", annotation_qualified_name, attr.as_str()))
}

fn resolve_full_name(checker: &Checker, call: &ast::ExprCall) -> Option<String> {
    match call.func.as_ref() {
        ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) => {
            let ast::Expr::Name(name) = value.as_ref() else {
                return None;
            };

            let binding_id = checker.semantic().resolve_name(name)?;
            let binding = checker.semantic().binding(binding_id);
            match analyze::typing::find_binding_value(binding, checker.semantic()) {
                Some(ast::Expr::Call(inner_call)) => {
                    // Attempt to handle method calls on a class that has been instantiated,
                    // Example case:
                    // df = pd.DataFrame(); df.merge(...)
                    checker
                        .semantic()
                        .resolve_qualified_name(inner_call.func.as_ref())
                        .map(|qualified_name| format!("{}.{}", qualified_name, attr.as_str()))
                }
                _ => {
                    if matches!(binding.kind, BindingKind::Argument) {
                        // Example case:
                        // def f(df: pd.DataFrame):
                        //     df.merge(...)
                        return handle_binding_as_argument_to_function(checker, binding, attr);
                    }
                    // Just try to use the qualified name of the original call directly
                    // Example case: pd.merge(df)
                    checker
                        .semantic()
                        .resolve_qualified_name(call.func.as_ref())
                        .map(|qualified_name| qualified_name.to_string())
                }
            }
        }
        ast::Expr::Name(_) => {
            // Just try resolving the qualified name for the function in this case for now
            // Example case:
            // from pandas import merge; merge(...)
            checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())
                .map(|qualified_name| qualified_name.to_string())
        }
        _ => None,
    }
}

/// RUF059
pub(crate) fn optional_made_mandatory(checker: &Checker, call: &ast::ExprCall) {
    let optional_made_mandatory = &checker.settings.ruff.optional_made_mandatory;
    if optional_made_mandatory.is_empty() {
        return;
    }
    let Some(full_name) = resolve_full_name(checker, call) else {
        return;
    };
    if let Some(mandatory_args) = optional_made_mandatory.get(&full_name) {
        for mandatory_arg in &mandatory_args.args {
            // Assumes arguments are supplied as keyword arguments; does not handle args passed as positional.
            if !&call.arguments.keywords.iter().any(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|arg| arg == mandatory_arg.as_str())
            }) {
                // kw not explicitly assigned at call site.
                checker.report_diagnostic(Diagnostic::new(OptionalMadeMandatory, call.range()));
            }
        }
    }
}
