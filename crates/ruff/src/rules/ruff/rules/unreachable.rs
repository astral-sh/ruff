use std::{fmt, iter};

use log::error;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{
    Expr, Identifier, MatchCase, Pattern, PatternMatchAs, Ranged, Stmt, StmtAsyncFor,
    StmtAsyncWith, StmtFor, StmtMatch, StmtReturn, StmtTry, StmtTryStar, StmtWhile, StmtWith,
};
use rustpython_parser::text_size::{TextRange, TextSize};

/// ## What it does
/// Checks for unreachable code.
///
/// ## Why is this bad?
/// Unreachable code can be a maintenance burden without ever being used.
///
/// ## Example
/// ```python
/// def function()
///     if False
///         return 0
///     return 1
/// ```
///
/// Use instead:
/// ```python
/// def function()
///     return 1
/// ```
#[violation]
pub struct UnreachableCode {
    name: String,
}

impl Violation for UnreachableCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnreachableCode { name } = self;
        format!("Unreachable code in {name}")
    }
}

pub(crate) fn in_function(name: &Identifier, body: &[Stmt]) -> Vec<Diagnostic> {
    // Create basic code blocks from the body.
    let basic_blocks = BasicBlocks::from(body);

    // Basic on the code blocks we can (more) easily follow what statements are
    // and aren't reached, we'll mark them as such in `reached_map`.
    let mut reached_map = Bitmap::with_capacity(basic_blocks.blocks.len());
    mark_reached(
        &mut reached_map,
        &basic_blocks.blocks,
        basic_blocks.blocks.len() - 1,
    );

    // For each unreached code block create a diagnostic.
    reached_map
        .unset()
        .filter_map(|idx| {
            let block = &basic_blocks.blocks[idx];
            if block.is_sentinel() {
                return None;
            }

            // TODO: add more information to the diagnostic. Include the entire
            // code block, not just the first line. Maybe something to indicate
            // the code flow and where it prevents this block from being reached
            // for example.
            let Some(stmt) = block.stmts.first() else {
                // This should never happen.
                error!("Got an unexpected empty code block");
                return None;
            };
            Some(Diagnostic::new(
                UnreachableCode {
                    name: name.as_str().to_owned(),
                },
                stmt.range(),
            ))
        })
        .collect()
}

/// Simple bitmap.
#[derive(Debug)]
struct Bitmap {
    bits: Box<[usize]>,
    capacity: usize,
}

impl Bitmap {
    /// Create a new `Bitmap` with `capacity` capacity.
    fn with_capacity(capacity: usize) -> Bitmap {
        let mut size = capacity / usize::BITS as usize;
        if (capacity % usize::BITS as usize) != 0 {
            size += 1;
        }
        Bitmap {
            bits: vec![0; size].into_boxed_slice(),
            capacity,
        }
    }

    /// Set bit at index `idx` to true.
    ///
    /// Returns a boolean indicating if the bit was already set.
    fn set(&mut self, idx: usize) -> bool {
        let n = idx / usize::BITS as usize;
        let s = idx % usize::BITS as usize;
        if (self.bits[n] & (1 << s)) == 0 {
            self.bits[n] |= 1 << s;
            false
        } else {
            true
        }
    }

    /// Returns an iterator of all unset indices.
    fn unset(&self) -> impl Iterator<Item = usize> + '_ {
        let mut index = 0;
        let mut shift = 0;
        let last_max_shift = self.capacity % usize::BITS as usize;
        iter::from_fn(move || loop {
            if shift >= usize::BITS as usize {
                shift = 0;
                index += 1;
            }
            if self.bits.len() <= index || (index >= self.bits.len() - 1 && shift >= last_max_shift)
            {
                return None;
            }

            let is_set = (self.bits[index] & (1 << shift)) != 0;
            shift += 1;
            if !is_set {
                return Some((index * usize::BITS as usize) + shift - 1);
            }
        })
    }
}

