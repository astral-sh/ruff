use std::collections::HashSet;

use itertools::Itertools;
use ruff_python_ast::{Identifier, Stmt};
use ruff_python_semantic::cfg::graph::{build_cfg, BlockId, Condition, ControlFlowGraph};
use ruff_text_size::{Ranged, TextRange};

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
    let reachable = reachable(&cfg);

    let mut unreachable = (0..cfg.num_blocks())
        .map(BlockId::from_usize)
        .filter(|block| !reachable.contains(block) && !cfg.stmts(*block).is_empty())
        .map(|block| cfg.range(block))
        .sorted_by_key(ruff_text_size::Ranged::start)
        .peekable();

    while let Some(block_range) = unreachable.next() {
        let start = block_range.start();
        let mut end = block_range.end();
        while let Some(next_block) = unreachable.next_if(|nxt| nxt.start() <= end) {
            end = next_block.end();
        }
        checker.report_diagnostic(Diagnostic::new(
            UnreachableCode {
                name: name.to_string(),
            },
            TextRange::new(start, end),
        ));
    }
}

/// Returns set of block indices reachable from entry block
fn reachable(cfg: &ControlFlowGraph) -> HashSet<BlockId> {
    let mut reachable = HashSet::with_capacity(cfg.num_blocks());
    let mut stack = Vec::new();

    stack.push(cfg.initial());

    while let Some(block) = stack.pop() {
        if reachable.insert(block) {
            stack.extend(
                cfg.outgoing(block)
                    // Traverse edges that are statically known to be possible to cross.
                    .filter_targets_by_conditions(|cond| matches!(taken(cond), Some(true) | None)),
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
fn taken(condition: &Condition) -> Option<bool> {
    match condition {
        Condition::Always => Some(true),
    }
}
