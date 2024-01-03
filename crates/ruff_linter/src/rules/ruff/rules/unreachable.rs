use std::{fmt, iter, usize};

use log::error;
use ruff_python_ast::{
    Expr, ExprBooleanLiteral, Identifier, MatchCase, Pattern, PatternMatchAs, PatternMatchOr, Stmt,
    StmtFor, StmtMatch, StmtReturn, StmtTry, StmtWhile, StmtWith,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_index::{IndexSlice, IndexVec};
use ruff_macros::{derive_message_formats, newtype_index, violation};

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
    let mut reached_map = Bitmap::with_capacity(basic_blocks.len());

    if let Some(start_index) = basic_blocks.start_index() {
        mark_reached(&mut reached_map, &basic_blocks.blocks, start_index);
    }

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
    fn set(&mut self, idx: BlockIndex) -> bool {
        let bits_index = (idx.as_u32() / usize::BITS) as usize;
        let shift = idx.as_u32() % usize::BITS;
        if (self.bits[bits_index] & (1 << shift)) == 0 {
            self.bits[bits_index] |= 1 << shift;
            false
        } else {
            true
        }
    }

    /// Returns an iterator of all unset indices.
    fn unset(&self) -> impl Iterator<Item = BlockIndex> + '_ {
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
                return Some(BlockIndex::from_usize(
                    (index * usize::BITS as usize) + shift - 1,
                ));
            }
        })
    }
}

/// Set bits in `reached_map` for all blocks that are reached in `blocks`
/// starting with block at index `idx`.
fn mark_reached(
    reached_map: &mut Bitmap,
    blocks: &IndexSlice<BlockIndex, BasicBlock<'_>>,
    start_index: BlockIndex,
) {
    let mut idx = start_index;

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
            Expr::BooleanLiteral(ExprBooleanLiteral { value, .. }) => Some(*value),
            _ => None,
        },
        Condition::Iterator(_) => None,
        Condition::Match { .. } => None,
    }
}

/// Index into [`BasicBlocks::blocks`].
#[newtype_index]
#[derive(PartialOrd, Ord)]
struct BlockIndex;

/// Collection of basic block.
#[derive(Debug, PartialEq)]
struct BasicBlocks<'stmt> {
    /// # Notes
    ///
    /// The order of these block is unspecified. However it's guaranteed that
    /// the last block is the first statement in the function and the first
    /// block is the last statement. The block are more or less in reverse
    /// order, but it gets fussy around control flow statements (e.g. `while`
    /// statements).
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
    /// Finally `BasicBlock` can also be a sentinel node, see the associated
    /// constants of [`BasicBlock`].
    blocks: IndexVec<BlockIndex, BasicBlock<'stmt>>,
}

impl BasicBlocks<'_> {
    fn len(&self) -> usize {
        self.blocks.len()
    }

    fn start_index(&self) -> Option<BlockIndex> {
        self.blocks.indices().last()
    }
}

