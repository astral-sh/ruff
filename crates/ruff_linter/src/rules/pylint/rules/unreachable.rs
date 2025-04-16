use std::collections::HashSet;

use itertools::Itertools;
use ruff_python_ast::{helpers::Truthiness, Identifier, Stmt};
use ruff_python_semantic::{
    cfg::graph::{build_cfg, BlockId, Condition, ControlFlowGraph},
    SemanticModel,
};
use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unreachable code.
///
/// ## Why is this bad?
/// Unreachable code can be a maintenance burden without ever being used.
///
/// ## Example
/// ```python
/// def function():
///     if False:
///         return "unreachable"
///     return "reachable"
/// ```
///
/// Use instead:
/// ```python
/// def function():
///     return "reachable"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnreachableCode {
    name: String,
}

impl Violation for UnreachableCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnreachableCode { name } = self;
        format!("Unreachable code in `{name}`")
    }
}

pub(crate) fn in_function(checker: &Checker, name: &Identifier, body: &[Stmt]) {
    let cfg = build_cfg(body);
    let reachable = reachable(&cfg, checker.semantic());

    let mut blocks = (0..cfg.num_blocks())
        .map(BlockId::from_usize)
        .filter(|block| !cfg.stmts(*block).is_empty())
        .sorted_by_key(|block| cfg.range(*block).start())
        .peekable();

    // Advance past leading reachable blocks
    while blocks.next_if(|block| reachable.contains(block)).is_some() {}

    while let Some(start_block) = blocks.next() {
        // Advance to next reachable block
        let mut end_block = start_block;
        while let Some(next_block) = blocks.next_if(|block| !reachable.contains(block)) {
            end_block = next_block;
        }
        let start = cfg.range(start_block).start();
        let end = cfg.range(end_block).end();

        checker.report_diagnostic(Diagnostic::new(
            UnreachableCode {
                name: name.to_string(),
            },
            TextRange::new(start, end),
        ));
        // Advance past reachable blocks
        while blocks.next_if(|block| reachable.contains(block)).is_some() {}
    }
}

/// Returns set of block indices reachable from entry block
fn reachable(cfg: &ControlFlowGraph, semantic: &SemanticModel) -> HashSet<BlockId> {
    let mut reachable = HashSet::with_capacity(cfg.num_blocks());
    let mut stack = Vec::new();

    stack.push(cfg.initial());

    while let Some(block) = stack.pop() {
        if reachable.insert(block) {
            let mut already_taken_branch = false;
            stack.extend(
                cfg.outgoing(block)
                    // Traverse edges that are statically known to be possible to cross.
                    .filter_targets_by_conditions(|cond| {
                        if already_taken_branch {
                            false
                        } else {
                            match taken(cond, semantic) {
                                Some(true) => {
                                    already_taken_branch = true;
                                    true
                                }
                                Some(false) => false,
                                None => true,
                            }
                        }
                    }),
            );
        }
    }

    reachable
}

/// Determines if `condition` is taken.
///
/// Returns `Some(true)` if the condition is always true, e.g. `if True`, same
/// with `Some(false)` if it's never taken. If it can't be determined it returns
/// `None`, e.g. `if i == 100`.
#[allow(clippy::unnecessary_wraps)]
fn taken(condition: &Condition, semantic: &SemanticModel) -> Option<bool> {
    match condition {
        Condition::Always => Some(true),
        Condition::Test(expr) => {
            Truthiness::from_expr(expr, |id| semantic.has_builtin_binding(id)).into_bool()
        }
        Condition::NotStopIter(expr) => {
            match Truthiness::from_expr(expr, |id| semantic.has_builtin_binding(id)) {
                Truthiness::False | Truthiness::Falsey => Some(false),
                _ => None,
            }
        }
        Condition::Else
        | Condition::Match {
            subject: _,
            case: _,
        } => None,
        Condition::ExceptHandler(_) => None,
        Condition::UncaughtException => None,
        Condition::Deferred(_) => None,
        Condition::MaybeException => None,
    }
}
