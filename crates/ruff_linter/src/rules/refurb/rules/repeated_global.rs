use itertools::Itertools;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for consecutive `global` (or `nonlocal`) statements.
///
/// ## Why is this bad?
/// The `global` and `nonlocal` keywords accepts multiple comma-separated names.
/// Instead of using multiple `global` (or `nonlocal`) statements for separate
/// variables, you can use a single statement to declare multiple variables at
/// once.
///
/// ## Example
/// ```python
/// def func():
///     global x
///     global y
///
///     print(x, y)
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     global x, y
///
///     print(x, y)
/// ```
///
/// ## References
/// - [Python documentation: the `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
/// - [Python documentation: the `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#the-nonlocal-statement)
#[derive(ViolationMetadata)]
pub(crate) struct RepeatedGlobal {
    global_kind: GlobalKind,
}

impl AlwaysFixableViolation for RepeatedGlobal {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of repeated consecutive `{}`", self.global_kind)
    }

    fn fix_title(&self) -> String {
        format!("Merge `{}` statements", self.global_kind)
    }
}

/// FURB154
pub(crate) fn repeated_global(checker: &Checker, mut suite: &[Stmt]) {
    while let Some(idx) = suite
        .iter()
        .position(|stmt| GlobalKind::from_stmt(stmt).is_some())
    {
        let global_kind = GlobalKind::from_stmt(&suite[idx]).unwrap();

        suite = &suite[idx..];

        // Collect until we see a non-`global` (or non-`nonlocal`) statement.
        let (globals_sequence, next_suite) = suite.split_at(
            suite
                .iter()
                .position(|stmt| GlobalKind::from_stmt(stmt) != Some(global_kind))
                .unwrap_or(suite.len()),
        );

        // If there are at least two consecutive `global` (or `nonlocal`) statements, raise a
        // diagnostic.
        if let [first, .., last] = globals_sequence {
            let range = first.range().cover(last.range());
            checker.report_diagnostic(
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
                                .map(ruff_python_ast::Identifier::id)
                                .format(", ")
                        ),
                        range,
                    ),
                )),
            );
        }

        suite = next_suite;
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum GlobalKind {
    Global,
    NonLocal,
}

impl GlobalKind {
    fn from_stmt(stmt: &Stmt) -> Option<Self> {
        match stmt {
            Stmt::Global(_) => Some(GlobalKind::Global),
            Stmt::Nonlocal(_) => Some(GlobalKind::NonLocal),
            _ => None,
        }
    }
}

impl std::fmt::Display for GlobalKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlobalKind::Global => write!(f, "global"),
            GlobalKind::NonLocal => write!(f, "nonlocal"),
        }
    }
}
