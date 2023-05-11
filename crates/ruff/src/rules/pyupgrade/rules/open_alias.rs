use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct OpenAlias;

impl Violation for OpenAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with builtin `open`".to_string())
    }
}

/// UP020
pub(crate) fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["io", "open"])
    {
        let fixable = checker
            .ctx
            .find_binding("open")
            .map_or(true, |binding| binding.kind.is_builtin());
        let mut diagnostic = Diagnostic::new(OpenAlias, expr.range());
        if fixable && checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                "open".to_string(),
                func.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
