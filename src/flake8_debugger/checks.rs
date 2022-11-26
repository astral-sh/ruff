use fnv::FnvHashMap;
use rustpython_ast::{Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_debugger::types::{Debugger, DebuggerUsingType};

const BREAKPOINT_FN: &str = "breakpoint";

const BUILTINS_MODULE: &str = "builtins";

const DEBUGGERS: [Debugger; 7] = [
    ("pdb", &["set_trace"]),
    ("pudb", &["set_trace"]),
    ("ipdb", &["set_trace", "sset_trace"]),
    ("IPython.terminal.embed", &["InteractiveShellEmbed"]),
    ("IPython.frontend.terminal.embed", &["InteractiveShellEmbed"]),
    ("celery.contrib.rdb", &["set_trace"]),
    (BUILTINS_MODULE, &[BREAKPOINT_FN]),
];

fn get_debugger(module_name: &str) -> Option<&Debugger> {
    DEBUGGERS.iter().find(|&d| d.0 == module_name)
}

fn function_name(func: &Expr) -> Option<&str> {
    if let ExprKind::Name { id, .. } = &func.node {
        Some(id)
    } else {
        None
    }
}

/// Checks for the presence of a debugger call.
pub fn debugger_call(
    expr: &Expr,
    func: &Expr,
    import_aliases: &FnvHashMap<&str, &str>,
) -> Option<Check> {
    let raw_func_name = function_name(func)?;
    let func_name = match import_aliases.get(raw_func_name) {
        Some(func_name) => func_name,
        None => raw_func_name,
    };

    if func_name == BREAKPOINT_FN {
        return Some(Check::new(
            CheckKind::Debugger(DebuggerUsingType::Call(func_name.to_string())),
            Range::from_located(expr),
        ));
    }

    if let Some(_) = DEBUGGERS.iter().find(|&d| d.1.contains(&func_name)) {
        return Some(Check::new(
            CheckKind::Debugger(DebuggerUsingType::Call(raw_func_name.to_string())),
            Range::from_located(expr),
        ));
    }

    None
}

/// Checks for the presence of a debugger import.
pub fn debugger_import(stmt: &Stmt, module: &Option<String>, name: &str) -> Option<Check> {
    if let Some(module) = module {
        if let Some(debugger) = get_debugger(module) {
            if debugger.1.contains(&name) {
                return Some(Check::new(
                    CheckKind::Debugger(DebuggerUsingType::Import),
                    Range::from_located(stmt),
                ));
            }
        }
    } else if name != BUILTINS_MODULE {
        if let Some(_) = get_debugger(name) {
            return Some(Check::new(
                CheckKind::Debugger(DebuggerUsingType::Import),
                Range::from_located(stmt),
            ));
        }
    }

    None
}
