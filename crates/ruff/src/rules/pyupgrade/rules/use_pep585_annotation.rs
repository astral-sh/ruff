use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::typing::AnnotationKind;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NonPEP585Annotation {
    pub name: String,
    pub fixable: bool,
}

impl Violation for NonPEP585Annotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP585Annotation { name, .. } = self;
        format!(
            "Use `{}` instead of `{}` for type annotations",
            name.to_lowercase(),
            name,
        )
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable.then_some(|NonPEP585Annotation { name, .. }| {
            format!("Replace `{name}` with `{}`", name.to_lowercase())
        })
    }
}

/// UP006
pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr) {
    if let Some(binding) = checker
        .ctx
        .resolve_call_path(expr)
        .and_then(|call_path| call_path.last().copied())
    {
        let fixable = checker
            .ctx
            .in_deferred_string_type_definition
            .as_ref()
            .map_or(true, AnnotationKind::is_simple);
        let mut diagnostic = Diagnostic::new(
            NonPEP585Annotation {
                name: binding.to_string(),
                fixable,
            },
            Range::from(expr),
        );
        if fixable && checker.patch(diagnostic.kind.rule()) {
            let binding = binding.to_lowercase();
            if checker.ctx.is_builtin(&binding) {
                diagnostic.set_fix(Edit::replacement(
                    binding,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
