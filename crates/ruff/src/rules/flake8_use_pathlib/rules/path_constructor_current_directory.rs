use ruff_python_ast::{self as ast, Arguments, Constant, Expr, ExprCall, ExprConstant};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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

impl AlwaysAutofixableViolation for PathConstructorCurrentDirectory {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not pass the current directory explicitly to `Path`")
    }

    fn autofix_title(&self) -> String {
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

    let [Expr::Constant(ExprConstant {
        value: Constant::Str(ast::StringConstant { value, .. }),
        range,
    })] = args.as_slice()
    else {
        return;
    };

    if matches!(value.as_str(), "" | ".") {
        let mut diagnostic = Diagnostic::new(PathConstructorCurrentDirectory, *range);
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::automatic(Edit::range_deletion(*range)));
        }
        checker.diagnostics.push(diagnostic);
    }
}