/// Set bits in `reached_map` for all blocks that are reached in `blocks`
/// starting with block at index `idx`.
fn mark_reached(reached_map: &mut Bitmap, blocks: &[BasicBlock<'_>], mut idx: BlockIndex) {
    loop {
        let block = &blocks[idx];
        if reached_map.set(idx) {
            return; // Block already visited, no needed to do it again.
        }

        match &block.next {
            NextBlock::Always(next) => idx = *next,
            NextBlock::If {
                condition,
                next,
                orelse,
            } => {
                match taken(condition) {
                    Some(true) => idx = *next,    // Always taken.
                    Some(false) => idx = *orelse, // Never taken.
                    None => {
                        // Don't know, both branches might be taken.
                        idx = *next;
                        mark_reached(reached_map, blocks, *orelse);
                    }
                }
            }
            NextBlock::Terminate => return,
        }
    }
}

/// Determines if `condition` is taken.
/// Returns `Some(true)` if the condition is always true, e.g. `if True`, same
/// with `Some(false)` if it's never taken. If it can't be determined it returns
/// `None`, e.g. `If i == 100`.
fn taken(condition: &Condition) -> Option<bool> {
    // TODO: add more cases to this where we can determine a condition
    // statically. For now we only consider constant booleans.
    match condition {
        Condition::Test(expr) => match expr {
            Expr::Constant(constant) => constant.value.as_bool().copied(),
            _ => None,
        },
        Condition::Iterator(_) => None,
        Condition::Match { .. } => None,
    }
}

/// Collection of basic block.
#[derive(Debug, PartialEq)]
struct BasicBlocks<'stmt> {
    /// # Notes
    ///
    /// The order of these block is unspecified. However it's guaranteed that
    /// the last block is the statement in the function and the first block is
    /// the last statement. The block are more or less in reverse order, but it
    /// gets fussy around control flow statements (e.g. `if` statements).
    ///
    /// For loop blocks, and similar recurring control flows, the end of the
    /// body will point to the loop block again (to create the loop). However an
    /// oddity here is that this block might contain statements before the loop
    /// itself which, of course, won't be executed again.
    ///
    /// For example:
    /// ```python
    /// i = 0          # block 0
    /// while True:    #
    ///     continue   # block 1
    /// ```
    /// Will create a connection between block 1 (loop body) and block 0, which
    /// includes the `i = 0` statement.
    ///
    /// To keep `NextBlock` simple(r) `NextBlock::If`'s `next` and `orelse`
    /// fields only use `BlockIndex`, which means that they can't terminate
    /// themselves. To support this we insert *empty*/fake blocks before the end
    /// of the function that we can link to.
    ///
    /// Finally `BasicBlock` can also be a sentinal node, see the associated
    /// constants of [`BasicBlock`].
    blocks: Vec<BasicBlock<'stmt>>,
}

impl<'stmt> From<&'stmt [Stmt]> for BasicBlocks<'stmt> {
    /// # Notes
    ///
    /// This assumes that `stmts` is a function body.
    fn from(stmts: &'stmt [Stmt]) -> BasicBlocks<'stmt> {
        let mut blocks = Vec::with_capacity(stmts.len());

        create_blocks(&mut blocks, stmts, None);

        if blocks.is_empty() {
            blocks.push(BasicBlock::EMPTY);
        }

        BasicBlocks { blocks }
    }
}

/// Basic code block, sequence of statements unconditionally executed
/// "together".
#[derive(Debug, PartialEq)]
struct BasicBlock<'stmt> {
    stmts: &'stmt [Stmt],
    next: NextBlock<'stmt>,
}

/// Edge between basic blocks (in the control-flow graph).
#[derive(Debug, PartialEq)]
enum NextBlock<'stmt> {
    /// Always continue with a block.
    Always(BlockIndex),
    /// Condition jump.
    If {
        /// Condition that needs to be evaluated to jump to the `next` or
        /// `orelse` block.
        condition: Condition<'stmt>,
        /// Next block if `condition` is true.
        next: BlockIndex,
        /// Next block if `condition` is false.
        orelse: BlockIndex,
    },
    /// The end.
    Terminate,
}