impl<'stmt> From<&'stmt [Stmt]> for BasicBlocks<'stmt> {
    /// # Notes
    ///
    /// This assumes that `stmts` is a function body.
    fn from(stmts: &'stmt [Stmt]) -> BasicBlocks<'stmt> {
        let mut blocks = BasicBlocksBuilder::with_capacity(stmts.len());

        blocks.create_blocks(stmts, None);

        blocks.finish()
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

impl<'stmt> BasicBlock<'stmt> {
    /// A sentinel block indicating an empty termination block.
    const EMPTY: BasicBlock<'static> = BasicBlock {
        stmts: &[],
        next: NextBlock::Terminate,
    };

    /// A sentinel block indicating an exception was raised.
    const EXCEPTION: BasicBlock<'static> = BasicBlock {
        stmts: &[Stmt::Return(StmtReturn {
            range: TextRange::new(TextSize::new(0), TextSize::new(0)),
            value: None,
        })],
        next: NextBlock::Terminate,
    };

    /// Return true if the block is a sentinel or fake block.
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

/// Handle a loop block, such as a `while`, `for` or `async for` statement.
fn loop_block<'stmt>(
    blocks: &mut BasicBlocksBuilder<'stmt>,
    condition: Condition<'stmt>,
    body: &'stmt [Stmt],
    orelse: &'stmt [Stmt],
    after: Option<BlockIndex>,
) -> NextBlock<'stmt> {
    let after_block = blocks.maybe_next_block_index(after, || orelse.is_empty());
    // NOTE: a while loop's body must not be empty, so we can safely
    // create at least one block from it.
    let last_statement_index = blocks.append_blocks(body, after);
    let last_orelse_statement = blocks.append_blocks_if_not_empty(orelse, after_block);
    // `create_blocks` always continues to the next block by
    // default. However in a while loop we want to continue with the
    // while block (we're about to create) to create the loop.
    // NOTE: `blocks.len()` is an invalid index at time of creation
    // as it points to the block which we're about to create.
    blocks.change_next_block(
        last_statement_index,
        after_block,
        blocks.blocks.next_index(),
        |block| {
            // For `break` statements we don't want to continue with the
            // loop, but instead with the statement after the loop (i.e.
            // not change anything).
            !block.stmts.last().is_some_and(Stmt::is_break_stmt)
        },
    );
    NextBlock::If {
        condition,
        next: last_statement_index,
        orelse: last_orelse_statement,
    }
}

/// Handle a single match case.
///
/// `next_after_block` is the block *after* the entire match statement that is
/// taken after this case is taken.
/// `orelse_after_block` is the next match case (or the block after the match
/// statement if this is the last case).
fn match_case<'stmt>(
    blocks: &mut BasicBlocksBuilder<'stmt>,
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
        let from = blocks.last_index();
        let last_statement_index = blocks.append_blocks(&case.body, Some(next_after_block));
        if let Some(from) = from {
            blocks.change_next_block(last_statement_index, from, next_after_block, |_| true);
        }
        last_statement_index
    };
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

/// Returns true if the [`MatchCase`] is a wildcard pattern.
fn is_wildcard(pattern: &MatchCase) -> bool {
    /// Returns true if the [`Pattern`] is a wildcard pattern.
    fn is_wildcard_pattern(pattern: &Pattern) -> bool {
        match pattern {
            Pattern::MatchValue(_)
            | Pattern::MatchSingleton(_)
            | Pattern::MatchSequence(_)
            | Pattern::MatchMapping(_)
            | Pattern::MatchClass(_)
            | Pattern::MatchStar(_) => false,
            Pattern::MatchAs(PatternMatchAs { pattern, .. }) => pattern.is_none(),
            Pattern::MatchOr(PatternMatchOr { patterns, .. }) => {
                patterns.iter().all(is_wildcard_pattern)
            }
        }
    }

    pattern.guard.is_none() && is_wildcard_pattern(&pattern.pattern)
}

#[derive(Debug, Default)]
struct BasicBlocksBuilder<'stmt> {
    blocks: IndexVec<BlockIndex, BasicBlock<'stmt>>,
}

