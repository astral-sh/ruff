use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_python_semantic::ScopeKind;
use ruff_python_semantic::analyze::{function_type, visibility};

use crate::checkers::ast::Checker;
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
#[violation_metadata(preview_since = "0.14.7")]
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
pub(crate) fn property_without_return(checker: &Checker, function_def: &StmtFunctionDef) {
    let semantic = checker.semantic();

    if checker.source_type.is_stub() {
        return;
    }

    let StmtFunctionDef {
        decorator_list,
        body,
        name,
        ..
    } = function_def;

    let extra_property_decorators = checker.settings().pydocstyle.property_decorators();
    if !visibility::is_property(decorator_list, extra_property_decorators, semantic)
        || visibility::is_overload(decorator_list, semantic)
        || visibility::is_abstract(decorator_list, semantic)
        || function_type::is_stub(function_def, semantic)
    {
        return;
    }

    // A property is only exempt when its *owning* class is a protocol — the class the
    // method is directly defined in. A concrete class does not become a protocol just
    // because an enclosing class is one, so we check the nearest class scope rather than
    // any ancestor.
    let owner_is_protocol = matches!(
        semantic.current_scope().kind,
        ScopeKind::Class(class_def)
            if class_def
                .bases()
                .iter()
                .any(|base| semantic.match_typing_expr(map_subscript(base), "Protocol"))
    );
    if owner_is_protocol {
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
    fn visit_expr(&mut self, expr: &Expr) {
        if self.found {
            return;
        }

        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => self.found = true,
            // Lambdas are a separate scope; a `yield` (or any fall-through) inside a
            // lambda body belongs to the lambda, not the enclosing property.
            Expr::Lambda(_) => {}
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        if self.found {
            return;
        }

        match stmt {
            Stmt::Return(_) | Stmt::Raise(_) => self.found = true,
            Stmt::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
