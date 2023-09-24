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
    separator: Option<Separator>,
}

impl Violation for PrintEmptyString {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PrintEmptyString { separator } = self;
        match separator {
            None | Some(Separator::Retain) => format!("Unnecessary empty string passed to `print`"),
            Some(Separator::Remove) => {
                format!("Unnecessary empty string passed to `print` with redundant separator")
            }
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let PrintEmptyString { separator } = self;
        match separator {
            None | Some(Separator::Retain) => Some("Remove empty string".to_string()),
            Some(Separator::Remove) => Some("Remove empty string and separator".to_string()),
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
        // Ex) `print("", sep="")` or `print("", "", **kwargs)`
        let empty_separator = call
            .arguments
            .find_keyword("sep")
            .map_or(false, |keyword| is_empty_string(&keyword.value))
            && !call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none());

        // Avoid flagging, e.g., `print("", "", sep="sep")`
        if !empty_separator && call.arguments.args.len() != 1 {
            return;
        }

        // Check if the positional arguments is are all empty strings, or if
        // there are any empty strings and the `sep` keyword argument is also
        // an empty string.
        if call.arguments.args.iter().all(is_empty_string)
            || (empty_separator && call.arguments.args.iter().any(is_empty_string))
        {
            let separator = call
                .arguments
                .keywords
                .iter()
                .any(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|arg| arg.as_str() == "sep")
                })
                .then(|| {
                    let is_starred = call.arguments.args.iter().any(Expr::is_starred_expr);
                    if is_starred {
                        return Separator::Retain;
                    }

                    let non_empty = call
                        .arguments
                        .args
                        .iter()
                        .filter(|arg| !is_empty_string(arg))
                        .count();
                    if non_empty > 1 {
                        return Separator::Retain;
                    }

                    Separator::Remove
                });

            let mut diagnostic = Diagnostic::new(PrintEmptyString { separator }, call.range());

            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::suggested(Edit::replacement(
                    generate_suggestion(call, separator, checker.generator()),
                    call.start(),
                    call.end(),
                )));
            }

            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Check if an expression is a constant empty string.
fn is_empty_string(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(s),
            ..
        }) if s.is_empty()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    Remove,
    Retain,
}

/// Generate a suggestion to remove the empty string positional argument and
/// the `sep` keyword argument, if it exists.
fn generate_suggestion(
    call: &ast::ExprCall,
    separator: Option<Separator>,
    generator: Generator,
) -> String {
    let mut call = call.clone();

    // Remove all empty string positional arguments.
    call.arguments.args.retain(|arg| !is_empty_string(arg));

    // Remove the `sep` keyword argument if it exists.
    if separator == Some(Separator::Remove) {
        call.arguments.keywords.retain(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(true, |arg| arg.as_str() != "sep")
        });
    }

    generator.expr(&call.into())
}
