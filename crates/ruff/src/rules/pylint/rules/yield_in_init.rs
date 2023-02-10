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
    /// Checks for `__init__` methods that are turned into generators by the
    /// inclusion of `yield` or `yield from` statements.
    ///
    /// ### Why is this bad?
    /// The `__init__` method of a class is used to initialize new objects, not
    /// create them. As such, it should not return any value. By including a
    /// yield expression in the method turns it into a generator method. On
    /// calling, it will return a generator resulting in a runtime error.
    ///
    /// ### Example
    /// ```python
    /// class InitIsGenerator:
    ///     def __init__(self, i):
    ///         yield i
    /// ```
    ///
    /// ### References
    /// * [`py-init-method-is-generator`](https://codeql.github.com/codeql-query-help/python/py-init-method-is-generator/)
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