impl<'stmt> BasicBlocksBuilder<'stmt> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            blocks: IndexVec::with_capacity(capacity),
        }
    }

    /// Creates basic blocks from `stmts` and appends them to `blocks`.
    fn create_blocks(
        &mut self,
        stmts: &'stmt [Stmt],
        mut after: Option<BlockIndex>,
    ) -> Option<BlockIndex> {
        // We process the statements in reverse so that we can always point to the
        // next block (as that should always be processed).
        let mut stmts_iter = stmts.iter().enumerate().rev().peekable();
        while let Some((i, stmt)) = stmts_iter.next() {
            let next = match stmt {
                // Statements that continue to the next statement after execution.
                Stmt::FunctionDef(_)
                | Stmt::Import(_)
                | Stmt::ImportFrom(_)
                | Stmt::ClassDef(_)
                | Stmt::Global(_)
                | Stmt::Nonlocal(_)
                | Stmt::Delete(_)
                | Stmt::Assign(_)
                | Stmt::AugAssign(_)
                | Stmt::AnnAssign(_)
                | Stmt::Break(_)
                | Stmt::TypeAlias(_)
                | Stmt::IpyEscapeCommand(_)
                | Stmt::Pass(_) => self.unconditional_next_block(after),
                Stmt::Continue(_) => {
                    // NOTE: the next branch gets fixed up in `change_next_block`.
                    self.unconditional_next_block(after)
                }
                // Statements that (can) divert the control flow.
                Stmt::If(stmt_if) => {
                    let after_consequent_block =
                        self.maybe_next_block_index(after, || needs_next_block(&stmt_if.body));
                    let after_alternate_block = self.maybe_next_block_index(after, || {
                        stmt_if
                            .elif_else_clauses
                            .last()
                            .map_or(true, |clause| needs_next_block(&clause.body))
                    });

                    let consequent =
                        self.append_blocks_if_not_empty(&stmt_if.body, after_consequent_block);

                    // Block ID of the next elif or else clause.
                    let mut next_branch = after_alternate_block;

                    for clause in stmt_if.elif_else_clauses.iter().rev() {
                        let consequent =
                            self.append_blocks_if_not_empty(&clause.body, after_consequent_block);

                        next_branch = if let Some(test) = &clause.test {
                            let next = NextBlock::If {
                                condition: Condition::Test(test),
                                next: consequent,
                                orelse: next_branch,
                            };
                            let stmts = std::slice::from_ref(stmt);
                            let block = BasicBlock { stmts, next };
                            self.blocks.push(block)
                        } else {
                            consequent
                        };
                    }

                    NextBlock::If {
                        condition: Condition::Test(&stmt_if.test),
                        next: consequent,
                        orelse: next_branch,
                    }
                }
                Stmt::While(StmtWhile {
                    test: condition,
                    body,
                    orelse,
                    ..
                }) => loop_block(self, Condition::Test(condition), body, orelse, after),
                Stmt::For(StmtFor {
                    iter: condition,
                    body,
                    orelse,
                    ..
                }) => loop_block(self, Condition::Iterator(condition), body, orelse, after),
                Stmt::Try(StmtTry {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                    ..
                }) => {
                    // TODO: handle `try` statements. The `try` control flow is very
                    // complex, what blocks are and aren't taken and from which
                    // block the control flow is actually returns is **very**
                    // specific to the contents of the block. Read
                    // <https://docs.python.org/3/reference/compound_stmts.html#the-try-statement>
                    // very carefully.
                    // For now we'll skip over it.
                    let _ = (body, handlers, orelse, finalbody); // Silence unused code warnings.
                    self.unconditional_next_block(after)
                }
                Stmt::With(StmtWith { items, body, .. }) => {
                    // TODO: handle `with` statements, see
                    // <https://docs.python.org/3/reference/compound_stmts.html#the-with-statement>.
                    // I recommend to `try` statements first as `with` can desugar
                    // to a `try` statement.
                    // For now we'll skip over it.
                    let _ = (items, body); // Silence unused code warnings.
                    self.unconditional_next_block(after)
                }
                Stmt::Match(StmtMatch { subject, cases, .. }) => {
                    let next_after_block = self.maybe_next_block_index(after, || {
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
                        let block = match_case(
                            self,
                            stmt,
                            subject,
                            case,
                            next_after_block,
                            orelse_after_block,
                        );
                        // For the case above this use the just added case as the
                        // `orelse` branch, this convert the match statement to
                        // (essentially) a bunch of if statements.
                        orelse_after_block = self.blocks.push(block);
                    }
                    // TODO: currently we don't include the lines before the match
                    // statement in the block, unlike what we do for other
                    // statements.
                    after = Some(orelse_after_block);
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
                    let next = self.force_next_block_index();
                    let orelse = self.fake_exception_block_index();
                    NextBlock::If {
                        condition: Condition::Test(&stmt.test),
                        next,
                        orelse,
                    }
                }
                Stmt::Expr(stmt) => {
                    match &*stmt.value {
                        Expr::BoolOp(_)
                        | Expr::BinOp(_)
                        | Expr::UnaryOp(_)
                        | Expr::Dict(_)
                        | Expr::Set(_)
                        | Expr::Compare(_)
                        | Expr::Call(_)
                        | Expr::FString(_)
                        | Expr::StringLiteral(_)
                        | Expr::BytesLiteral(_)
                        | Expr::NumberLiteral(_)
                        | Expr::BooleanLiteral(_)
                        | Expr::NoneLiteral(_)
                        | Expr::EllipsisLiteral(_)
                        | Expr::Attribute(_)
                        | Expr::Subscript(_)
                        | Expr::Starred(_)
                        | Expr::Name(_)
                        | Expr::List(_)
                        | Expr::IpyEscapeCommand(_)
                        | Expr::Tuple(_)
                        | Expr::Slice(_) => self.unconditional_next_block(after),
                        // TODO: handle these expressions.
                        Expr::NamedExpr(_)
                        | Expr::Lambda(_)
                        | Expr::IfExp(_)
                        | Expr::ListComp(_)
                        | Expr::SetComp(_)
                        | Expr::DictComp(_)
                        | Expr::GeneratorExp(_)
                        | Expr::Await(_)
                        | Expr::Yield(_)
                        | Expr::YieldFrom(_) => self.unconditional_next_block(after),
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
            after = Some(self.blocks.push(block));
        }

        after
    }

    /// Calls [`create_blocks`] and returns this first block reached (i.e. the last
    /// block).
    fn append_blocks(&mut self, stmts: &'stmt [Stmt], after: Option<BlockIndex>) -> BlockIndex {
        assert!(!stmts.is_empty());
        self.create_blocks(stmts, after)
            .expect("Expect `create_blocks` to create a block if `stmts` is not empty")
    }

    /// If `stmts` is not empty this calls [`create_blocks`] and returns this first
    /// block reached (i.e. the last block). If `stmts` is empty this returns
    /// `after` and doesn't change `blocks`.
    fn append_blocks_if_not_empty(
        &mut self,
        stmts: &'stmt [Stmt],
        after: BlockIndex,
    ) -> BlockIndex {
        if stmts.is_empty() {
            after // Empty body, continue with block `after` it.
        } else {
            self.append_blocks(stmts, Some(after))
        }
    }

    /// Select the next block from `blocks` unconditionally.
    fn unconditional_next_block(&self, after: Option<BlockIndex>) -> NextBlock<'static> {
        if let Some(after) = after {
            return NextBlock::Always(after);
        }

        // Either we continue with the next block (that is the last block `blocks`).
        // Or it's the last statement, thus we terminate.
        self.blocks
            .last_index()
            .map_or(NextBlock::Terminate, NextBlock::Always)
    }

    /// Select the next block index from `blocks`. If there is no next block it will
    /// add a fake/empty block.
    fn force_next_block_index(&mut self) -> BlockIndex {
        self.maybe_next_block_index(None, || true)
    }

    /// Select the next block index from `blocks`. If there is no next block it will
    /// add a fake/empty block if `condition` returns true. If `condition` returns
    /// false the returned index may not be used.
    fn maybe_next_block_index(
        &mut self,
        after: Option<BlockIndex>,
        condition: impl FnOnce() -> bool,
    ) -> BlockIndex {
        if let Some(after) = after {
            // Next block is already determined.
            after
        } else if let Some(idx) = self.blocks.last_index() {
            // Otherwise we either continue with the next block (that is the last
            // block in `blocks`).
            idx
        } else if condition() {
            // Or if there are no blocks, but need one based on `condition` than we
            // add a fake end block.
            self.blocks.push(BasicBlock::EMPTY)
        } else {
            // NOTE: invalid, but because `condition` returned false this shouldn't
            // be used. This only used as an optimisation to avoid adding fake end
            // blocks.
            BlockIndex::MAX
        }
    }

    /// Returns a block index for a fake exception block in `blocks`.
    fn fake_exception_block_index(&mut self) -> BlockIndex {
        for (i, block) in self.blocks.iter_enumerated() {
            if block.is_exception() {
                return i;
            }
        }
        self.blocks.push(BasicBlock::EXCEPTION)
    }

    /// Change the next basic block for the block, or chain of blocks, in index
    /// `fixup_index` from `from` to `to`.
    ///
    /// This doesn't change the target if it's `NextBlock::Terminate`.
    fn change_next_block(
        &mut self,
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
            match self.blocks.get(fixup_index).map(|b| &b.next) {
                Some(NextBlock::Always(next)) => {
                    let next = *next;
                    if is_target(&self.blocks[fixup_index], next, from, check_condition) {
                        // Found our target, change it.
                        self.blocks[fixup_index].next = NextBlock::Always(to);
                    }
                    return;
                }
                Some(NextBlock::If {
                    condition,
                    next,
                    orelse,
                }) => {
                    let idx = fixup_index;
                    let condition = condition.clone();
                    let next = *next;
                    let orelse = *orelse;
                    let new_next = if is_target(&self.blocks[idx], next, from, check_condition) {
                        // Found our target in the next branch, change it (below).
                        Some(to)
                    } else {
                        // Follow the chain.
                        fixup_index = next;
                        None
                    };

                    let new_orelse = if is_target(&self.blocks[idx], orelse, from, check_condition)
                    {
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
                        self.change_next_block(orelse, from, to, check_condition);
                        None
                    };

                    let (next, orelse) = match (new_next, new_orelse) {
                        (Some(new_next), Some(new_orelse)) => (new_next, new_orelse),
                        (Some(new_next), None) => (new_next, orelse),
                        (None, Some(new_orelse)) => (next, new_orelse),
                        (None, None) => continue, // Not changing anything.
                    };

                    self.blocks[idx].next = NextBlock::If {
                        condition,
                        next,
                        orelse,
                    };
                }
                Some(NextBlock::Terminate) | None => return,
            }
        }
    }

    fn finish(mut self) -> BasicBlocks<'stmt> {
        if self.blocks.is_empty() {
            self.blocks.push(BasicBlock::EMPTY);
        }

        BasicBlocks {
            blocks: self.blocks,
        }
    }
}

