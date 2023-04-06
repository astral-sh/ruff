use rustpython_parser::ast::{Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{format_call_path, from_unqualified_name, CallPath};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::rules::flake8_debugger::types::DebuggerUsingType;

#[violation]
pub struct Debugger {
    pub using_type: DebuggerUsingType,
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

const DEBUGGERS: &[&[&str]] = &[
    &["pdb", "set_trace"],
    &["pudb", "set_trace"],
    &["ipdb", "set_trace"],
    &["ipdb", "sset_trace"],
    &["IPython", "terminal", "embed", "InteractiveShellEmbed"],
    &[
        "IPython",
        "frontend",
        "terminal",
        "embed",
        "InteractiveShellEmbed",
    ],
    &["celery", "contrib", "rdb", "set_trace"],
    &["builtins", "breakpoint"],
    &["", "breakpoint"],
];

/// Checks for the presence of a debugger call.
pub fn debugger_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let Some(target) = checker.ctx.resolve_call_path(func).and_then(|call_path| {
        DEBUGGERS
            .iter()
            .find(|target| call_path.as_slice() == **target)
    }) {
        checker.diagnostics.push(Diagnostic::new(
            Debugger {
                using_type: DebuggerUsingType::Call(format_call_path(target)),
            },
            Range::from(expr),
        ));
    }
}

/// Checks for the presence of a debugger import.
pub fn debugger_import(stmt: &Stmt, module: Option<&str>, name: &str) -> Option<Diagnostic> {
    // Special-case: allow `import builtins`, which is far more general than (e.g.)
    // `import celery.contrib.rdb`).
    if module.is_none() && name == "builtins" {
        return None;
    }

    if let Some(module) = module {
        let mut call_path: CallPath = from_unqualified_name(module);
        call_path.push(name);
        if DEBUGGERS
            .iter()
            .any(|target| call_path.as_slice() == *target)
        {
            return Some(Diagnostic::new(
                Debugger {
                    using_type: DebuggerUsingType::Import(format_call_path(&call_path)),
                },
                Range::from(stmt),
            ));
        }
    } else {
        let parts: CallPath = from_unqualified_name(name);
        if DEBUGGERS
            .iter()
            .any(|call_path| &call_path[..call_path.len() - 1] == parts.as_slice())
        {
            return Some(Diagnostic::new(
                Debugger {
                    using_type: DebuggerUsingType::Import(name.to_string()),
                },
                Range::from(stmt),
            ));
        }
    }
    None
}
