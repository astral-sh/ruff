use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UsePEP585Annotation {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for UsePEP585Annotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UsePEP585Annotation { name } = self;
        format!(
            "Use `{}` instead of `{}` for type annotations",
            name.to_lowercase(),
            name,
        )
    }

    fn autofix_title(&self) -> String {
        let UsePEP585Annotation { name } = self;
        format!("Replace `{name}` with `{}`", name.to_lowercase(),)
    }
}

/// UP006
pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr) {
    if let Some(binding) = checker
        .resolve_call_path(expr)
        .and_then(|call_path| call_path.last().copied())
    {
        let mut diagnostic = Diagnostic::new(
            UsePEP585Annotation {
                name: binding.to_string(),
            },
            Range::from_located(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                binding.to_lowercase(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