impl<'stmt> std::ops::Deref for BasicBlocksBuilder<'stmt> {
    type Target = IndexSlice<BlockIndex, BasicBlock<'stmt>>;

    fn deref(&self) -> &Self::Target {
        &self.blocks
    }
}

impl<'stmt> std::ops::DerefMut for BasicBlocksBuilder<'stmt> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.blocks
    }
}

/// Returns true if `stmts` need a next block, false otherwise.
fn needs_next_block(stmts: &[Stmt]) -> bool {
    // No statements, we automatically continue with the next block.
    let Some(last) = stmts.last() else {
        return true;
    };

    match last {
        Stmt::Return(_) | Stmt::Raise(_) => false,
        Stmt::If(stmt) => needs_next_block(&stmt.body) || stmt.elif_else_clauses.last().map_or(true, |clause| needs_next_block(&clause.body)),
        Stmt::FunctionDef(_)
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
        | Stmt::TypeAlias(_)
        | Stmt::IpyEscapeCommand(_)
        // TODO: check below.
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::For(_)
        | Stmt::While(_)
        | Stmt::With(_)
        | Stmt::Match(_)
        | Stmt::Try(_)
        | Stmt::Assert(_) => true,
    }
}

/// Returns true if `stmt` contains a control flow statement, e.g. an `if` or
/// `return` statement.
fn is_control_flow_stmt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::FunctionDef(_)
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
        | Stmt::TypeAlias(_)
        | Stmt::IpyEscapeCommand(_)
        | Stmt::Pass(_) => false,
        Stmt::Return(_)
        | Stmt::For(_)
        | Stmt::While(_)
        | Stmt::If(_)
        | Stmt::With(_)
        | Stmt::Match(_)
        | Stmt::Raise(_)
        | Stmt::Try(_)
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
}

