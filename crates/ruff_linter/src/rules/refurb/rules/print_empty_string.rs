use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_empty_f_string;
use ruff_python_ast::{self as ast, Builder, Expr, ExprKind, build};
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
pub(crate) fn print_empty_string(checker: &Checker, call: &ast::ExprCallFields<'_>) {
    if !checker.semantic().match_builtin_expr(call.func, "print") {
        return;
    }

    match call.arguments.args().as_array() {
        // Ex) `print("")` or `print("", sep="\t")`
        Some([arg]) if is_empty_string(arg) => {
            let reason = if call.arguments.find_keyword("sep").is_some() {
                Reason::Both
            } else {
                Reason::EmptyArgument
            };

            let mut diagnostic = checker.report_diagnostic(PrintEmptyString { reason }, call.range);

            diagnostic.set_fix(
                EmptyStringFix::from_call(
                    call.node,
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
        _ if call.arguments.args().len() <= 1 => {
            // If there's a `sep` argument, remove it, regardless of what it is.
            if call.arguments.find_keyword("sep").is_some() {
                let mut diagnostic = checker.report_diagnostic(
                    PrintEmptyString {
                        reason: Reason::UselessSeparator,
                    },
                    call.range,
                );

                diagnostic.set_fix(
                    EmptyStringFix::from_call(
                        call.node,
                        Separator::Remove,
                        checker.generator(),
                        checker.comment_ranges(),
                    )
                    .into_fix(),
                );
            }
        }
        if !sep.is_some_and(is_empty_string) {
            return;
        }

        // Ex) `print("foo", "", "bar", sep="")`
        _ => {
            // Ignore `**kwargs`.
            let has_kwargs = call
                .arguments
                .keywords()
                .iter()
                .any(ruff_python_ast::Keyword::is_variadic);
            if has_kwargs {
                return;
            }

            // Require an empty `sep` argument.
            let empty_separator = call
                .arguments
                .find_keyword("sep")
                .is_some_and(|keyword| is_empty_string(keyword.value()));
            if !empty_separator {
                return;
            }

            // Count the number of empty and non-empty arguments.
            let empty_arguments = call
                .arguments
                .args()
                .iter()
                .filter(|arg| is_empty_string(*arg))
                .count();
            if empty_arguments == 0 {
                return;
            }

            // If removing the arguments would leave us with one or fewer, then we can remove the
            // separator too.
            let separator = if call.arguments.args().len() - empty_arguments > 1
                || call.arguments.args().iter().any(ast::Expr::is_starred_expr)
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
                call.range,
            );

            diagnostic.set_fix(
                EmptyStringFix::from_call(
                    call.node,
                    separator,
                    checker.generator(),
                    checker.comment_ranges(),
                )
                .into_fix(),
            );
        }
        if !has_empty {
            return;
        }
        if remaining > 1 || starred {
            (Reason::EmptyArgument, Separator::Retain)
        } else {
            (Reason::Both, Separator::Remove)
        }
    } else {
        if first.is_some_and(Expr::is_starred_expr) {
            return;
        }
        let has_sep = call.arguments.find_keyword("sep").is_some();
        let reason = match (first.is_some_and(is_empty_string), has_sep) {
            (true, true) => Reason::Both,
            (true, false) => Reason::EmptyArgument,
            (false, true) => Reason::UselessSeparator,
            (false, false) => return,
        };
        (reason, Separator::Remove)
    };

    let mut diagnostic = checker.report_diagnostic(PrintEmptyString { reason }, call.range());
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

/// Check if an expression is a constant empty string.
fn is_empty_string(expr: Expr<'_>) -> bool {
    match expr.classify() {
        ExprKind::StringLiteral(expr_string_literal) => {
            let value = expr_string_literal.value();

            value.is_empty()
        }
        ExprKind::FString(f_string) => is_empty_f_string(f_string),
        _ => false,
    }
}

fn is_known_valid_separator(expr: Expr<'_>) -> bool {
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
        call: ast::ExprCall<'_>,
        separator: Separator,
        generator: Generator,
        comment_ranges: &CommentRanges,
    ) -> Self {
        let range = call.range();
        let mut applicability = Applicability::Safe;

        if comment_ranges.intersects(range) {
            applicability = Applicability::Unsafe;
        }

        let mut builder = Builder::default();
        let kept = call
            .arguments()
            .items()
            .iter()
            .filter(|argument| match argument {
                ast::ArgOrKeyword::Arg(arg) => !is_empty_string(*arg),
                ast::ArgOrKeyword::Keyword(keyword)
                    if separator == Separator::Remove
                        && keyword.arg().is_some_and(|name| name == "sep") =>
                {
                    if !is_known_valid_separator(keyword.value()) {
                        applicability = Applicability::Unsafe;
                    }
                    false
                }
                ast::ArgOrKeyword::Keyword(_) => true,
            });
        let items = builder.map_list(kept, |b, argument| match argument {
            ast::ArgOrKeyword::Arg(arg) => ast::ArgumentId::Positional(b.copy_node(arg)),
            ast::ArgOrKeyword::Keyword(kw) => ast::ArgumentId::Keyword(b.copy_node(kw)),
        });
        let arguments = builder.add(build::Arguments {
            range: call.arguments().range(),
            items,
        });
        let func = builder.copy_node(call.func());
        let replacement = builder.add(build::ExprCall {
            range,
            func,
            arguments,
        });
        let contents = generator.expr(builder.get(replacement).into());
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
