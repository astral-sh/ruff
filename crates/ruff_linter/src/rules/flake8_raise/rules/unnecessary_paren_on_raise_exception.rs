use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::BindingKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary parentheses on raised exceptions.
///
/// ## Why is this bad?
/// If an exception is raised without any arguments, parentheses are not
/// required, as the `raise` statement accepts either an exception instance
/// or an exception class (which is then implicitly instantiated).
///
/// Removing the parentheses makes the code more concise.
///
/// ## Known problems
/// Parentheses can only be omitted if the exception is a class, as opposed to
/// a function call. This rule isn't always capable of distinguishing between
/// the two.
///
/// For example, if you import a function `module.get_exception` from another
/// module, and `module.get_exception` returns an exception object, this rule will
/// incorrectly mark the parentheses in `raise module.get_exception()` as
/// unnecessary.
///
/// ## Example
/// ```python
/// raise TypeError()
/// ```
///
/// Use instead:
/// ```python
/// raise TypeError
/// ```
///
/// ## References
/// - [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
#[violation]
pub struct UnnecessaryParenOnRaiseException;

impl AlwaysFixableViolation for UnnecessaryParenOnRaiseException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses on raised exception")
    }

    fn fix_title(&self) -> String {
        format!("Remove unnecessary parentheses")
    }
}

/// RSE102
pub(crate) fn unnecessary_paren_on_raise_exception(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments,
        range: _,
    }) = expr
    else {
        return;
    };

    if arguments.is_empty() {
        // `raise func()` still requires parentheses; only `raise Class()` does not.
        let exception_type = if let Some(id) = checker.semantic().lookup_attribute(func) {
            match checker.semantic().binding(id).kind {
                BindingKind::FunctionDefinition(_) => return,
                BindingKind::ClassDefinition(_) => Some(ExceptionType::Class),
                BindingKind::Builtin => Some(ExceptionType::Builtin),
                _ => None,
            }
        } else {
            None
        };

        // `ctypes.WinError()` is a function, not a class. It's part of the standard library, so
        // we might as well get it right.
        if exception_type.is_none()
            && checker
                .semantic()
                .resolve_call_path(func)
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["ctypes", "WinError"]))
        {
            return;
        }

        let mut diagnostic = Diagnostic::new(UnnecessaryParenOnRaiseException, arguments.range());

        // If the arguments are immediately followed by a `from`, insert whitespace to avoid
        // a syntax error, as in:
        // ```python
        // raise IndexError()from ZeroDivisionError
        // ```
        if checker
            .locator()
            .after(arguments.end())
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
        {
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_replacement(" ".to_string(), arguments.range()),
                if exception_type.is_some() {
                    Applicability::Safe
                } else {
                    Applicability::Unsafe
                },
            ));
        } else {
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_deletion(arguments.range()),
                if exception_type.is_some() {
                    Applicability::Safe
                } else {
                    Applicability::Unsafe
                },
            ));
        }

        checker.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, is_macro::Is)]
enum ExceptionType {
    Class,
    Builtin,
}
