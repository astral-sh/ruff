use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprCall, ExprKind};
use ruff_text_size::Ranged;

use crate::codes::Category;
use crate::{Edit, Fix, FixAvailability, Violation};
use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for current-directory lookups using `Path().resolve()`.
///
/// ## Why is this bad?
/// When looking up the current directory, prefer `Path.cwd()` over
/// `Path().resolve()`, as `Path.cwd()` is more explicit in its intent.
///
/// ## Example
/// ```python
/// from pathlib import Path
///
/// cwd = Path().resolve()
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// cwd = Path.cwd()
/// ```
///
/// ## References
/// - [Python documentation: `Path.cwd`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.cwd)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Complexity)]
pub(crate) struct ImplicitCwd;

impl Violation for ImplicitCwd {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Prefer `Path.cwd()` over `Path().resolve()` for current-directory lookups".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace `Path().resolve()` with `Path.cwd()`".to_string())
    }
}

/// FURB177
pub(crate) fn no_implicit_cwd(checker: &Checker, call: ExprCall<'_>) {
    if !call.arguments().is_empty() {
        return;
    }

    let ExprKind::Attribute(attribute) = call.func().classify() else {
        return;
    };
    let attr = attribute.attr();
    let value = attribute.value();

    if attr != "resolve" {
        return;
    }

    let ExprKind::Call(call_node) = value.classify() else {
        return;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();

    // Match on arguments, but ignore keyword arguments. `Path()` accepts keyword arguments, but
    // ignores them. See: https://github.com/python/cpython/issues/98094.
    match arguments.args() {
        // Ex) `Path().resolve()`
        args if args.is_empty() => {}
        // Ex) `Path(".").resolve()`
        args if let Some([arg]) = args.as_array() => {
            let ExprKind::StringLiteral(string_literal) = arg.classify() else {
                return;
            };
            let value = string_literal.value();
            if !matches!(value.to_str(), "" | ".") {
                return;
            }
        }
        // Ex) `Path("foo", "bar").resolve()`
        _ => return,
    }

    if !checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pathlib", "Path"]))
    {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(ImplicitCwd, call.range());

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;
        Ok(Fix::unsafe_edits(
            Edit::range_replacement(format!("{binding}.cwd()"), call.range()),
            [import_edit],
        ))
    });
}
