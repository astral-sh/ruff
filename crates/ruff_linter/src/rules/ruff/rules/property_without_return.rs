use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_python_semantic::analyze::{function_type, visibility};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Detects class `@property` methods that does not have a `return` statement.
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
///
/// ## Options
///
/// - `lint.pydocstyle.property-decorators`
///
/// ## References
/// - [Python documentation: The property class](https://docs.python.org/3/library/functions.html#property)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.7", category = Category::Correctness)]
pub(crate) struct PropertyWithoutReturn {
    name: String,
}

impl Violation for PropertyWithoutReturn {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("`{name}` is a property without a `return` statement")
    }
}

/// RUF066
pub(crate) fn property_without_return(
    checker: &Checker,
    function_def: flat::View<'_, StmtFunctionDef>,
) {
    let semantic = checker.semantic();

    if checker.source_type.is_stub() || semantic.in_protocol_or_abstract_method() {
        return;
    }

    let decorator_list = function_def.decorator_list();
    let body = function_def.body();
    let name = function_def.name();

    let extra_property_decorators = checker.settings().pydocstyle.property_decorators();
    if !visibility::is_property(decorator_list, extra_property_decorators, semantic)
        || visibility::is_overload(decorator_list, semantic)
        || function_type::is_stub(function_def, semantic)
    {
        return;
    }

    let mut visitor = PropertyVisitor::default();
    visitor.visit_body(body);
    if visitor.found {
        return;
    }

    checker.report_diagnostic(
        PropertyWithoutReturn {
            name: name.to_string(),
        },
        function_def.identifier(),
    );
}

#[derive(Default)]
struct PropertyVisitor {
    found: bool,
}

// NOTE: We are actually searching for the presence of
// `yield`/`yield from`/`raise`/`return` statement/expression,
// as having one of those indicates that there's likely no implementation mistake
impl Visitor<'_> for PropertyVisitor {
    fn visit_expr(&mut self, expr: flat::View<'_, Expr>) {
        if self.found {
            return;
        }

        match expr.classify() {
            flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_) => self.found = true,
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: flat::View<'_, Stmt>) {
        if self.found {
            return;
        }

        match stmt.classify() {
            flat::StmtKind::Return(_) | flat::StmtKind::Raise(_) => self.found = true,
            flat::StmtKind::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
