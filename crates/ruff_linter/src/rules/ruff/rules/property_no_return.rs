use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::statement_visitor::{self, StatementVisitor};
use ruff_python_ast::{Stmt, StmtFunctionDef};
use ruff_python_semantic::analyze::visibility::is_abstract;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Detects `@property` methods that do not contain a return statement.
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
#[violation_metadata(preview_since = "0.14.6")]
pub(crate) struct PropertyNoReturn {
    name: String,
}

impl Violation for PropertyNoReturn {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("{name} is a property with no return statement")
    }
}

/// RUF066
pub(crate) fn property_no_return(checker: &Checker, function_def: &StmtFunctionDef) {
    let StmtFunctionDef {
        decorator_list,
        body,
        name,
        ..
    } = function_def;

    if is_abstract(decorator_list, checker.semantic()) {
        return;
    }

    let mut visitor = ReturnFinder::default();
    visitor.visit_body(body);

    if visitor.found {
        return;
    }

    checker.report_diagnostic(
        PropertyNoReturn {
            name: name.to_string(),
        },
        function_def.range(),
    );
}

#[derive(Default)]
struct ReturnFinder {
    found: bool,
}

impl StatementVisitor<'_> for ReturnFinder {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Return(_) => self.found = true,
            Stmt::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            _ => statement_visitor::walk_stmt(self, stmt),
        }
    }
}
