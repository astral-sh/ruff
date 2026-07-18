use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::class::any_qualified_base_class;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for overridden `unittest` lifecycle methods (`setUp`, `tearDown`,
/// `setUpClass`, `tearDownClass`) that do not call the corresponding
/// `super()` method, when the test class inherits from a **custom**
/// `unittest.TestCase` subclass defined in the same file.
///
/// ## Why is this bad?
/// Direct subclasses of `unittest.TestCase` rarely need to call
/// `super().setUp()` (etc.), because `unittest.TestCase` itself implements
/// those methods as no-ops.
///
/// The important case is when tests inherit from a project-specific base
/// class that itself subclasses `unittest.TestCase` and provides shared
/// setup/teardown. Forgetting `super().setUp()` silently skips that shared
/// logic.
///
/// Without multifile analysis, Ruff can only reliably detect those custom
/// base classes when they are defined in the same module as the subclass.
/// Cross-module custom bases are currently false negatives by design.
///
/// ## Example
///
/// ```python
/// import unittest
///
///
/// class DatabaseTestCase(unittest.TestCase):
///     def setUp(self):
///         self.db = connect()
///
///
/// class MyTest(DatabaseTestCase):
///     def setUp(self):
///         # Missing super().setUp() — shared DB setup is skipped
///         self.fixture = load_fixture()
/// ```
///
/// Use instead:
///
/// ```python
/// import unittest
///
///
/// class DatabaseTestCase(unittest.TestCase):
///     def setUp(self):
///         self.db = connect()
///
///
/// class MyTest(DatabaseTestCase):
///     def setUp(self):
///         super().setUp()
///         self.fixture = load_fixture()
/// ```
///
/// ## References
/// - [Python documentation: Organizing test code](https://docs.python.org/3/library/unittest.html#organizing-test-code)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.23")]
pub(crate) struct UnittestMissingSuperCall {
    method_name: String,
}

impl Violation for UnittestMissingSuperCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`{}` override should call `super().{}()`",
            self.method_name, self.method_name
        )
    }
}

/// RUF077
pub(crate) fn unittest_missing_super_call(checker: &Checker, class_def: &StmtClassDef) {
    if checker.source_type.is_stub() {
        return;
    }

    if !inherits_from_custom_testcase(class_def, checker.semantic()) {
        return;
    }

    for stmt in &class_def.body {
        let Stmt::FunctionDef(function_def) = stmt else {
            continue;
        };
        let method_name = function_def.name.as_str();
        if !matches!(
            method_name,
            "setUp" | "tearDown" | "setUpClass" | "tearDownClass"
        ) {
            continue;
        }
        if has_super_call(function_def, method_name) {
            continue;
        }
        checker.report_diagnostic(
            UnittestMissingSuperCall {
                method_name: method_name.to_string(),
            },
            function_def.identifier(),
        );
    }
}

/// Return `true` when `class_def` subclasses `unittest.TestCase` via at least
/// one *immediate* base that is a custom class (also a `TestCase` subclass)
/// resolvable in the current semantic model — typically same-file.
fn inherits_from_custom_testcase(class_def: &StmtClassDef, semantic: &SemanticModel) -> bool {
    if !is_testcase_subclass(class_def, semantic) {
        return false;
    }

    class_def.bases().iter().any(|base| {
        // Direct `unittest.TestCase` bases do not require super() calls.
        if is_unittest_testcase_expr(base, semantic) {
            return false;
        }

        let Some(binding_id) = semantic.lookup_attribute(map_subscript(base)) else {
            return false;
        };
        let binding = semantic.binding(binding_id);
        let Some(scope_id) = binding.kind.as_class_definition() else {
            return false;
        };
        let Some(base_class) = semantic.scopes[*scope_id].kind.as_class() else {
            return false;
        };

        // Custom base must itself be a TestCase subclass (same-file MRO).
        is_testcase_subclass(base_class, semantic)
    })
}

fn is_testcase_subclass(class_def: &StmtClassDef, semantic: &SemanticModel) -> bool {
    any_qualified_base_class(class_def, semantic, |qualified_name| {
        matches!(qualified_name.segments(), ["unittest", "TestCase"])
    })
}

fn is_unittest_testcase_expr(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(map_subscript(expr))
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["unittest", "TestCase"])
        })
}

/// Return `true` if the function body contains a call to `super().{method_name}(...)`
/// (including nested blocks such as `with` / `if`).
fn has_super_call(function_def: &StmtFunctionDef, method_name: &str) -> bool {
    let mut visitor = SuperCallVisitor {
        method_name,
        found: false,
    };
    visitor.visit_body(&function_def.body);
    visitor.found
}

struct SuperCallVisitor<'a> {
    method_name: &'a str,
    found: bool,
}

impl<'a> Visitor<'a> for SuperCallVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if self.found {
            return;
        }
        if is_super_method_call(expr, self.method_name) {
            self.found = true;
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

fn is_super_method_call(expr: &Expr, method_name: &str) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    let Expr::Attribute(ast::ExprAttribute {
        value: attr_value,
        attr,
        ..
    }) = func.as_ref()
    else {
        return false;
    };
    if attr != method_name {
        return false;
    }
    let Expr::Call(ast::ExprCall {
        func: super_func, ..
    }) = attr_value.as_ref()
    else {
        return false;
    };
    match super_func.as_ref() {
        Expr::Name(ast::ExprName { id, .. }) => id == "super",
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr == "super",
        _ => false,
    }
}
