use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::function_type::is_stub;
use ruff_python_semantic::analyze::terminal::Terminal;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__len__` implementations that return values that are not non-negative
/// integers.
///
/// ## Why is this bad?
/// The `__len__` method should return a non-negative integer. Returning a different
/// value may cause unexpected behavior.
///
/// Note: `bool` is a subclass of `int`, so it's technically valid for `__len__` to
/// return `True` or `False`. However, for consistency with other rules, Ruff will
/// still emit a diagnostic when `__len__` returns a `bool`.
///
/// ## Example
/// ```python
/// class Foo:
///     def __len__(self):
///         return "2"
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __len__(self):
///         return 2
/// ```
///
/// ## References
/// - [Python documentation: The `__len__` method](https://docs.python.org/3/reference/datamodel.html#object.__len__)
#[violation]
pub struct InvalidLengthReturnType;

impl Violation for InvalidLengthReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__len__` does not return a non-negative integer")
    }
}

/// E0303
pub(crate) fn invalid_length_return(checker: &mut Checker, function_def: &ast::StmtFunctionDef) {
    if function_def.name.as_str() != "__len__" {
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
        checker.diagnostics.push(Diagnostic::new(
            InvalidLengthReturnType,
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
            if is_negative_integer(value)
                || !matches!(
                    ResolvedPythonType::from(value),
                    ResolvedPythonType::Unknown
                        | ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
                )
            {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidLengthReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidLengthReturnType, stmt.range()));
        }
    }
}

/// Returns `true` if the given expression is a negative integer.
fn is_negative_integer(value: &Expr) -> bool {
    matches!(
        value,
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            ..
        })
    )
}
