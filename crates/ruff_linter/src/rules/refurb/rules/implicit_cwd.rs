use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, ExprAttribute, ExprCall};
use ruff_text_size::Ranged;

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
/// cwd = Path().resolve()
/// ```
///
/// Use instead:
/// ```python
/// cwd = Path.cwd()
/// ```
///
/// ## References
/// - [Python documentation: `Path.cwd`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.cwd)

#[violation]
pub struct ImplicitCwd;

impl Violation for ImplicitCwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `Path.cwd()` over `Path().resolve()` for current-directory lookups")
    }
}

/// FURB177
pub(crate) fn no_implicit_cwd(checker: &mut Checker, call: &ExprCall) {
    if !call.arguments.is_empty() {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr != "resolve" {
        return;
    }

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    // Match on arguments, but ignore keyword arguments. `Path()` accepts keyword arguments, but
    // ignores them. See: https://github.com/python/cpython/issues/98094.
    match arguments.args.as_slice() {
        // Ex) `Path().resolve()`
        [] => {}
        // Ex) `Path(".").resolve()`
        [arg] => {
            let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(str),
                ..
            }) = arg
            else {
                return;
            };
            if !matches!(str.value.as_str(), "" | ".") {
                return;
            }
        }
        // Ex) `Path("foo", "bar").resolve()`
        _ => return,
    }

    if !checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pathlib", "Path"]))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(ImplicitCwd, call.range());

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

    checker
        .diagnostics
        .push(Diagnostic::new(ImplicitCwd, call.range()));
}
