use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::helpers::is_empty_f_string;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// This fix is marked as unsafe if it removes comments or an unused `sep` keyword argument
/// that is not known to be a valid separator. Removing such arguments may change the
/// program's behavior by skipping their evaluation or hiding a `TypeError`.
///
/// ## References
/// - [Python documentation: `print`](https://docs.python.org/3/library/functions.html#print)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Complexity)]
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

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
pub(crate) fn print_empty_string(checker: &Checker, call: flat::View<'_, ast::ExprCall>) {
    if !checker.semantic().match_builtin_expr(call.func(), "print") {
        return;
    }

    match call.arguments().args().as_array() {
        // Ex) `print("")` or `print("", sep="\t")`
        Some([arg]) if is_empty_string(arg) => {
            let reason = if call.arguments().find_keyword("sep").is_some() {
                Reason::Both
            } else {
                Reason::EmptyArgument
            };

            let mut diagnostic =
                checker.report_diagnostic(PrintEmptyString { reason }, call.range());

            diagnostic.set_fix(
                EmptyStringFix::from_call(
                    call,
                    Separator::Remove,
                    checker.generator(),
                    checker.comment_ranges(),
                )
                .into_fix(),
            );
        }

        Some([arg]) if arg.is_starred_expr() => {
            // If there's a starred argument, we can't remove the empty string.
        }

        // Ex) `print(sep="\t")` or `print(obj, sep="\t")`
        _ if call.arguments().args().len() <= 1 => {
            // If there's a `sep` argument, remove it, regardless of what it is.
            if call.arguments().find_keyword("sep").is_some() {
                let mut diagnostic = checker.report_diagnostic(
                    PrintEmptyString {
                        reason: Reason::UselessSeparator,
                    },
                    call.range(),
                );

                diagnostic.set_fix(
                    EmptyStringFix::from_call(
                        call,
                        Separator::Remove,
                        checker.generator(),
                        checker.comment_ranges(),
                    )
                    .into_fix(),
                );
            }
        }

        // Ex) `print("foo", "", "bar", sep="")`
        _ => {
            // Ignore `**kwargs`.
            let has_kwargs = call
                .arguments()
                .keywords()
                .iter()
                .any(|keyword| keyword.arg().is_none());
            if has_kwargs {
                return;
            }

            // Require an empty `sep` argument.
            let empty_separator = call
                .arguments()
                .find_keyword("sep")
                .is_some_and(|keyword| is_empty_string(keyword.value()));
            if !empty_separator {
                return;
            }

            // Count the number of empty and non-empty arguments.
            let empty_arguments = call
                .arguments()
                .args()
                .iter()
                .filter(|arg| is_empty_string(*arg))
                .count();
            if empty_arguments == 0 {
                return;
            }

            // If removing the arguments would leave us with one or fewer, then we can remove the
            // separator too.
            let separator = if call.arguments().args().len() - empty_arguments > 1
                || call
                    .arguments()
                    .args()
                    .iter()
                    .any(flat::View::is_starred_expr)
            {
                Separator::Retain
            } else {
                Separator::Remove
            };

            let mut diagnostic = checker.report_diagnostic(
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
                EmptyStringFix::from_call(
                    call,
                    separator,
                    checker.generator(),
                    checker.comment_ranges(),
                )
                .into_fix(),
            );
        }
    }
}

/// Check if an expression is a constant empty string.
fn is_empty_string(expr: flat::View<'_, Expr>) -> bool {
    match expr.classify() {
        flat::ExprKind::StringLiteral(expr_string_literal) => {
            let value = expr_string_literal.value();

            value.is_empty()
        }
        flat::ExprKind::FString(f_string) => is_empty_f_string(f_string),
        _ => false,
    }
}

fn is_known_valid_separator(expr: flat::View<'_, Expr>) -> bool {
    expr.is_string_literal_expr() || expr.is_none_literal_expr()
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
        call: flat::View<'_, ast::ExprCall>,
        separator: Separator,
        generator: Generator,
        comment_ranges: &CommentRanges,
    ) -> Self {
        let range = call.range();
        let mut applicability = Applicability::Safe;

        if comment_ranges.intersects(range) {
            applicability = Applicability::Unsafe;
        }

        // Remove all empty string positional arguments.
        let args = call
            .arguments()
            .args()
            .iter()
            .filter(|arg| !is_empty_string(*arg))
            .map(flat::View::to_owned_node)
            .collect();

        // Remove the `sep` keyword argument if it exists.
        let keywords = call
            .arguments()
            .keywords()
            .iter()
            .filter(|keyword| {
                if separator != Separator::Remove {
                    return true;
                }
                let Some(arg) = keyword.arg() else {
                    return true;
                };

                if arg.as_str() != "sep" {
                    return true;
                }

                if !is_known_valid_separator(keyword.value()) {
                    applicability = Applicability::Unsafe;
                }

                false
            })
            .map(flat::View::to_owned_node)
            .collect();
        let replacement = ast::ExprCall {
            range_start: call.start(),
            node_index: ast::NodeIndex::NONE,
            func: Box::new(call.func().to_owned_node()),
            arguments: ast::Arguments {
                range: call.arguments().range(),
                node_index: ast::NodeIndex::NONE,
                args,
                keywords,
            },
        };
        let contents = generator.expr(&replacement.into());

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