/// Condition used to determine to take the `next` or `orelse` branch in
/// [`NextBlock::If`].
#[derive(Clone, Debug, PartialEq)]
enum Condition<'stmt> {
    /// Conditional statement, this should evaluate to a boolean, for e.g. `if`
    /// or `while`.
    Test(&'stmt Expr),
    /// Iterator for `for` statements, e.g. for `i in range(10)` this will be
    /// `range(10)`.
    Iterator(&'stmt Expr),
    Match {
        /// `match $subject`.
        subject: &'stmt Expr,
        /// `case $case`, include pattern, guard, etc.
        case: &'stmt MatchCase,
    },
}

impl<'stmt> Ranged for Condition<'stmt> {
    fn range(&self) -> TextRange {
        match self {
            Condition::Test(expr) | Condition::Iterator(expr) => expr.range(),
            // The case of the match statement, without the body.
            Condition::Match { subject: _, case } => TextRange::new(
                case.start(),
                case.guard
                    .as_ref()
                    .map_or(case.pattern.end(), |guard| guard.end()),
            ),
        }
    }
}

/// Index into [`BasicBlocks::blocks`].
type BlockIndex = usize;

impl<'stmt> BasicBlock<'stmt> {
    /// A sentinal block indicating an empty termination block.
    const EMPTY: BasicBlock<'static> = BasicBlock {
        stmts: &[],
        next: NextBlock::Terminate,
    };

    /// A sentinal block indicating an exception was raised.
    const EXCEPTION: BasicBlock<'static> = BasicBlock {
        stmts: &[Stmt::Return(StmtReturn {
            range: TextRange::new(TextSize::new(0), TextSize::new(0)),
            value: None,
        })],
        next: NextBlock::Terminate,
    };

    /// Return true if the block is a sentinal or fake block.
    fn is_sentinel(&self) -> bool {
        self.is_empty() || self.is_exception()
    }

    /// Returns an empty block that terminates.
    fn is_empty(&self) -> bool {
        matches!(self.next, NextBlock::Terminate) && self.stmts.is_empty()
    }

    /// Returns true if `self` an [`BasicBlock::EXCEPTION`].
    fn is_exception(&self) -> bool {
        matches!(self.next, NextBlock::Terminate) && BasicBlock::EXCEPTION.stmts == self.stmts
    }
}

