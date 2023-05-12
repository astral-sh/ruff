use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NonPEP585Annotation {
    name: String,
}

impl Violation for NonPEP585Annotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP585Annotation { name } = self;
        format!(
            "Use `{}` instead of `{}` for type annotations",
            name.to_lowercase(),
            name,
        )
    }

    fn autofix_title(&self) -> Option<String> {
        let NonPEP585Annotation { name } = self;
        Some(format!("Replace `{name}` with `{}`", name.to_lowercase()))
    }
}

/// UP006
pub(crate) fn use_pep585_annotation(checker: &mut Checker, expr: &Expr) {
    if let Some(binding) = checker
        .ctx
        .resolve_call_path(expr)
        .and_then(|call_path| call_path.last().copied())
    {
        let fixable = !checker.ctx.in_complex_string_type_definition();
        let mut diagnostic = Diagnostic::new(
            NonPEP585Annotation {
                name: binding.to_string(),
            },
            expr.range(),
        );
        if fixable && checker.patch(diagnostic.kind.rule()) {
            let binding = binding.to_lowercase();
            if checker.ctx.is_builtin(&binding) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    binding,
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
