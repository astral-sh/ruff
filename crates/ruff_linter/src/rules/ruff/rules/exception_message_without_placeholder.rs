use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_python_ast::{self as ast, Expr};

use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for instantiations of exception with a static string literal message.
///
/// ## Why is this bad?
/// When raising an exception, it is important to provide context to the message
/// to help the user understand the cause of the exception.
///
/// Consider exposing the relevant local variables related to the cause of the
/// exception, to aid debugging for the user.
///
/// Alternatively, consider creating a custom exception class that targets the
/// specific error condition.
///
/// ## Example
///
/// ```python
/// from pathlib import Path
///
/// settings_file = Path(__file__) / "settings.json"
/// if not settings_file.exists():
///     raise FileNotFoundError("File not found")
/// ```
///
/// Use instead:
///
/// ```python
/// if not settings_file.exists():
///     raise FileNotFoundError(
///         f"Settings not found at '{settings_file.resolve().as_posix()}'"
///     )
/// ```
#[violation]
pub struct ExceptionMessageWithoutPlaceholder;

impl Violation for ExceptionMessageWithoutPlaceholder {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Consider adding context to the exception message by formatting message \
            with variables, or create a custom exception class"
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add context to the exception message".to_owned())
    }
}

/// RUF031
pub(crate) fn exception_message_without_placeholder(checker: &mut Checker, call: &ast::ExprCall) {
    let ast::ExprCall {
        func, arguments, ..
    } = call;

    if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
        let semantic = checker.semantic();

        // We need to check for the following conditions:
        // 1. The exception class accepts a message argument,
        // 2. The exception is a built-in exception class, not a user redefined exception class
        //    that happens to match the name of a built-in exception class,
        // 3. The exception contains exactly one argument, which is a positional argument,
        // 4. The argument is a static value.
        if exception_classes::accepts_message_args(id.as_str())
            && semantic.match_builtin_expr(func, id.as_str())
            && arguments.args.len() == 1
            && arguments.len() == 1
            && (exception_message::has_no_context(&arguments.args[0], semantic))
        {
            // This is the confirmed rule condition
            let diagnostic = Diagnostic::new(
                ExceptionMessageWithoutPlaceholder,
                arguments.args[0].range(),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Module containing helpers for checking if an exception class is applicable
/// to this rule.
mod exception_classes {
    /// The list of built-in exception classes that accept a message argument.
    ///
    /// There are a few outliers in this list, such as:
    /// - `NotImplementedError`
    ///   - This is a special case, which is expected to be raised with no runtime
    ///     context.
    /// - `UnicodeDecodeError`
    /// - `UnicodeEncodeError`
    /// - `UnicodeTranslateError`
    ///   - which requires contextual information to be passed at instantiation time.
    ///
    /// Some of the exceptions on this list technically should not be raised by the
    /// user, such as:
    /// - `BaseException`
    /// - `Exception`
    /// - `GeneratorExit`
    /// - `KeyboardInterrupt`
    /// - `SystemExit`
    /// - `SyntaxError`
    /// - `StopAsyncIteration`
    /// - `StopIteration`
    /// - `UnicodeError`
    /// but they are included in this list for completeness; the correct usage of these
    /// exceptions is outside the scope of this rule.
    const ACCEPTS_MESSAGE_ARGS: [&str; 52] = [
        "ArithmeticError",
        "AssertionError",
        "AttributeError",
        "BaseException",
        "BlockingIOError",
        "BrokenPipeError",
        "BufferError",
        "ChildProcessError",
        "ConnectionAbortedError",
        "ConnectionError",
        "ConnectionRefusedError",
        "ConnectionResetError",
        "EOFError",
        "EnvironmentError",
        "Exception",
        "FileExistsError",
        "FileNotFoundError",
        "FloatingPointError",
        "GeneratorExit",
        "IOError",
        "ImportError",
        "IndentationError",
        "IndexError",
        "InterruptedError",
        "IsADirectoryError",
        "KeyError",
        "KeyboardInterrupt",
        "LookupError",
        "MemoryError",
        "ModuleNotFoundError",
        "NameError",
        "NotADirectoryError",
        "OSError",
        "OverflowError",
        "PermissionError",
        "ProcessLookupError",
        "RecursionError",
        "ReferenceError",
        "RuntimeError",
        "StopAsyncIteration",
        "StopIteration",
        "SyntaxError",
        "SystemError",
        "SystemExit",
        "TabError",
        "TimeoutError",
        "TypeError",
        "UnboundLocalError",
        "UnicodeError",
        "ValueError",
        "WindowsError",
        "ZeroDivisionError",
    ];

    /// Returns `true` if the given exception class name accepts a message argument.
    ///
    /// Instead of exporting the above list, use this function to check if an exception
    /// name is in it.
    pub(crate) fn accepts_message_args(name: &str) -> bool {
        ACCEPTS_MESSAGE_ARGS.contains(&name)
    }
}

/// Module containing helpers for checking if an exception message is considered static
/// for this rule.
mod exception_message {
    use ruff_python_ast::{self as ast, Expr, Stmt};
    use ruff_python_semantic::SemanticModel;

    /// Returns `true` if the given expression is a static exception message that could not
    /// have any runtime contextual information, hence violating this rule.
    ///
    /// A message is considered static if it is either:
    /// - is a [literal value],
    /// - is an [f-string literal] with only literal values inside its placeholders,
    /// - is a string literal [formatted with only literal values], or
    /// - is a [variable previously assigned a static value].
    ///
    /// Some examples of static messages:
    /// ```python
    /// MY_STATIC_VARIABLE = "This is a static message"
    /// my_other_static_variable = MY_STATIC_VARIABLE
    /// raise ValueError("This is a static message")
    /// raise ValueError(f"This is a {'static':-^11} message even with an f-string")
    /// raise ValueError("This is a {kind} message".format(kind="static"))
    /// raise ValueError(my_other_static_variable)
    /// ```
    ///
    /// Some examples of dynamic messages:
    /// ```python
    /// MY_STATIC_TEMPLATE = "Your error is because of {reasons}"
    /// raise ValueError(MY_STATIC_TEMPLATE.format(reasons="reasons"))
    ///
    /// reasons = "reasons"
    /// raise ValueError(f"Your error is because of {reasons}")
    ///
    /// raise ValueError(f"Your error is because of {reasons}".format(reasons=reasons))
    ///
    /// raise ValueError("This error" if some_condition() else "That error")
    /// ```
    ///
    /// The above examples differs from the static messages in that there could be
    /// conditions that change the message at runtime. For example:
    ///
    /// ```python
    /// MY_STATIC_TEMPLATE = "Your error is because of {reasons}"
    ///
    /// if some_condition():
    ///    raise ValueError(MY_STATIC_TEMPLATE.format(reasons="reason 1"))
    /// else:
    ///    raise ValueError(MY_STATIC_TEMPLATE.format(reasons="reason 2"))
    /// ```
    ///
    /// In this case, the message being formatted is a static value, but subject to
    /// change at runtime, hence it is considered a dynamic message.
    ///
    /// [literal value]: is_literal_value
    /// [f-string literal]: is_f_string_literal
    /// [formatted with only literal values]: is_formatted_string_literal
    /// [variable previously assigned a static value]: is_static_variable
    pub(crate) fn has_no_context(expr: &Expr, semantic: &SemanticModel) -> bool {
        is_literal_value(expr)
            || is_f_string_literal(expr)
            || is_formatted_string_literal(expr)
            || is_static_variable(expr, semantic)
    }

    /// Returns `true` if the given expression is a literal value.
    ///
    /// Currently a thin wrapper around [`Expr::is_literal_expr`], for standardisation
    /// across all the functions in this module.
    fn is_literal_value(expr: &Expr) -> bool {
        expr.is_literal_expr()
    }

    /// Returns `true` if the given expression is an f-string literal with only
    /// literal values inside its placeholders.
    ///
    /// If an f-string formats itself with a variable, it is considered a dynamic
    /// message even if that variable is a static value. See examples in
    /// [`has_no_context`].
    fn is_f_string_literal(expr: &Expr) -> bool {
        if let Expr::FString(ast::ExprFString { value, .. }) = expr {
            value.iter().all(|part| match part {
                ast::FStringPart::FString(ast::FString { elements, .. }) => {
                    elements.expressions().all(
                        // Check if all the expressions in the f-string are literal values
                        // being formatted
                        |ast::FStringExpressionElement { expression, .. }| {
                            // We cannot use `has_no_context` here, because that will prevent
                            // actual contextual information from being passed to the exception.
                            is_literal_value(expression)
                        },
                    )
                }
                ast::FStringPart::Literal(_) => true,
            })
        } else {
            false
        }
    }

    /// Returns `true` if the given expression is a string literal formatted with only
    /// literal values, which is in other words, a static string literal.
    ///
    /// If an static variable formats itself with a string literal, it is considered a dynamic
    /// message even if that variable is a static value. See examples in
    /// [`has_no_context`].
    ///
    /// ## Note
    ///
    /// This function does not currently support the Python 2 "%" formatting syntax.
    pub(crate) fn is_formatted_string_literal(expr: &Expr) -> bool {
        if let Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) = expr
        {
            if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
                attr == "format"
                // We cannot use `has_no_context` here, because that will prevent string
                // templates from being used.
                && is_literal_value(value)
                && arguments.args.iter().chain(arguments.keywords.iter().map(
                    |ast::Keyword { value, .. }| value,
                )).all(
                    // We cannot use `has_no_context` here, because that will prevent
                    // actual contextual information from being passed to the exception.
                    |expr| is_literal_value(expr) || is_f_string_literal(expr)
                )
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Returns `true` if the given expression is a static variable.
    ///
    /// This function recursively checks if the variable is assigned is a static value.
    /// If the value is mutated in any ways, it is considered a dynamic message.
    ///
    /// This is typically to detect static messages being stored as Python CONSTANTS,
    /// but are used as exception messages unaltered.
    fn is_static_variable(expr: &Expr, semantic: &SemanticModel) -> bool {
        // Check if the expression is a variable
        if let Expr::Name(expr_name) = expr {
            // Check if the variable is a constant
            semantic
                .resolve_name(expr_name)
                .and_then(|binding_id| semantic.bindings.get(binding_id))
                .and_then(|binding| binding.statement(semantic))
                .map_or(false, |stmt| match stmt {
                    // We will use `has_no_context` here, because we want to detect
                    // static variables being passed around recursively. This is common
                    // with exception messages stored as Python CONSTANTS.
                    Stmt::Assign(ast::StmtAssign { value, .. }) => has_no_context(value, semantic),
                    Stmt::AnnAssign(ast::StmtAnnAssign {
                        value: Some(value), ..
                    }) => has_no_context(value, semantic),
                    _ => false,
                })
        } else {
            false
        }
    }
}