/// Creates basic blocks from `stmts` and appends them to `blocks`.
fn create_blocks<'stmt>(
    blocks: &mut Vec<BasicBlock<'stmt>>,
    stmts: &'stmt [Stmt],
    mut after: Option<BlockIndex>,
) {
    // We process the statements in reverse so that we can always point to the
    // next block (as that should always be processed).
    let mut stmts_iter = stmts.iter().enumerate().rev().peekable();
    while let Some((i, stmt)) = stmts_iter.next() {
        let next = match stmt {
            // Statements that continue to the next statement after execution.
            Stmt::FunctionDef(_)
            | Stmt::AsyncFunctionDef(_)
            | Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::ClassDef(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Delete(_)
            | Stmt::Assign(_)
            | Stmt::AugAssign(_)
            | Stmt::AnnAssign(_)
            | Stmt::Expr(_)
            | Stmt::Break(_)
            | Stmt::Continue(_) // NOTE: the next branch gets fixed up in `change_next_block`.
            | Stmt::Pass(_) => unconditional_next_block(blocks),
            // Statements that (can) divert the control flow.
            Stmt::If(stmt) => {
                let next_after_block = after.unwrap_or_else(|| maybe_next_block_index(blocks, || needs_next_block(&stmt.body)));
                let orelse_after_block = after.unwrap_or_else(|| maybe_next_block_index(blocks, || needs_next_block(&stmt.orelse)));
                let next = append_blocks_if_not_empty(blocks, &stmt.body, next_after_block);
                let orelse = append_blocks_if_not_empty(blocks, &stmt.orelse, orelse_after_block);
                NextBlock::If {
                    condition: Condition::Test(&stmt.test),
                    next,
                    orelse,
                }
            }
            Stmt::While(StmtWhile {
                test: condition,
                body,
                orelse,
                ..
            }) => loop_block(blocks, Condition::Test(condition), body, orelse, after),
            Stmt::For(StmtFor {
                iter: condition,
                body,
                orelse,
                ..
            })
            | Stmt::AsyncFor(StmtAsyncFor {
                iter: condition,
                body,
                orelse,
                ..
            }) => loop_block(blocks, Condition::Iterator(condition), body, orelse, after),
            Stmt::Try(StmtTry { body, handlers, orelse, finalbody, .. })
            | Stmt::TryStar(StmtTryStar { body, handlers, orelse, finalbody, .. }) => {
                // TODO: handle `try` statements. The `try` control flow is very
                // complex, what blocks are and aren't taken and from which
                // block the control flow is actually returns is **very**
                // specific to the contents of the block. Read
                // <https://docs.python.org/3/reference/compound_stmts.html#the-try-statement>
                // very carefully.
                // For now we'll skip over it.
                let _ = (body, handlers, orelse, finalbody); // Silence unused code warnings.
                unconditional_next_block(blocks)
            }
            Stmt::With(StmtWith { items, body, type_comment, .. })
            | Stmt::AsyncWith(StmtAsyncWith { items, body, type_comment, .. }) => {
                // TODO: handle `with` statements, see
                // <https://docs.python.org/3/reference/compound_stmts.html#the-with-statement>.
                // I recommend to `try` statements first as `with` can desugar
                // to a `try` statement.
                // For now we'll skip over it.
                let _ = (items, body, type_comment); // Silence unused code warnings.
                unconditional_next_block(blocks)
            }
            Stmt::Match(StmtMatch { subject, cases, .. }) => {
                let next_after_block = maybe_next_block_index(blocks, || {
                    // We don't need need a next block if all cases don't need a
                    // next block, i.e. if no cases need a next block, and we
                    // have a wildcard case (to ensure one of the block is
                    // always taken).
                    // NOTE: match statement require at least one case, so we
                    // don't have to worry about empty `cases`.
                    // TODO: support exhaustive cases without a wildcard.
                    cases.iter().any(|case| needs_next_block(&case.body))
                        || !cases.iter().any(is_wildcard)
                });
                let mut orelse_after_block = next_after_block;
                for case in cases.iter().rev() {
                    let block = match_case(blocks, stmt, subject, case, next_after_block, orelse_after_block);
                    blocks.push(block);
                    // For the case above this use the just added case as the
                    // `orelse` branch, this convert the match statement to
                    // (essentially) a bunch of if statements.
                    orelse_after_block = blocks.len() - 1;
                }
                // TODO: currently we don't include the lines before the match
                // statement in the block, unlike what we do for other
                // statements.
                continue;
            }
            Stmt::Raise(_) => {
                // TODO: this needs special handling within `try` and `with`
                // statements. For now we just terminate the execution, it's
                // possible it's continued in an `catch` or `finally` block,
                // possibly outside of the function.
                // Also see `Stmt::Assert` handling.
                NextBlock::Terminate
            }
            Stmt::Assert(stmt) => {
                // TODO: this needs special handling within `try` and `with`
                // statements. For now we just terminate the execution if the
                // assertion fails, it's possible it's continued in an `catch`
                // or `finally` block, possibly outside of the function.
                // Also see `Stmt::Raise` handling.
                let next = force_next_block_index(blocks);
                let orelse = fake_exception_block_index(blocks);
                NextBlock::If {
                    condition: Condition::Test(&stmt.test),
                    next,
                    orelse,
                }
            }
            // The tough branches are done, here is an easy one.
            Stmt::Return(_) => NextBlock::Terminate,
        };

        // Include any statements in the block that don't divert the control flow.
        let mut start = i;
        let end = i + 1;
        while stmts_iter
            .next_if(|(_, stmt)| !is_control_flow_stmt(stmt))
            .is_some()
        {
            start -= 1;
        }

        let block = BasicBlock {
            stmts: &stmts[start..end],
            next,
        };
        blocks.push(block);
        after = Some(blocks.len() - 1);
    }
}

/// Handle a loop block, such as a `while`, `for` or `async for` statement.
fn loop_block<'stmt>(
    blocks: &mut Vec<BasicBlock<'stmt>>,
    condition: Condition<'stmt>,
    body: &'stmt [Stmt],
    orelse: &'stmt [Stmt],
    after: Option<BlockIndex>,
) -> NextBlock<'stmt> {
    let after_block = maybe_next_block_index(blocks, || orelse.is_empty());
    // NOTE: a while loop's body must not be empty, so we can safely
    // create at least one block from it.
    debug_assert!(!body.is_empty());
    create_blocks(blocks, body, after);
    let next = blocks.len() - 1;
    let orelse = append_blocks_if_not_empty(blocks, orelse, after_block);
    // `create_blocks` always continues to the next block by
    // default. However in a while loop we want to continue with the
    // while block (we're about to create) to create the loop.
    // NOTE: `blocks.len()` is an invalid index at time of creation
    // as it points to the block which we're about to create.
    change_next_block(blocks, next, after_block, blocks.len(), |block| {
        // For `break` statements we don't want to continue with the
        // loop, but instead with the statement after the loop (i.e.
        // not change anything).
        !block.stmts.last().map_or(false, Stmt::is_break_stmt)
    });
    NextBlock::If {
        condition,
        next,
        orelse,
    }
}

