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
/// Checks for `__hash__` implementations that return non-integer values.
///
/// ## Why is this bad?
/// The `__hash__` method should return an integer. Returning a different
/// type may cause unexpected behavior.
///
/// Note: `bool` is a subclass of `int`, so it's technically valid for `__hash__` to
/// return `True` or `False`. However, for consistency with other rules, Ruff will
/// still emit a diagnostic when `__hash__` returns a `bool`.
///
/// ## Example
/// ```python
/// class Foo:
///     def __hash__(self):
///         return "2"
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __hash__(self):
///         return 2
/// ```
///
/// ## References
/// - [Python documentation: The `__hash__` method](https://docs.python.org/3/reference/datamodel.html#object.__hash__)
#[derive(ViolationMetadata)]
pub(crate) struct InvalidHashReturnType;

impl Violation for InvalidHashReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__hash__` does not return an integer".to_string()
    }
}

/// E0309
pub(crate) fn invalid_hash_return(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    if function_def.name.as_str() != "__hash__" {
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
            InvalidHashReturnType,
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
                    | ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
            ) {
                checker.report_diagnostic(Diagnostic::new(InvalidHashReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker.report_diagnostic(Diagnostic::new(InvalidHashReturnType, stmt.range()));
        }
    }
}
