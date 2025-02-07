use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `print` calls with unnecessary empty strings as positional
/// arguments and unnecessary `sep` keyword arguments.
///
/// ## Why is this bad?
/// Prefer calling `print` without any positional arguments, which is
/// equivalent and more concise.
///
/// Similarly, when printing one or fewer items, the `sep` keyword argument,
/// (used to define the string that separates the `print` arguments) can be
/// omitted, as it's redundant when there are no items to separate.
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
#[derive(ViolationMetadata)]
pub(crate) struct PrintEmptyString {
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
        match self.reason {
            Reason::EmptyArgument => "Unnecessary empty string passed to `print`".to_string(),
            Reason::UselessSeparator => "Unnecessary separator passed to `print`".to_string(),
            Reason::Both => "Unnecessary empty string and separator passed to `print`".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = match self.reason {
            Reason::EmptyArgument => "Remove empty string",
            Reason::UselessSeparator => "Remove separator",
            Reason::Both => "Remove empty string and separator",
        };
        Some(title.to_string())
    }
}

/// FURB105
pub(crate) fn print_empty_string(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().match_builtin_expr(&call.func, "print") {
        return;
    }

    match &*call.arguments.args {
        // Ex) `print("")` or `print("", sep="\t")`
        [arg] if is_empty_string(arg) => {
            let reason = if call.arguments.find_keyword("sep").is_some() {
                Reason::Both
            } else {
                Reason::EmptyArgument
            };

            let mut diagnostic = Diagnostic::new(PrintEmptyString { reason }, call.range());

            diagnostic.set_fix(
                EmptyStringFix::from_call(
                    call,
                    Separator::Remove,
                    checker.semantic(),
                    checker.generator(),
                )
                .into_fix(),
            );

            checker.report_diagnostic(diagnostic);
        }

        [arg] if arg.is_starred_expr() => {
            // If there's a starred argument, we can't remove the empty string.
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

                diagnostic.set_fix(
                    EmptyStringFix::from_call(
                        call,
                        Separator::Remove,
                        checker.semantic(),
                        checker.generator(),
                    )
                    .into_fix(),
                );

                checker.report_diagnostic(diagnostic);
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
                .is_some_and(|keyword| is_empty_string(&keyword.value));
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

            diagnostic.set_fix(
                EmptyStringFix::from_call(call, separator, checker.semantic(), checker.generator())
                    .into_fix(),
            );

            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Check if an expression is a constant empty string.
fn is_empty_string(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::StringLiteral(ast::ExprStringLiteral {
            value,
            ..
        }) if value.is_empty()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    Remove,
    Retain,
}

#[derive(Debug, Clone)]
struct EmptyStringFix(Fix);

impl EmptyStringFix {
    /// Generate a suggestion to remove the empty string positional argument and
    /// the `sep` keyword argument, if it exists.
    fn from_call(
        call: &ast::ExprCall,
        separator: Separator,
        semantic: &SemanticModel,
        generator: Generator,
    ) -> Self {
        let range = call.range();
        let mut call = call.clone();
        let mut applicability = Applicability::Safe;

        // Remove all empty string positional arguments.
        call.arguments.args = call
            .arguments
            .args
            .iter()
            .filter(|arg| !is_empty_string(arg))
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        // Remove the `sep` keyword argument if it exists.
        if separator == Separator::Remove {
            call.arguments.keywords = call
                .arguments
                .keywords
                .iter()
                .filter(|keyword| {
                    let Some(arg) = keyword.arg.as_ref() else {
                        return true;
                    };

                    if arg.as_str() != "sep" {
                        return true;
                    }

                    if contains_effect(&keyword.value, |id| semantic.has_builtin_binding(id)) {
                        applicability = Applicability::Unsafe;
                    }

                    false
                })
                .cloned()
                .collect::<Vec<_>>()
                .into_boxed_slice();
        }

        let contents = generator.expr(&call.into());

        Self(Fix::applicable_edit(
            Edit::range_replacement(contents, range),
            applicability,
        ))
    }

    /// Return the [`Fix`] contained in this [`EmptyStringFix`].
    fn into_fix(self) -> Fix {
        self.0
    }
}