impl<'stmt, 'source> fmt::Display for MermaidGraph<'stmt, 'source> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
                    mermaid_write_quoted_str(f, code_line)?;
                    write!(f, "\\n")?;
                }
            }
            writeln!(f, "\"{close}")?;
        }
        writeln!(f)?;

        // Then link all the blocks.
        writeln!(f, "  start --> block{}", self.graph.blocks.len() - 1)?;
        for (i, block) in self.graph.blocks.iter_enumerated().rev() {
            let i = i.as_u32();
            match &block.next {
                NextBlock::Always(target) => {
                    writeln!(f, "  block{i} --> block{target}", target = target.as_u32())?;
                }
                NextBlock::If {
                    condition,
                    next,
                    orelse,
                } => {
                    let condition_code = &self.source[condition.range()].trim();
                    writeln!(
                        f,
                        "  block{i} -- \"{condition_code}\" --> block{next}",
                        next = next.as_u32()
                    )?;
                    writeln!(
                        f,
                        "  block{i} -- \"else\" --> block{orelse}",
                        orelse = orelse.as_u32()
                    )?;
                }
                NextBlock::Terminate => writeln!(f, "  block{i} --> return")?,
            }
        }

        Ok(())
    }
}

/// Escape double quotes (`"`) in `value` using `#quot;`.
fn mermaid_write_quoted_str(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
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

    use ruff_python_parser::{parse, Mode};
    use ruff_text_size::Ranged;
    use std::fmt::Write;
    use test_case::test_case;

    use crate::rules::ruff::rules::unreachable::{
        BasicBlocks, BlockIndex, MermaidGraph, NextBlock,
    };

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
        let source = fs::read_to_string(path).expect("failed to read file");
        let stmts = parse(&source, Mode::Module)
            .unwrap_or_else(|err| panic!("failed to parse source: '{source}': {err}"))
            .expect_module()
            .body;

        let mut output = String::new();

        for (i, stmts) in stmts.into_iter().enumerate() {
            let Some(func) = stmts.function_def_stmt() else {
                use std::io::Write;
                let _ = std::io::stderr().write_all(b"unexpected statement kind, ignoring");
                continue;
            };

            let got = BasicBlocks::from(&*func.body);
            // Basic sanity checks.
            assert!(!got.blocks.is_empty(), "basic blocks should never be empty");
            assert_eq!(
                got.blocks.first().unwrap().next,
                NextBlock::Terminate,
                "first block should always terminate"
            );

            let got_mermaid = MermaidGraph {
                graph: &got,
                source: &source,
            };

            // All block index should be valid.
            let valid = BlockIndex::from_usize(got.blocks.len());
            for block in &got.blocks {
                match block.next {
                    NextBlock::Always(index) => assert!(index < valid, "invalid block index"),
                    NextBlock::If { next, orelse, .. } => {
                        assert!(next < valid, "invalid next block index");
                        assert!(orelse < valid, "invalid orelse block index");
                    }
                    NextBlock::Terminate => {}
                }
            }

            writeln!(
                output,
                "## Function {i}\n### Source\n```python\n{}\n```\n\n### Control Flow Graph\n```mermaid\n{}```\n",
                &source[func.range()],
                got_mermaid
            )
            .unwrap();
        }

        insta::with_settings!({
            omit_expression => true,
            input_file => filename,
            description => "This is a Mermaid graph. You can use https://mermaid.live to visualize it as a diagram."
        }, {
            insta::assert_snapshot!(format!("{filename}.md"), output);
        });
    }
}
