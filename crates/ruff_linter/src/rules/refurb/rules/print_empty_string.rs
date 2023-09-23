use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr};
use ruff_python_codegen::Generator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `print` calls with an empty string as the only positional
/// argument.
///
/// ## Why is this bad?
/// Prefer calling `print` without any positional arguments, which is
/// equivalent and more concise.
///
/// ## Example
/// ```python
/// print("")
/// ```
///
/// Use instead:
/// ```python
/// print()
/// ```
///
/// ## References
/// - [Python documentation: `print`](https://docs.python.org/3/library/functions.html#print)
#[violation]
pub struct PrintEmptyString {
    suggestion: String,
    redundant_sep: bool,
}

impl Violation for PrintEmptyString {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PrintEmptyString {
            suggestion,
            redundant_sep,
        } = self;
        if redundant_sep == &true {
            format!(
                "Called `print` with an empty string and a redundant separator, use `{suggestion}` instead",
            )
        } else {
            format!("Called `print` with an empty string, use `{suggestion}` instead",)
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let PrintEmptyString { redundant_sep, .. } = self;
        if redundant_sep == &true {
            Some("Remove empty string positional argument and redundant separator".to_string())
        } else {
            Some("Remove empty string positional argument".to_string())
        }
    }
}

/// FURB105
pub(crate) fn print_empty_string(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .as_ref()
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "print"]))
    {
        // If the print call does not have precisely one positional argument,
        // do not trigger (more than one positional empty string argument is
        // not equivalent to no positional arguments due to the `sep` argument).
        if call.arguments.args.len() != 1 {
            return;
        }
        // Check if the one positional argument is an empty string.
        if let Expr::Constant(ast::ExprConstant {
            value: Constant::Str(s),
            ..
        }) = &call.arguments.args[0]
        {
            if s.is_empty() {
                let sep_index = call.arguments.keywords.iter().position(|keyword| {
                    keyword
                        .arg
                        .clone()
                        .is_some_and(|arg| arg.to_string() == "sep")
                });
                let suggestion = generate_suggestion(&call.clone(), sep_index, checker.generator());
                let mut diagnostic = Diagnostic::new(
                    PrintEmptyString {
                        suggestion: suggestion.clone(),
                        redundant_sep: sep_index.is_some(),
                    },
                    call.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::suggested(Edit::replacement(
                        suggestion,
                        call.start(),
                        call.end(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

fn generate_suggestion(
    call: &ast::ExprCall,
    sep_index: Option<usize>,
    generator: Generator,
) -> String {
    let mut suggestion = call.clone();
    // Remove the empty string argument.
    suggestion.arguments.args.remove(0);
    // If there is a `sep` keyword argument, remove it too (the separator is
    // not needed if there are no objects to separate) by finding its index.
    if let Some(index) = sep_index {
        suggestion.arguments.keywords.remove(index);
    }
    generator.expr(&suggestion.into())
}
