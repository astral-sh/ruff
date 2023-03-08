use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

// TODO: document referencing [PEP 585]: https://peps.python.org/pep-0585/
#[violation]
pub struct DeprecatedCollectionType {
    pub name: String,
}

impl AlwaysAutofixableViolation for DeprecatedCollectionType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedCollectionType { name } = self;
        format!(
            "Use `{}` instead of `{}` for type annotations",
            name.to_lowercase(),
            name,
        )
    }

    fn autofix_title(&self) -> String {
        let DeprecatedCollectionType { name } = self;
        format!("Replace `{name}` with `{}`", name.to_lowercase())
    }
}

/// UP006
pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr) {
    if let Some(binding) = checker
        .ctx
        .resolve_call_path(expr)
        .and_then(|call_path| call_path.last().copied())
    {
        let mut diagnostic = Diagnostic::new(
            DeprecatedCollectionType {
                name: binding.to_string(),
            },
            Range::from(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            let binding = binding.to_lowercase();
            if checker.ctx.is_builtin(&binding) {
                diagnostic.amend(Fix::replacement(
                    binding,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
