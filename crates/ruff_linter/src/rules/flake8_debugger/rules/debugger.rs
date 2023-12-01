use ruff_python_ast::{Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{format_call_path, from_unqualified_name, CallPath};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_debugger::types::DebuggerUsingType;

/// ## What it does
/// Checks for the presence of debugger calls and imports.
///
/// ## Why is this bad?
/// Debugger calls and imports should be used for debugging purposes only. The
/// presence of a debugger call or import in production code is likely a
/// mistake and may cause unintended behavior, such as exposing sensitive
/// information or causing the program to hang.
///
/// Instead, consider using a logging library to log information about the
/// program's state, and writing tests to verify that the program behaves
/// as expected.
///
/// ## Example
/// ```python
/// def foo():
///     breakpoint()
/// ```
///
/// ## References
/// - [Python documentation: `pdb` — The Python Debugger](https://docs.python.org/3/library/pdb.html)
/// - [Python documentation: `logging` — Logging facility for Python](https://docs.python.org/3/library/logging.html)
#[violation]
pub struct Debugger {
    using_type: DebuggerUsingType,
}

impl Violation for Debugger {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Debugger { using_type } = self;
        match using_type {
            DebuggerUsingType::Call(name) => format!("Trace found: `{name}` used"),
            DebuggerUsingType::Import(name) => format!("Import for `{name}` found"),
        }
    }
}

/// Checks for the presence of a debugger call.
pub(crate) fn debugger_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let Some(using_type) = checker
        .semantic()
        .resolve_call_path(func)
        .and_then(|call_path| {
            if is_debugger_call(&call_path) {
                Some(DebuggerUsingType::Call(format_call_path(&call_path)))
            } else {
                None
            }
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(Debugger { using_type }, expr.range()));
    }
}

/// Checks for the presence of a debugger import.
pub(crate) fn debugger_import(stmt: &Stmt, module: Option<&str>, name: &str) -> Option<Diagnostic> {
    if let Some(module) = module {
        let mut call_path: CallPath = from_unqualified_name(module);
        call_path.push(name);

        if is_debugger_call(&call_path) {
            return Some(Diagnostic::new(
                Debugger {
                    using_type: DebuggerUsingType::Import(format_call_path(&call_path)),
                },
                stmt.range(),
            ));
        }
    } else {
        let call_path: CallPath = from_unqualified_name(name);

        if is_debugger_import(&call_path) {
            return Some(Diagnostic::new(
                Debugger {
                    using_type: DebuggerUsingType::Import(name.to_string()),
                },
                stmt.range(),
            ));
        }
    }
    None
}

fn is_debugger_call(call_path: &CallPath) -> bool {
    matches!(
        call_path.as_slice(),
        ["pdb" | "pudb" | "ipdb", "set_trace"]
            | ["ipdb", "sset_trace"]
            | ["IPython", "terminal", "embed", "InteractiveShellEmbed"]
            | [
                "IPython",
                "frontend",
                "terminal",
                "embed",
                "InteractiveShellEmbed"
            ]
            | ["celery", "contrib", "rdb", "set_trace"]
            | ["builtins" | "", "breakpoint"]
    )
}

fn is_debugger_import(call_path: &CallPath) -> bool {
    // Constructed by taking every pattern in `is_debugger_call`, removing the last element in
    // each pattern, and de-duplicating the values.
    // As a special-case, we omit `builtins` to allow `import builtins`, which is far more general
    // than (e.g.) `import celery.contrib.rdb`.
    matches!(
        call_path.as_slice(),
        ["pdb" | "pudb" | "ipdb"]
            | ["IPython", "terminal", "embed"]
            | ["IPython", "frontend", "terminal", "embed",]
            | ["celery", "contrib", "rdb"]
    )
}
