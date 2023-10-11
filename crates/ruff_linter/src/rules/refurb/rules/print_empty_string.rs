use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr};
use ruff_python_codegen::Generator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
    reason: Reason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    /// Ex) `print("")`
    EmptyArgument,
    /// Ex) `print("foo", sep="\t")`
    UselessSeparator,
    /// Ex) `print("", sep="\t")`
    Both,
}

impl Violation for PrintEmptyString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PrintEmptyString { reason } = self;
        match reason {
            Reason::EmptyArgument => format!("Unnecessary empty string passed to `print`"),
            Reason::UselessSeparator => format!("Unnecessary separator passed to `print`"),
            Reason::Both => format!("Unnecessary empty string and separator passed to `print`"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let PrintEmptyString { reason } = self;
        match reason {
            Reason::EmptyArgument => Some("Remove empty string".to_string()),
            Reason::UselessSeparator => Some("Remove separator".to_string()),
            Reason::Both => Some("Remove empty string and separator".to_string()),
        }
    }
}

/// FURB105
pub(crate) fn print_empty_string(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .as_ref()
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "print"]))
    {
        return;
    }

    match &call.arguments.args.as_slice() {
        // Ex) `print("")` or `print("", sep="\t")`
        [arg] if is_empty_string(arg) => {
            let reason = if call.arguments.find_keyword("sep").is_some() {
                Reason::Both
            } else {
                Reason::EmptyArgument
            };

            let mut diagnostic = Diagnostic::new(PrintEmptyString { reason }, call.range());

            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                generate_suggestion(call, Separator::Remove, checker.generator()),
                call.start(),
                call.end(),
            )));

            checker.diagnostics.push(diagnostic);
        }

        // Ex) `print(sep="\t")` or `print(obj, sep="\t")`
        [] | [_] => {
            // If there's a `sep` argument, remove it, regardless of what it is.
            if call.arguments.find_keyword("sep").is_some() {
                let mut diagnostic = Diagnostic::new(
                    PrintEmptyString {
                        reason: Reason::UselessSeparator,
                    },
                    call.range(),
                );

                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                    generate_suggestion(call, Separator::Remove, checker.generator()),
                    call.start(),
                    call.end(),
                )));

                checker.diagnostics.push(diagnostic);
            }
        }

        // Ex) `print("foo", "", "bar", sep="")`
        _ => {
            // Ignore `**kwargs`.
            let has_kwargs = call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none());
            if has_kwargs {
                return;
            }

            // Require an empty `sep` argument.
            let empty_separator = call
                .arguments
                .find_keyword("sep")
                .map_or(false, |keyword| is_empty_string(&keyword.value));
            if !empty_separator {
                return;
            }

            // Count the number of empty and non-empty arguments.
            let empty_arguments = call
                .arguments
                .args
                .iter()
                .filter(|arg| is_empty_string(arg))
                .count();
            if empty_arguments == 0 {
                return;
            }

            // If removing the arguments would leave us with one or fewer, then we can remove the
            // separator too.
            let separator = if call.arguments.args.len() - empty_arguments > 1
                || call.arguments.args.iter().any(Expr::is_starred_expr)
            {
                Separator::Retain
            } else {
                Separator::Remove
            };

            let mut diagnostic = Diagnostic::new(
                PrintEmptyString {
                    reason: if separator == Separator::Retain {
                        Reason::EmptyArgument
                    } else {
                        Reason::Both
                    },
                },
                call.range(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                generate_suggestion(call, separator, checker.generator()),
                call.start(),
                call.end(),
            )));

            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Check if an expression is a constant empty string.
fn is_empty_string(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(value),
            ..
        }) if value.is_empty()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    Remove,
    Retain,
}

/// Generate a suggestion to remove the empty string positional argument and
/// the `sep` keyword argument, if it exists.
fn generate_suggestion(call: &ast::ExprCall, separator: Separator, generator: Generator) -> String {
    let mut call = call.clone();

    // Remove all empty string positional arguments.
    call.arguments.args.retain(|arg| !is_empty_string(arg));

    // Remove the `sep` keyword argument if it exists.
    if separator == Separator::Remove {
        call.arguments.keywords.retain(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(true, |arg| arg.as_str() != "sep")
        });
    }

    generator.expr(&call.into())
}
