use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, Stmt};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_debugger::types::DebuggerUsingType;

const DEBUGGERS: &[(&str, &str)] = &[
    ("pdb", "set_trace"),
    ("pudb", "set_trace"),
    ("ipdb", "set_trace"),
    ("ipdb", "sset_trace"),
    ("IPython.terminal.embed", "InteractiveShellEmbed"),
    ("IPython.frontend.terminal.embed", "InteractiveShellEmbed"),
    ("celery.contrib.rdb", "set_trace"),
    ("builtins", "breakpoint"),
    ("", "breakpoint"),
];

/// Checks for the presence of a debugger call.
pub fn debugger_call(
    expr: &Expr,
    func: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Check> {
    let call_path = dealias_call_path(collect_call_paths(func), import_aliases);
    if DEBUGGERS
        .iter()
        .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
    {
        Some(Check::new(
            CheckKind::Debugger(DebuggerUsingType::Call(call_path.join("."))),
            Range::from_located(expr),
        ))
    } else {
        None
    }
}

/// Checks for the presence of a debugger import.
pub fn debugger_import(stmt: &Stmt, module: Option<&str>, name: &str) -> Option<Check> {
    // Special-case: allow `import builtins`, which is far more general than (e.g.)
    // `import celery.contrib.rdb`).
    if module.is_none() && name == "builtins" {
        return None;
    }

    if let Some(module) = module {
        if let Some((module_name, member)) = DEBUGGERS
            .iter()
            .find(|(module_name, member)| module_name == &module && member == &name)
        {
            return Some(Check::new(
                CheckKind::Debugger(DebuggerUsingType::Import(format!("{module_name}.{member}"))),
                Range::from_located(stmt),
            ));
        }
    } else if DEBUGGERS
        .iter()
        .any(|(module_name, ..)| module_name == &name)
    {
        return Some(Check::new(
            CheckKind::Debugger(DebuggerUsingType::Import(name.to_string())),
            Range::from_located(stmt),
        ));
    }
    None
}
