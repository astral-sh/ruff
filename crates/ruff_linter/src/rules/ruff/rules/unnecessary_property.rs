use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_python_semantic::analyze::{function_type, visibility};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Detects unnecessary `@property` methods.
///
/// An unnecessary property is a property that does not:
/// - return
/// - yield (or `yield from`)
/// - raises
///
/// ## Why is this bad?
/// Property methods are expected to return a computed value, a missing return in a property usually indicates an implementation mistake.
///
/// ## Example
/// ```python
/// class User:
///     @property
///     def full_name(self):
///         f"{self.first_name} {self.last_name}"
/// ```
///
/// Use instead:
/// ```python
/// class User:
///     @property
///     def full_name(self):
///         return f"{self.first_name} {self.last_name}"
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.7")]
pub(crate) struct UnnecessaryProperty {
    name: String,
}

impl Violation for UnnecessaryProperty {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("{name} is an unnecessary property")
    }
}

/// RUF069
pub(crate) fn unnecessary_property(checker: &Checker, function_def: &StmtFunctionDef) {
    let semantic = checker.semantic();

    if checker.source_type.is_stub() || semantic.in_protocol_or_abstract_method() {
        return;
    }

    let StmtFunctionDef {
        decorator_list,
        body,
        name,
        ..
    } = function_def;

    if !visibility::is_property(decorator_list, [], semantic)
        || visibility::is_overload(decorator_list, semantic)
        || function_type::is_stub(function_def, semantic)
    {
        return;
    }

    let mut visitor = PropertyVisitor::default();
    visitor.visit_body(body);
    if visitor.necessary {
        return;
    }

    checker.report_diagnostic(
        UnnecessaryProperty {
            name: name.to_string(),
        },
        function_def.range(),
    );
}

#[derive(Default)]
struct PropertyVisitor {
    necessary: bool,
}

impl Visitor<'_> for PropertyVisitor {
    fn visit_expr(&mut self, expr: &Expr) {
        if self.necessary {
            return;
        }

        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => self.necessary = true,
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        if self.necessary {
            return;
        }

        match stmt {
            Stmt::Return(_) => self.necessary = true,
            // Sometimes a property is defined because of an ABC requiremnet but it will always raise.
            // Thus not an unnecessary property.
            Stmt::Raise(_) => self.necessary = true,
            Stmt::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
