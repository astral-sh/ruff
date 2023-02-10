use rustpython_parser::ast::Expr;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::function_type;
use crate::ast::function_type::FunctionType;
use crate::{
    ast::types::{FunctionDef, Range, ScopeKind},
    checkers::ast::Checker,
    registry::Diagnostic,
    violation::Violation,
};

define_violation!(
    /// ### What it does
    /// Checks for `__init__` methods that turned into generators
    /// via the presence of `yield` or `yield from` statements.
    ///
    /// ### Why is this bad?
    /// Generators are not allowed in `__init__` methods.
    ///
    /// ### Example
    /// ```python
    /// class Foo:
    ///     def __init__(self):
    ///         yield 1
    /// ```
    pub struct YieldInInit;
);

impl Violation for YieldInInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__init__` method is a generator")
    }
}

/// PLE0100
pub fn yield_in_init(checker: &mut Checker, expr: &Expr) {
    let scope = checker.current_scope();
    let ScopeKind::Function(FunctionDef {
        name,
        decorator_list,
        ..
    }) = &scope.kind else {
        return;
    };

    if *name != "__init__" {
        return;
    }

    let Some(parent) = checker.current_scope_parent() else {
        return;
    };

    if !matches!(
        function_type::classify(
            checker,
            parent,
            name,
            decorator_list,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        FunctionType::Method
    ) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(YieldInInit, Range::from_located(expr)));
}
