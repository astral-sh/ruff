use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast};
use ruff_python_semantic::analyze::function_type::is_stub;
use ruff_python_semantic::analyze::terminal::Terminal;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__bool__` implementations that return a type other than `bool`.
///
/// ## Why is this bad?
/// The `__bool__` method should return a `bool` object. Returning a different
/// type may cause unexpected behavior.
///
/// ## Example
/// ```python
/// class Foo:
///     def __bool__(self):
///         return 2
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __bool__(self):
///         return True
/// ```
///
/// ## References
/// - [Python documentation: The `__bool__` method](https://docs.python.org/3/reference/datamodel.html#object.__bool__)
#[derive(ViolationMetadata)]
pub(crate) struct InvalidBoolReturnType;

impl Violation for InvalidBoolReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__bool__` does not return `bool`".to_string()
    }
}

/// PLE0304
pub(crate) fn invalid_bool_return(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    if function_def.name.as_str() != "__bool__" {
        return;
    }

    if !checker.semantic().current_scope().kind.is_class() {
        return;
    }

    if is_stub(function_def, checker.semantic()) {
        return;
    }

    // Determine the terminal behavior (i.e., implicit return, no return, etc.).
    let terminal = Terminal::from_function(function_def);

    // If every control flow path raises an exception, ignore the function.
    if terminal == Terminal::Raise {
        return;
    }

    // If there are no return statements, add a diagnostic.
    if terminal == Terminal::Implicit {
        checker.report_diagnostic(Diagnostic::new(
            InvalidBoolReturnType,
            function_def.identifier(),
        ));
        return;
    }

    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(&function_def.body);
        visitor.returns
    };

    for stmt in returns {
        if let Some(value) = stmt.value.as_deref() {
            if !matches!(
                ResolvedPythonType::from(value),
                ResolvedPythonType::Unknown
                    | ResolvedPythonType::Atom(PythonType::Number(NumberLike::Bool))
            ) {
                checker.report_diagnostic(Diagnostic::new(InvalidBoolReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker.report_diagnostic(Diagnostic::new(InvalidBoolReturnType, stmt.range()));
        }
    }
}
