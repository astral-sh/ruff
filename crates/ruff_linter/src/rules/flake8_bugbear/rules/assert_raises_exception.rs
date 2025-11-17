use std::fmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Arguments, Expr, WithItem};
use ruff_text_size::Ranged;

use crate::Violation;
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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.83")]
pub(crate) struct AssertRaisesException {
    exception: ExceptionKind,
}

impl Violation for AssertRaisesException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssertRaisesException { exception } = self;
        format!("Do not assert blind exception: `{exception}`")
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

fn detect_blind_exception(
    semantic: &ruff_python_semantic::SemanticModel<'_>,
    func: &Expr,
    arguments: &Arguments,
) -> Option<ExceptionKind> {
    let is_assert_raises = matches!(
        func,
        &Expr::Attribute(ast::ExprAttribute { ref attr, .. }) if attr.as_str() == "assertRaises"
    );

    let is_pytest_raises = semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "raises"]));

    if !(is_assert_raises || is_pytest_raises) {
        return None;
    }

    if is_pytest_raises {
        if arguments.find_keyword("match").is_some() {
            return None;
        }

        if arguments
            .find_positional(1)
            .is_some_and(|arg| matches!(arg, Expr::StringLiteral(_) | Expr::BytesLiteral(_)))
        {
            return None;
        }
    }

    let exception_argument_name = if is_pytest_raises {
        "expected_exception"
    } else {
        "exception"
    };

    let exception_expr = arguments.find_argument_value(exception_argument_name, 0)?;
    let builtin_symbol = semantic.resolve_builtin_symbol(exception_expr)?;

    match builtin_symbol {
        "Exception" => Some(ExceptionKind::Exception),
        "BaseException" => Some(ExceptionKind::BaseException),
        _ => None,
    }
}

/// B017
pub(crate) fn assert_raises_exception(checker: &Checker, items: &[WithItem]) {
    for item in items {
        let Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
            node_index: _,
        }) = &item.context_expr
        else {
            continue;
        };

        if item.optional_vars.is_some() {
            continue;
        }

        if let Some(exception) =
            detect_blind_exception(checker.semantic(), func.as_ref(), arguments)
        {
            checker.report_diagnostic(AssertRaisesException { exception }, item.range());
        }
    }
}

/// B017 (call form)
pub(crate) fn assert_raises_exception_call(
    checker: &Checker,
    ast::ExprCall {
        func,
        arguments,
        range,
        node_index: _,
    }: &ast::ExprCall,
) {
    let semantic = checker.semantic();

    if arguments.args.len() < 2 && arguments.find_argument("func", 1).is_none() {
        return;
    }

    if let Some(exception) = detect_blind_exception(semantic, func.as_ref(), arguments) {
        checker.report_diagnostic(AssertRaisesException { exception }, *range);
    }
}
