use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct OpenAlias {
    pub fixable: bool,
}

impl Violation for OpenAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|_| format!("Replace with builtin `open`"))
    }
}

/// UP020
pub fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["io", "open"])
    {
        let fixable = checker
            .ctx
            .find_binding("open")
            .map_or(true, |binding| binding.kind.is_builtin());
        let mut diagnostic = Diagnostic::new(OpenAlias { fixable }, Range::from(expr));
        if fixable && checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Edit::replacement(
                "open".to_string(),
                func.location,
                func.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