/// Handle a single match case.
///
/// `next_after_block` is the block *after* the entire match statement that is
/// taken after this case is taken.
/// `orelse_after_block` is the next match case (or the block after the match
/// statement if this is the last case).
fn match_case<'stmt>(
    blocks: &mut Vec<BasicBlock<'stmt>>,
    match_stmt: &'stmt Stmt,
    subject: &'stmt Expr,
    case: &'stmt MatchCase,
    next_after_block: BlockIndex,
    orelse_after_block: BlockIndex,
) -> BasicBlock<'stmt> {
    // FIXME: this is not ideal, we want to only use the `case` statement here,
    // but that is type `MatchCase`, not `Stmt`. For now we'll point to the
    // entire match statement.
    let stmts = std::slice::from_ref(match_stmt);
    let next_block_index = if case.body.is_empty() {
        next_after_block
    } else {
        let from = blocks.len().saturating_sub(1);
        let next = append_blocks(blocks, &case.body, Some(next_after_block));
        change_next_block(blocks, next, from, next_after_block, |_| true);
        next
    };
    // TODO: handle named arguments, e.g.
    // ```python
    // match $subjet:
    //   case $binding:
    //     print($binding)
    // ```
    // These should also return `NextBlock::Always`.
    let next = if is_wildcard(case) {
        // Wildcard case is always taken.
        NextBlock::Always(next_block_index)
    } else {
        NextBlock::If {
            condition: Condition::Match { subject, case },
            next: next_block_index,
            orelse: orelse_after_block,
        }
    };
    BasicBlock { stmts, next }
}

/// Returns true if `pattern` is a wildcard (`_`) pattern.
fn is_wildcard(pattern: &MatchCase) -> bool {
    pattern.guard.is_none()
        && matches!(&pattern.pattern, Pattern::MatchAs(PatternMatchAs { pattern, name, .. }) if pattern.is_none() && name.is_none())
}

/// Calls [`create_blocks`] and returns this first block reached (i.e. the last
/// block).
fn append_blocks<'stmt>(
    blocks: &mut Vec<BasicBlock<'stmt>>,
    stmts: &'stmt [Stmt],
    after: Option<BlockIndex>,
) -> BlockIndex {
    create_blocks(blocks, stmts, after);
    blocks.len() - 1
}

