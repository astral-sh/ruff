use ruff_python_ast::{self as ast, Arguments, Expr, ExprCall};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `pathlib.Path` objects that are initialized with the current
/// directory.
///
/// ## Why is this bad?
/// The `Path()` constructor defaults to the current directory, so passing it
/// in explicitly (as `"."`) is unnecessary.
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
///
/// ## References
/// - [Python documentation: `Path`](https://docs.python.org/3/library/pathlib.html#pathlib.Path)
#[violation]
pub struct PathConstructorCurrentDirectory;

impl AlwaysFixableViolation for PathConstructorCurrentDirectory {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not pass the current directory explicitly to `Path`")
    }

    fn fix_title(&self) -> String {
        "Remove the current directory argument".to_string()
    }
}

/// PTH201
pub(crate) fn path_constructor_current_directory(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if !checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pathlib", "Path" | "PurePath"]))
    {
        return;
    }

    let Expr::Call(ExprCall {
        arguments: Arguments { args, keywords, .. },
        ..
    }) = expr
    else {
        return;
    };

    if !keywords.is_empty() {
        return;
    }

    let [Expr::StringLiteral(ast::ExprStringLiteral { value, range })] = args.as_slice() else {
        return;
    };

    if matches!(value.as_str(), "" | ".") {
        let mut diagnostic = Diagnostic::new(PathConstructorCurrentDirectory, *range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(*range)));
        checker.diagnostics.push(diagnostic);
    }
}
