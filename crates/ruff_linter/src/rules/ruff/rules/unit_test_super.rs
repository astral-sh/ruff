use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt, StmtClassDef};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for subclasses of unittest. `TestCase` where the `setUp()`, `tearDown()`,
/// `setUpClass()`, or `tearDownClass()` methods do not call the corresponding `super()`
/// method.
///
/// ## Why is this bad?
/// Failing to call the corresponding `super()` method can lead to incomplete or
/// improper initialization and cleanup.
///
///
/// ## Example
///
/// ``` python
/// import unittest
///
/// class MyTestCase(unittest.TestCase):
///     def setUp(self):
///         self.resource = "Test resource"
/// ```
///
/// Use instead:
///
/// ``` python
/// import unittest
///
/// class MyTestCase(unittest.TestCase):
///     def setUp(self):
///         super().setUp()
///         self.resource = "Test resource"
/// ```
///
#[derive(ViolationMetadata)]
pub(crate) struct UnitTestSuper;

impl Violation for UnitTestSuper {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Call super methods for unit test methods".to_string()
    }
}

pub(crate) fn unit_test_super(checker: &mut Checker, class: &StmtClassDef) {
    let Some(Arguments { args: bases, .. }) = class.arguments.as_deref() else {
        return;
    };

    if !is_testcase_subclass(bases) {
        return;
    }

    let unit_test_methods = ["setUp", "tearDown", "setUpClass", "tearDownClass"];

    for stmt in &class.body {
        if let Stmt::FunctionDef(function_def) = stmt {
            let body = &function_def.body;
            let name = function_def.name.as_str();
            if unit_test_methods.contains(&name) && !has_super_call(body, name) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(UnitTestSuper, stmt.identifier()));
            }
        }
    }
}

fn is_testcase_subclass(bases: &[Expr]) -> bool {
    bases.iter().any(|base| match base {
        Expr::Attribute(attr) => attr.attr.as_str() == "TestCase",
        _ => false,
    })
}

fn has_super_call(body: &[Stmt], method_name: &str) -> bool {
    body.iter().any(|stmt| {
        if let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt {
            if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
                if let Expr::Attribute(ast::ExprAttribute {
                    value: attr_value,
                    attr,
                    ..
                }) = func.as_ref()
                {
                    if attr == method_name {
                        if let Expr::Call(ast::ExprCall { func, .. }) = attr_value.as_ref() {
                            if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                                return id == "super";
                            }
                        }
                    }
                }
            }
        }
        false
    })
}