/// If `stmts` is not empty this calls [`create_blocks`] and returns this first
/// block reached (i.e. the last block). If `stmts` is empty this returns
/// `after` and doesn't change `blocks`.
fn append_blocks_if_not_empty<'stmt>(
    blocks: &mut Vec<BasicBlock<'stmt>>,
    stmts: &'stmt [Stmt],
    after: BlockIndex,
) -> BlockIndex {
    if stmts.is_empty() {
        after // Empty body, continue with block `after` it.
    } else {
        append_blocks(blocks, stmts, Some(after))
    }
}

/// Select the next block from `blocks` unconditonally.
fn unconditional_next_block(blocks: &[BasicBlock<'_>]) -> NextBlock<'static> {
    // Either we continue with the next block (that is the last block `blocks`).
    // Or it's the last statement, thus we terminate.
    blocks
        .len()
        .checked_sub(1)
        .map_or(NextBlock::Terminate, NextBlock::Always)
}

/// Select the next block index from `blocks`. If there is no next block it will
/// add a fake/empty block.
fn force_next_block_index(blocks: &mut Vec<BasicBlock<'_>>) -> BlockIndex {
    maybe_next_block_index(blocks, || true)
}

/// Select the next block index from `blocks`. If there is no next block it will
/// add a fake/empty block if `condition` returns true. If `condition` returns
/// false the returned index may not be used.
fn maybe_next_block_index(
    blocks: &mut Vec<BasicBlock<'_>>,
    condition: impl FnOnce() -> bool,
) -> BlockIndex {
    // Either we continue with the next block (that is the last block in `blocks`).
    if let Some(idx) = blocks.len().checked_sub(1) {
        idx
    } else if condition() {
        // Or if there are no blocks, but need one based on `condition` than we
        // add a fake end block.
        blocks.push(BasicBlock::EMPTY);
        0
    } else {
        // NOTE: invalid, but because `condition` returned false this shouldn't
        // be used. This only used as an optimisation to avoid adding fake end
        // blocks.
        usize::MAX
    }
}

/// Returns a block index for a fake exception block in `blocks`.
fn fake_exception_block_index(blocks: &mut Vec<BasicBlock<'_>>) -> BlockIndex {
    for (i, block) in blocks.iter().enumerate() {
        if block.is_exception() {
            return i;
        }
    }
    blocks.push(BasicBlock::EXCEPTION);
    blocks.len() - 1
}

/// Change the next basic block for the block, or chain of blocks, in index
/// `fixup_index` from `from` to `to`.
///
/// This doesn't change the target if it's `NextBlock::Terminate`.
fn change_next_block(
    blocks: &mut Vec<BasicBlock<'_>>,
    mut fixup_index: BlockIndex,
    from: BlockIndex,
    to: BlockIndex,
    check_condition: impl Fn(&BasicBlock) -> bool + Copy,
) {
    /// Check if we found our target and if `check_condition` is met.
    fn is_target(
        block: &BasicBlock<'_>,
        got: BlockIndex,
        expected: BlockIndex,
        check_condition: impl Fn(&BasicBlock) -> bool,
    ) -> bool {
        got == expected && check_condition(block)
    }

    loop {
        match &blocks[fixup_index].next {
            NextBlock::Always(next) => {
                let next = *next;
                if is_target(&blocks[fixup_index], next, from, check_condition) {
                    // Found our target, change it.
                    blocks[fixup_index].next = NextBlock::Always(to);
                }
                return;
            }
            NextBlock::If {
                condition,
                next,
                orelse,
            } => {
                let idx = fixup_index;
                let condition = condition.clone();
                let next = *next;
                let orelse = *orelse;
                let new_next = if is_target(&blocks[idx], next, from, check_condition) {
                    // Found our target in the next branch, change it (below).
                    Some(to)
                } else {
                    // Follow the chain.
                    fixup_index = next;
                    None
                };

                let new_orelse = if is_target(&blocks[idx], orelse, from, check_condition) {
                    // Found our target in the else branch, change it (below).
                    Some(to)
                } else if new_next.is_none() {
                    // If we done with the next branch we only continue with the
                    // else branch.
                    fixup_index = orelse;
                    None
                } else {
                    // If we're not done with the next and else branches we need
                    // to deal with the else branch before deal with the next
                    // branch (in the next iteration).
                    change_next_block(blocks, orelse, from, to, check_condition);
                    None
                };

                let (next, orelse) = match (new_next, new_orelse) {
                    (Some(new_next), Some(new_orelse)) => (new_next, new_orelse),
                    (Some(new_next), None) => (new_next, orelse),
                    (None, Some(new_orelse)) => (next, new_orelse),
                    (None, None) => continue, // Not changing anything.
                };

                blocks[idx].next = NextBlock::If {
                    condition,
                    next,
                    orelse,
                };
            }
            NextBlock::Terminate => return,
        }
    }
}

/// Returns true if `stmts` need a next block, false otherwise.
fn needs_next_block(stmts: &[Stmt]) -> bool {
    // No statements, we automatically continue with the next block.
    let Some(last) = stmts.last() else { return true; };

    match last {
        Stmt::Return(_) | Stmt::Raise(_) => false,
        Stmt::If(stmt) => needs_next_block(&stmt.body) || needs_next_block(&stmt.orelse),
        Stmt::FunctionDef(_)
        | Stmt::AsyncFunctionDef(_)
        | Stmt::Import(_)
        | Stmt::ImportFrom(_)
        | Stmt::ClassDef(_)
        | Stmt::Global(_)
        | Stmt::Nonlocal(_)
        | Stmt::Delete(_)
        | Stmt::Assign(_)
        | Stmt::AugAssign(_)
        | Stmt::AnnAssign(_)
        | Stmt::Expr(_)
        | Stmt::Pass(_)
        // TODO: check below.
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::For(_)
        | Stmt::AsyncFor(_)
        | Stmt::While(_)
        | Stmt::With(_)
        | Stmt::AsyncWith(_)
        | Stmt::Match(_)
        | Stmt::Try(_)
        | Stmt::TryStar(_)
        | Stmt::Assert(_) => true,
    }
}

/// Returns true if `stmt` contains a control flow statement, e.g. an `if` or
/// `return` statement.
fn is_control_flow_stmt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::FunctionDef(_)
        | Stmt::AsyncFunctionDef(_)
        | Stmt::Import(_)
        | Stmt::ImportFrom(_)
        | Stmt::ClassDef(_)
        | Stmt::Global(_)
        | Stmt::Nonlocal(_)
        | Stmt::Delete(_)
        | Stmt::Assign(_)
        | Stmt::AugAssign(_)
        | Stmt::AnnAssign(_)
        | Stmt::Expr(_)
        | Stmt::Pass(_) => false,
        Stmt::Return(_)
        | Stmt::For(_)
        | Stmt::AsyncFor(_)
        | Stmt::While(_)
        | Stmt::If(_)
        | Stmt::With(_)
        | Stmt::AsyncWith(_)
        | Stmt::Match(_)
        | Stmt::Raise(_)
        | Stmt::Try(_)
        | Stmt::TryStar(_)
        | Stmt::Assert(_)
        | Stmt::Break(_)
        | Stmt::Continue(_) => true,
    }
}

