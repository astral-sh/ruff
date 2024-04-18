use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of the `exit()` and `quit()`.
///
/// ## Why is this bad?
/// `exit` and `quit` come from the `site` module, which is typically imported
/// automatically during startup. However, it is not _guaranteed_ to be
/// imported, and so using these functions may result in a `NameError` at
/// runtime. Generally, these constants are intended to be used in an interactive
/// interpreter, and not in programs.
///
/// Prefer `sys.exit()`, as the `sys` module is guaranteed to exist in all
/// contexts.
///
/// ## Example
/// ```python
/// if __name__ == "__main__":
///     exit()
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if __name__ == "__main__":
///     sys.exit()
/// ```
///
/// ## References
/// - [Python documentation: Constants added by the `site` module](https://docs.python.org/3/library/constants.html#constants-added-by-the-site-module)
#[violation]
pub struct SysExitAlias {
    name: String,
}

impl Violation for SysExitAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SysExitAlias { name } = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn fix_title(&self) -> Option<String> {
        let SysExitAlias { name } = self;
        Some(format!("Replace `{name}` with `sys.exit()`"))
    }
}

/// PLR1722
pub(crate) fn sys_exit_alias(checker: &mut Checker, func: &Expr) {
    let Some(builtin) = checker.semantic().resolve_builtin_symbol(func) else {
        return;
    };
    if !matches!(builtin, "exit" | "quit") {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        SysExitAlias {
            name: builtin.to_string(),
        },
        func.range(),
    );
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("sys", "exit"),
            func.start(),
            checker.semantic(),
        )?;
        let reference_edit = Edit::range_replacement(binding, func.range());
        Ok(Fix::unsafe_edits(import_edit, [reference_edit]))
    });
    checker.diagnostics.push(diagnostic);
}
