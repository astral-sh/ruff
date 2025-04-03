use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

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
#[derive(ViolationMetadata)]
pub(crate) struct SysExitAlias {
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
pub(crate) fn sys_exit_alias(checker: &Checker, call: &ExprCall) {
    let Some(builtin) = checker.semantic().resolve_builtin_symbol(&call.func) else {
        return;
    };
    if !matches!(builtin, "exit" | "quit") {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        SysExitAlias {
            name: builtin.to_string(),
        },
        call.func.range(),
    );

    let has_star_kwargs = call
        .arguments
        .keywords
        .iter()
        .any(|kwarg| kwarg.arg.is_none());
    // only one optional argument allowed, and we can't convert **kwargs
    if call.arguments.len() > 1 || has_star_kwargs {
        checker.report_diagnostic(diagnostic);
        return;
    }

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("sys", "exit"),
            call.func.start(),
            checker.semantic(),
        )?;
        let reference_edit = Edit::range_replacement(binding, call.func.range());
        let mut edits = vec![reference_edit];
        if let Some(kwarg) = call.arguments.find_keyword("code") {
            edits.push(Edit::range_replacement(
                checker.source()[kwarg.value.range()].to_string(),
                kwarg.range,
            ));
        }
        Ok(Fix::unsafe_edits(import_edit, edits))
    });
    checker.report_diagnostic(diagnostic);
}