/// Type to create a Mermaid graph.
///
/// To learn amount Mermaid see <https://mermaid.js.org/intro>, for the syntax
/// see <https://mermaid.js.org/syntax/flowchart.html>.
struct MermaidGraph<'stmt, 'source> {
    graph: &'stmt BasicBlocks<'stmt>,
    source: &'source str,
    /// The range of the `source` code which contains the function, used to
    /// print the source code.
    range: TextRange,
}

impl<'stmt, 'source> fmt::Display for MermaidGraph<'stmt, 'source> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Include the entire source code for debugging purposes.
        writeln!(f, "%% Source code:")?;
        for line in self.source[self.range].lines() {
            writeln!(f, "%% {line}")?;
        }
        if !self.source.is_empty() {
            writeln!(f)?;
        }

        // Flowchart type of graph, top down.
        writeln!(f, "flowchart TD")?;

        // List all blocks.
        writeln!(f, "  start((\"Start\"))")?;
        writeln!(f, "  return((\"End\"))")?;
        for (i, block) in self.graph.blocks.iter().enumerate() {
            let (open, close) = if block.is_sentinel() {
                ("[[", "]]")
            } else {
                ("[", "]")
            };
            write!(f, "  block{i}{open}\"")?;
            if block.is_empty() {
                write!(f, "`*(empty)*`")?;
            } else if block.is_exception() {
                write!(f, "Exception raised")?;
            } else {
                for stmt in block.stmts {
                    let code_line = &self.source[stmt.range()].trim();
                    mermaid_write_qouted_str(f, code_line)?;
                    write!(f, "\\n")?;
                }
            }
            writeln!(f, "\"{close}")?;
        }
        writeln!(f)?;

        // Then link all the blocks.
        writeln!(f, "  start --> block{}", self.graph.blocks.len() - 1)?;
        for (i, block) in self.graph.blocks.iter().enumerate().rev() {
            match &block.next {
                NextBlock::Always(target) => writeln!(f, "  block{i} --> block{target}")?,
                NextBlock::If {
                    condition,
                    next,
                    orelse,
                } => {
                    let condition_code = &self.source[condition.range()].trim();
                    writeln!(f, "  block{i} -- \"{condition_code}\" --> block{next}")?;
                    writeln!(f, "  block{i} -- \"else\" --> block{orelse}")?;
                }
                NextBlock::Terminate => writeln!(f, "  block{i} --> return")?,
            }
        }

        Ok(())
    }
}

