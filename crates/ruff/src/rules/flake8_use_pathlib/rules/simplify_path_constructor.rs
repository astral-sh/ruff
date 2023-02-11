use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{AutofixKind, Availability, Violation};

define_violation!(
    /// ## What it does
    /// This rule detects pathlib's `Path` initializations with the default current directory argument.
    ///
    /// ## Why is this bad?
    /// The `Path()` constructor defaults to the current directory, so don't pass the
    /// current directory (`"."`) explicitly.
    ///
    /// ## Example
    /// ```python
    /// from pathlib import Path
    ///
    /// _ = Path(".")
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from pathlib import Path
    ///
    /// _ = Path()
    /// ```
    pub struct PathConstructorCurrentDirectory;
);
impl Violation for PathConstructorCurrentDirectory {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not pass the current directory explicitly to `Path`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|PathConstructorCurrentDirectory| format!("Replace `Path(\".\")` with `Path()`"))
    }
}

/// PTH200
pub fn simplify_path_constructor(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["pathlib", "Path"]
    }) {
        if let ExprKind::Call { args, keywords, .. } = &expr.node {
            if keywords.is_empty() && args.len() == 1 {
                let arg = &args.first().unwrap();
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &arg.node
                {
                    if value == "." {
                        let mut diagnostic = Diagnostic::new(
                            PathConstructorCurrentDirectory,
                            Range::from_located(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic
                                .amend(Fix::deletion(arg.location, arg.end_location.unwrap()));
                        }
                        checker.diagnostics.push(diagnostic);
                    };
                };
            }
        }
    }
}
