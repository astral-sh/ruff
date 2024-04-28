use itertools::Itertools;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{traversal, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for consecutive `global` (`nonlocal`) statements.
///
/// ## Why is this bad?
/// The `global` and `nonlocal` keywords can take multiple comma-separated names, removing the need
/// for multiple lines.
///
/// ## Example
/// ```python
/// def some_func():
///     global x
///     global y
///
///     print(x, y)
/// ```
///
/// Use instead:
/// ```python
/// def some_func():
///     global x, y
///
///     print(x, y)
/// ```
///
/// ## References
/// - [Python documentation: the `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
/// - [Python documentation: the `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#the-nonlocal-statement)
#[violation]
pub struct RepeatedGlobal {
    global_kind: GlobalKind,
}

impl AlwaysFixableViolation for RepeatedGlobal {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of repeated consecutive `{}`", self.global_kind)
    }

    fn fix_title(&self) -> String {
        format!("Merge to one `{}`", self.global_kind)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum GlobalKind {
    Global,
    NonLocal,
}

impl std::fmt::Display for GlobalKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlobalKind::Global => write!(f, "global"),
            GlobalKind::NonLocal => write!(f, "nonlocal"),
        }
    }
}

fn get_global_kind(stmt: &Stmt) -> Option<GlobalKind> {
    match stmt {
        Stmt::Global(_) => Some(GlobalKind::Global),
        Stmt::Nonlocal(_) => Some(GlobalKind::NonLocal),
        _ => None,
    }
}

fn match_globals_sequence<'a>(
    semantic: &'a SemanticModel<'a>,
    stmt: &'a Stmt,
) -> Option<(GlobalKind, &'a [Stmt])> {
    let global_kind = get_global_kind(stmt)?;

    let siblings = if semantic.at_top_level() {
        semantic.definitions.python_ast()?
    } else {
        semantic
            .current_statement_parent()
            .and_then(|parent| traversal::suite(stmt, parent))?
    };
    let stmt_idx = siblings.iter().position(|sibling| sibling == stmt)?;
    if stmt_idx != 0 && get_global_kind(&siblings[stmt_idx - 1]) == Some(global_kind) {
        return None;
    }
    let siblings = &siblings[stmt_idx..];
    Some((
        global_kind,
        siblings
            .iter()
            .position(|sibling| get_global_kind(sibling) != Some(global_kind))
            .map_or(siblings, |size| &siblings[..size]),
    ))
}

/// FURB154
pub(crate) fn repeated_global(checker: &mut Checker, stmt: &Stmt) {
    let Some((global_kind, globals_sequence)) = match_globals_sequence(checker.semantic(), stmt)
    else {
        return;
    };
    // if there are at least 2 consecutive `global` (`nonlocal`) statements
    if let [first, .., last] = globals_sequence {
        let range = first.range().cover(last.range());
        checker.diagnostics.push(
            Diagnostic::new(RepeatedGlobal { global_kind }, range).with_fix(Fix::safe_edit(
                Edit::range_replacement(
                    format!(
                        "{global_kind} {}",
                        globals_sequence
                            .iter()
                            .flat_map(|stmt| match stmt {
                                Stmt::Global(stmt) => &stmt.names,
                                Stmt::Nonlocal(stmt) => &stmt.names,
                                _ => unreachable!(),
                            })
                            .map(|identifier| &identifier.id)
                            .format(", ")
                    ),
                    range,
                ),
            )),
        );
    }
}
