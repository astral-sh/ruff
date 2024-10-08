use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, WithItem};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `assertRaises` and `pytest.raises` context managers that catch
/// `Exception` or `BaseException`.
///
/// ## Why is this bad?
/// These forms catch every `Exception`, which can lead to tests passing even
/// if, e.g., the code under consideration raises a `SyntaxError` or
/// `IndentationError`.
///
/// Either assert for a more specific exception (builtin or custom), or use
/// `assertRaisesRegex` or `pytest.raises(..., match=<REGEX>)` respectively.
///
/// ## Example
/// ```python
/// self.assertRaises(Exception, foo)
/// ```
///
/// Use instead:
/// ```python
/// self.assertRaises(SomeSpecificException, foo)
/// ```
#[violation]
pub struct AssertRaisesException {
    assertion: AssertionKind,
    exception: ExceptionKind,
}

impl Violation for AssertRaisesException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssertRaisesException {
            assertion,
            exception,
        } = self;
        format!("`{assertion}({exception})` should be considered evil")
    }
}

#[derive(Debug, PartialEq, Eq)]
enum AssertionKind {
    AssertRaises,
    PytestRaises,
}

impl fmt::Display for AssertionKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AssertionKind::AssertRaises => fmt.write_str("assertRaises"),
            AssertionKind::PytestRaises => fmt.write_str("pytest.raises"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ExceptionKind {
    BaseException,
    Exception,
}

impl fmt::Display for ExceptionKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ExceptionKind::BaseException => fmt.write_str("BaseException"),
            ExceptionKind::Exception => fmt.write_str("Exception"),
        }
    }
}

/// B017
pub(crate) fn assert_raises_exception(checker: &mut Checker, items: &[WithItem]) {
    for item in items {
        let Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
        }) = &item.context_expr
        else {
            continue;
        };

        if item.optional_vars.is_some() {
            continue;
        }

        let [arg] = &*arguments.args else {
            continue;
        };

        let semantic = checker.semantic();

        let Some(builtin_symbol) = semantic.resolve_builtin_symbol(arg) else {
            continue;
        };

        let exception = match builtin_symbol {
            "Exception" => ExceptionKind::Exception,
            "BaseException" => ExceptionKind::BaseException,
            _ => continue,
        };

        let assertion = if matches!(func.as_ref(), Expr::Attribute(ast::ExprAttribute { attr, .. }) if attr == "assertRaises")
        {
            AssertionKind::AssertRaises
        } else if semantic
            .resolve_qualified_name(func)
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "raises"]))
            && arguments.find_keyword("match").is_none()
        {
            AssertionKind::PytestRaises
        } else {
            continue;
        };

        checker.diagnostics.push(Diagnostic::new(
            AssertRaisesException {
                assertion,
                exception,
            },
            item.range(),
        ));
    }
}
