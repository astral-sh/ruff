use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct SysExitAlias {
    pub name: String,
}

impl Violation for SysExitAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SysExitAlias { name } = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|SysExitAlias { name }| format!("Replace `{name}` with `sys.exit()`"))
    }
}

/// PLR1722
pub fn sys_exit_alias(checker: &mut Checker, func: &Expr) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    for name in ["exit", "quit"] {
        if id != name {
            continue;
        }
        if !checker.ctx.is_builtin(name) {
            continue;
        }
        let mut diagnostic = Diagnostic::new(
            SysExitAlias {
                name: name.to_string(),
            },
            Range::from(func),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(binding) = checker.ctx.resolve_qualified_import_name("sys", "exit") {
                diagnostic.set_fix(Edit::replacement(
                    binding,
                    func.location,
                    func.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
