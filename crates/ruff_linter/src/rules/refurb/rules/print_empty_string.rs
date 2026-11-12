use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_empty_f_string;
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
pub(crate) fn print_empty_string(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().match_builtin_expr(&call.func, "print") {
        return;
    }

    let mut positional = call.arguments.args();
    let first = positional.next();
    let (reason, separator) = if let Some(second) = positional.next() {
        // An unpacked mapping can supply the separator at runtime.
        let mut sep = None;
        for kw in call.arguments.keywords() {
            let Some(name) = &kw.arg else {
                return;
            };
            if name == "sep" && sep.is_none() {
                sep = Some(kw.value.as_ref());
            }
        }
        if !sep.is_some_and(is_empty_string) {
            return;
        }

        let mut has_empty = false;
        let mut remaining = 0;
        let mut starred = false;
        for arg in first.into_iter().chain([second]).chain(positional) {
            if is_empty_string(arg) {
                has_empty = true;
            } else {
                remaining += 1;
                starred = starred || arg.is_starred_expr();
            }
            if has_empty && (remaining > 1 || starred) {
                break;
            }
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
fn is_empty_string(expr: &Expr) -> bool {
    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.is_empty(),
        Expr::FString(f_string) => is_empty_f_string(f_string),
        _ => false,
    }
}

fn is_known_valid_separator(expr: &Expr) -> bool {
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
        call: &ast::ExprCall,
        separator: Separator,
        generator: Generator,
        comment_ranges: &CommentRanges,
    ) -> Self {
        let range = call.range();
        let mut call = call.clone();
        let mut applicability = Applicability::Safe;

        if comment_ranges.intersects(range) {
            applicability = Applicability::Unsafe;
        }

        call.arguments.items.retain(|item| match item {
            ast::Argument::Positional(arg) => !is_empty_string(arg),
            ast::Argument::Keyword(keyword)
                if separator == Separator::Remove && keyword.arg.as_deref() == Some("sep") =>
            {
                if !is_known_valid_separator(&keyword.value) {
                    applicability = Applicability::Unsafe;
                }
                false
            }
            ast::Argument::Keyword(_) => true,
        });

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