/// Escape double qoutes (`"`) in `value` using `#quot;`.
fn mermaid_write_qouted_str(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    let mut parts = value.split('"');
    if let Some(v) = parts.next() {
        write!(f, "{v}")?;
    }
    for v in parts {
        write!(f, "#quot;{v}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use rustpython_parser::{parse, Mode};
    use test_case::test_case;

    use crate::rules::ruff::rules::unreachable::{BasicBlocks, MermaidGraph, NextBlock};

    #[test_case("simple.py")]
    #[test_case("if.py")]
    #[test_case("while.py")]
    #[test_case("for.py")]
    #[test_case("async-for.py")]
    //#[test_case("try.py")] // TODO.
    #[test_case("raise.py")]
    #[test_case("assert.py")]
    #[test_case("match.py")]
    fn control_flow_graph(filename: &str) {
        let path = PathBuf::from_iter(["resources/test/fixtures/control-flow-graph", filename]);
        let source = fs::read_to_string(&path).expect("failed to read file");
        let stmts = parse(&source, Mode::Module, filename)
            .unwrap_or_else(|err| panic!("failed to parse source: '{source}': {err}"))
            .expect_module()
            .body;

        for (i, stmts) in stmts.into_iter().enumerate() {
            let func = stmts.function_def_stmt().expect("statement not a function");

            let got = BasicBlocks::from(&*func.body);
            // Basic sanity checks.
            assert!(!got.blocks.is_empty(), "basic blocks should never be empty");
            assert!(
                got.blocks.first().unwrap().next == NextBlock::Terminate,
                "first block should always terminate"
            );
            // All block index should be valid.
            let valid = got.blocks.len();
            for block in &got.blocks {
                match block.next {
                    NextBlock::Always(index) => assert!(index <= valid, "invalid block index"),
                    NextBlock::If { next, orelse, .. } => {
                        assert!(next <= valid, "invalid next block index");
                        assert!(orelse <= valid, "invalid orelse block index");
                    }
                    NextBlock::Terminate => {}
                }
            }

            let got_mermaid = MermaidGraph {
                graph: &got,
                source: &source,
                range: func.range,
            }
            .to_string();
            let snapshot = format!("{filename}_{i}");
            insta::with_settings!({ omit_expression => true }, {
                insta::assert_snapshot!(snapshot, got_mermaid);
            });
        }
    }
}
