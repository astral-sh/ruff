use std::cmp;

use ruff_python_ast::{
    self as ast, Expr, ExprBooleanLiteral, Identifier, MatchCase, Pattern, PatternMatchAs,
    PatternMatchOr, Stmt, StmtContinue, StmtFor, StmtMatch, StmtReturn, StmtTry, StmtWhile,
    StmtWith,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_index::{IndexSlice, IndexVec};
use ruff_macros::{derive_message_formats, newtype_index, ViolationMetadata};

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

pub(crate) fn in_function(name: &Identifier, body: &[Stmt]) -> Vec<Diagnostic> {
    // Create basic code blocks from the body.
    let mut basic_blocks = BasicBlocks::from(body);
    if let Some(start_index) = basic_blocks.start_index() {
        mark_reachable(&mut basic_blocks.blocks, start_index);
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Combine sequential unreachable blocks
    let mut blocks = basic_blocks.blocks.raw;
    blocks.sort_by_key(|a| a.start().to_u32());
    let mut start = None;
    let mut end = None;
    for block in blocks {
        if block.is_sentinel() {
            continue;
        }

        if block.reachable {
            // At each reachable block, create a violation for all the
            // unreachable blocks encountered since the last reachable
            // block.
            if let Some(start_index) = start {
                if let Some(end_index) = end {
                    // TODO: add more information to the diagnostic.
                    // Maybe something to indicate the code flow and where it
                    // prevents this block from being reached for example.
                    let diagnostic = Diagnostic::new(
                        UnreachableCode {
                            name: name.as_str().to_owned(),
                        },
                        TextRange::new(start_index, end_index),
                    );
                    diagnostics.push(diagnostic);

                    start = None;
                    end = None;
                }
            }
        } else {
            if let Some(end_index) = end {
                end = Some(cmp::max(block.end(), end_index));
            } else {
                start = Some(block.start());
                end = Some(block.end());
            }
        }
    }
    if let Some(start_index) = start {
        if let Some(end_index) = end {
            let diagnostic = Diagnostic::new(
                UnreachableCode {
                    name: name.as_str().to_owned(),
                },
                TextRange::new(start_index, end_index),
            );
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

/// Set bits in `reached_map` for all blocks that are reached in `blocks`
/// starting with block at index `idx`.
fn mark_reachable(blocks: &mut IndexSlice<BlockIndex, BasicBlock<'_>>, start_index: BlockIndex) {
    let mut idx = start_index;

    loop {
        if blocks[idx].reachable {
            return; // Block already visited, no needed to do it again.
        }
        blocks[idx].reachable = true;

        match &blocks[idx].next {
            NextBlock::Always(next) => idx = *next,
            NextBlock::If {
                condition,
                next,
                orelse,
                ..
            } => {
                match taken(condition) {
                    Some(true) => idx = *next,    // Always taken.
                    Some(false) => idx = *orelse, // Never taken.
                    None => {
                        // Don't know, both branches might be taken.
                        idx = *next;
                        mark_reachable(blocks, *orelse);
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
/// `None`, e.g. `if i == 100`.
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
        Condition::Except(_) => None,
        Condition::MaybeRaised => None,
    }
}

/// Index into [`BasicBlocks::blocks`].
#[newtype_index]
#[derive(PartialOrd, Ord)]
struct BlockIndex;

#[derive(Debug, PartialEq, Clone)]
enum BasicBlockKind {
    Generic,
    Empty,
    Exception,
    LoopContinue,
}

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
    /// For loop blocks (e.g. `while` and `for`), the end of the body will
    /// point to the loop block again (to create the loop). However an oddity
    /// here is that this block might contain statements before the loop
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
#[derive(Clone, Debug, PartialEq)]
struct BasicBlock<'stmt> {
    stmts: &'stmt [Stmt],
    next: NextBlock<'stmt>,
    reachable: bool,
    kind: BasicBlockKind,
}

/// Edge between basic blocks (in the control-flow graph).
#[derive(Clone, Debug, PartialEq)]
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
        /// Exit block. None indicates Terminate.
        /// The purpose of the `exit` block is to facilitate post processing
        /// steps. When iterating over `if` or `try` bodies it is necessary
        /// to know when we have exited the body. To avoid reprocessing blocks.
        ///
        /// For example:
        /// ```python
        /// while True:    # block 0
        ///     if True:   # block 1
        ///         x = 2  # block 2
        ///     y = 2      # block 3
        ///     z = 2      # block 4
        /// ```
        ///
        /// Recursive processing will proceed as follows:
        /// block 0 -> block 1 -> block 2 -> block 3 -> block 4 -> Terminate
        ///                    -> block 3 -> block 4 -> Terminate
        ///
        /// To avoid repeated work we remember that the `if` body exits on
        /// block 3, so the recursion can be terminated.
        exit: Option<BlockIndex>,
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
    /// Iterator for `for` statements, e.g. for `for i in range(10)` this will be
    /// `range(10)`.
    Iterator(&'stmt Expr),
    Match {
        /// `match $subject`.
        subject: &'stmt Expr,
        /// `case $case`, include pattern, guard, etc.
        case: &'stmt MatchCase,
    },
    /// Exception was raised and caught by `except` clause.
    /// If the raised `Exception` matches the one caught by the `except`
    /// then execute the `except` body, otherwise go to the next `except`.
    ///
    /// The `stmt` is the exception caught by the `except`.
    Except(&'stmt Expr),
    /// Exception was raised in a `try` block.
    /// This condition cannot be evaluated since it's impossible to know
    /// (in most cases) if an exception will be raised. So both paths
    /// (raise and not-raise) are assumed to be taken.
    MaybeRaised,
}

impl Ranged for Condition<'_> {
    fn range(&self) -> TextRange {
        match self {
            Condition::Test(expr) | Condition::Iterator(expr) | Condition::Except(expr) => {
                expr.range()
            }
            // The case of the match statement, without the body.
            Condition::Match { subject: _, case } => TextRange::new(
                case.start(),
                case.guard
                    .as_ref()
                    .map_or(case.pattern.end(), |guard| guard.end()),
            ),
            Condition::MaybeRaised => TextRange::new(TextSize::new(0), TextSize::new(0)),
        }
    }
}

impl<'stmt> BasicBlock<'stmt> {
    fn new(stmts: &'stmt [Stmt], next: NextBlock<'stmt>) -> Self {
        Self {
            stmts,
            next,
            reachable: false,
            kind: BasicBlockKind::Generic,
        }
    }

    /// A sentinel block indicating an empty termination block.
    const EMPTY: BasicBlock<'static> = BasicBlock {
        stmts: &[],
        next: NextBlock::Terminate,
        reachable: false,
        kind: BasicBlockKind::Empty,
    };

    /// A sentinel block indicating an exception was raised.
    /// This is useful for redirecting flow within `try` blocks.
    const EXCEPTION: BasicBlock<'static> = BasicBlock {
        stmts: &[Stmt::Return(StmtReturn {
            range: TextRange::new(TextSize::new(0), TextSize::new(0)),
            value: None,
        })],
        next: NextBlock::Terminate,
        reachable: false,
        kind: BasicBlockKind::Exception,
    };

    /// A sentinel block indicating a loop will restart.
    /// This is useful for redirecting flow within `while` and
    /// `for` blocks.
    const LOOP_CONTINUE: BasicBlock<'static> = BasicBlock {
        stmts: &[Stmt::Continue(StmtContinue {
            range: TextRange::new(TextSize::new(0), TextSize::new(0)),
        })],
        next: NextBlock::Terminate, // This must be updated dynamically
        reachable: false,
        kind: BasicBlockKind::LoopContinue,
    };

    /// Return true if the block is a sentinel or fake block.
    fn is_sentinel(&self) -> bool {
        self.is_empty() || self.is_exception() || self.is_loop_continue()
    }

    /// Returns true if `self` is an `EMPTY` block.
    fn is_empty(&self) -> bool {
        matches!(self.kind, BasicBlockKind::Empty)
    }

    /// Returns true if `self` is an `EXCEPTION` block.
    fn is_exception(&self) -> bool {
        matches!(self.kind, BasicBlockKind::Exception)
    }

    /// Returns true if `self` is a `LOOP_CONTINUE` block.
    fn is_loop_continue(&self) -> bool {
        matches!(self.kind, BasicBlockKind::LoopContinue)
    }
}

impl Ranged for BasicBlock<'_> {
    fn range(&self) -> TextRange {
        let Some(first) = self.stmts.first() else {
            return TextRange::new(TextSize::new(0), TextSize::new(0));
        };
        let Some(last) = self.stmts.last() else {
            return TextRange::new(TextSize::new(0), TextSize::new(0));
        };
        TextRange::new(first.start(), last.end())
    }
}

/// Handle a loop block, such as a `while`, `for`, or `async for` statement.
fn loop_block<'stmt>(
    blocks: &mut BasicBlocksBuilder<'stmt>,
    condition: Condition<'stmt>,
    body: &'stmt [Stmt],
    orelse: &'stmt [Stmt],
    after: Option<BlockIndex>,
) -> NextBlock<'stmt> {
    let after_block = blocks.find_next_block_index(after);
    let last_orelse_statement = blocks.append_blocks_if_not_empty(orelse, after_block);

    let loop_continue_index = blocks.create_loop_continue_block();
    let last_statement_index = blocks.append_blocks_if_not_empty(body, loop_continue_index);
    blocks.blocks[loop_continue_index].next = NextBlock::Always(blocks.blocks.next_index());

    post_process_loop(
        blocks,
        last_statement_index,
        blocks.blocks.next_index(),
        after,
        after,
    );

    NextBlock::If {
        condition,
        next: last_statement_index,
        orelse: last_orelse_statement,
        exit: after,
    }
}

/// Step through the loop in the forward direction so that `break`
/// and `continue` can be correctly directed now that the loop start
/// and exit have been established.
fn post_process_loop(
    blocks: &mut BasicBlocksBuilder<'_>,
    start_index: BlockIndex,
    loop_start: BlockIndex,
    loop_exit: Option<BlockIndex>,
    clause_exit: Option<BlockIndex>,
) {
    let mut idx = start_index;

    loop {
        if Some(idx) == clause_exit || idx == loop_start {
            return;
        }

        let block = &mut blocks.blocks[idx];

        if block.is_loop_continue() {
            return;
        }

        match block.next {
            NextBlock::Always(next) => {
                match block.stmts.last() {
                    Some(Stmt::Break(_)) => {
                        block.next = match loop_exit {
                            Some(exit) => NextBlock::Always(exit),
                            None => NextBlock::Terminate,
                        }
                    }
                    Some(Stmt::Continue(_)) => {
                        block.next = NextBlock::Always(loop_start);
                    }
                    Some(Stmt::Return(_)) => return,
                    _ => {}
                };
                idx = next;
            }
            NextBlock::If {
                condition: _,
                next,
                orelse,
                exit,
            } => {
                match block.stmts.last() {
                    Some(Stmt::For(_) | Stmt::While(_)) => {
                        idx = orelse;
                    }
                    Some(Stmt::Assert(_)) => {
                        post_process_loop(blocks, orelse, loop_start, loop_exit, exit);
                        idx = next;
                    }
                    _ => {
                        post_process_loop(blocks, next, loop_start, loop_exit, exit);
                        idx = orelse;
                    }
                };
            }
            NextBlock::Terminate => return,
        }
    }
}

/// Handle a try block.
fn try_block<'stmt>(
    blocks: &mut BasicBlocksBuilder<'stmt>,
    stmt: &'stmt Stmt,
    after: Option<BlockIndex>,
) -> NextBlock<'stmt> {
    let stmts = std::slice::from_ref(stmt);
    let Stmt::Try(StmtTry {
        body,
        handlers,
        orelse,
        finalbody,
        ..
    }) = stmt
    else {
        panic!("Should only be called with StmtTry.");
    };

    let after_block = blocks.find_next_block_index(after);
    let finally_block = blocks.append_blocks_if_not_empty(finalbody, after_block);
    let else_block = blocks.append_blocks_if_not_empty(orelse, finally_block);
    let try_block = blocks.append_blocks_if_not_empty(body, else_block);

    let finally_index = if finalbody.is_empty() {
        None
    } else {
        Some(finally_block)
    };

    // If an exception is raised and not caught then terminate with exception.
    let mut next_branch = blocks.create_exception_block();

    // If there is a finally block, then re-route to finally
    if let Some(finally_index) = finally_index {
        blocks.blocks[next_branch].next = NextBlock::Always(finally_index);
    }

    for handler in handlers.iter().rev() {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            body, type_, ..
        }) = handler;
        let except_block = blocks.append_blocks_if_not_empty(body, finally_block);

        post_process_try(
            blocks,
            except_block,
            None,
            finally_index,
            Some(finally_block),
        );

        if let Some(type_) = type_ {
            let next = NextBlock::If {
                condition: Condition::Except(type_.as_ref()),
                next: except_block,
                orelse: next_branch,
                exit: after,
            };
            let block = BasicBlock::new(stmts, next);
            next_branch = blocks.blocks.push(block);
        } else {
            // If no exception type is provided, i.e., `except:`
            // Then execute the body unconditionally.
            next_branch = except_block;
        }
    }

    let except_index = if handlers.is_empty() {
        None
    } else {
        Some(next_branch)
    };
    post_process_try(
        blocks,
        try_block,
        except_index,
        finally_index,
        Some(else_block),
    );
    // We cannot know if the try block will raise an exception (apart from explicit raise statements)
    // We therefore assume that both paths may execute
    NextBlock::If {
        condition: Condition::MaybeRaised,
        next: next_branch, // If exception raised go to except -> except -> ... -> finally
        orelse: try_block, // Otherwise try -> else -> finally
        exit: after,
    }
}

/// Step through the try in the forward direction so that `assert`
/// and `raise` can be correctly directed now that the `try` and `except`
/// blocks have been established.
fn post_process_try(
    blocks: &mut BasicBlocksBuilder<'_>,
    start_index: BlockIndex,
    except_index: Option<BlockIndex>,
    finally_index: Option<BlockIndex>,
    exit_index: Option<BlockIndex>,
) {
    let mut idx = start_index;
    let mut next_index;

    loop {
        if Some(idx) == exit_index {
            return;
        }

        let block = &blocks.blocks[idx];
        match &block.next {
            NextBlock::Always(next) => {
                next_index = *next;
                match block.stmts.last() {
                    Some(Stmt::Break(_)) => return,
                    Some(Stmt::Continue(_)) => return,
                    Some(Stmt::Raise(_)) => {
                        // re-route to except if not already re-routed
                        if let Some(except_index) = except_index {
                            if blocks.blocks[*next].is_exception() {
                                blocks.blocks[idx].next = NextBlock::Always(except_index);
                            }
                        } else if let Some(finally_index) = finally_index {
                            if blocks.blocks[*next].is_exception() {
                                blocks.blocks[idx].next = NextBlock::Always(finally_index);
                            }
                        }
                        return;
                    }
                    // return has already been re-routed
                    Some(Stmt::Return(_)) => return,
                    _ => {}
                };
            }
            NextBlock::If {
                condition,
                next,
                orelse,
                exit,
            } => {
                match block.stmts.last() {
                    Some(Stmt::Assert(_)) => {
                        next_index = *next;
                        // re-route to except if not already re-routed
                        if let Some(except_index) = except_index {
                            if blocks.blocks[*orelse].is_exception() {
                                blocks.blocks[idx].next = NextBlock::If {
                                    condition: condition.clone(),
                                    next: *next,
                                    orelse: except_index,
                                    exit: *exit,
                                };
                            }
                        } else if let Some(finally_index) = finally_index {
                            if blocks.blocks[*orelse].is_exception() {
                                blocks.blocks[idx].next = NextBlock::If {
                                    condition: condition.clone(),
                                    next: *next,
                                    orelse: finally_index,
                                    exit: *exit,
                                };
                            }
                        }
                    }
                    Some(Stmt::Try(_)) => {
                        next_index = *next;
                        post_process_try(blocks, *orelse, except_index, finally_index, *exit);
                    }
                    _ => {
                        next_index = *orelse;
                        post_process_try(blocks, *next, except_index, finally_index, *exit);
                    }
                };
            }
            NextBlock::Terminate => {
                match block.stmts.last() {
                    Some(Stmt::Return(_)) => {
                        // if we are already in a `finally` block, terminate
                        if Some(idx) == finally_index {
                            return;
                        }
                        // re-route to finally if present and not already re-routed
                        if let Some(finally_index) = finally_index {
                            blocks.blocks[idx].next = NextBlock::Always(finally_index);
                        }
                        return;
                    }
                    _ => return,
                };
            }
        }
        idx = next_index;
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
    let next_block_index = blocks.append_blocks_if_not_empty(&case.body, next_after_block);
    let next = if is_wildcard(case) {
        // Wildcard case is always taken.
        NextBlock::Always(next_block_index)
    } else {
        NextBlock::If {
            condition: Condition::Match { subject, case },
            next: next_block_index,
            orelse: orelse_after_block,
            exit: Some(next_after_block),
        }
    };
    BasicBlock::new(stmts, next)
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
                | Stmt::TypeAlias(_)
                | Stmt::IpyEscapeCommand(_)
                | Stmt::Pass(_) => self.unconditional_next_block(after),
                Stmt::Break(_) | Stmt::Continue(_) => {
                    // NOTE: These are handled in post_process_loop.
                    self.unconditional_next_block(after)
                }
                // Statements that (can) divert the control flow.
                Stmt::If(stmt_if) => {
                    // Always get an after_block to avoid having to get one for each branch that needs it.
                    let after_block = self.find_next_block_index(after);
                    let consequent = self.append_blocks_if_not_empty(&stmt_if.body, after_block);

                    // Block ID of the next elif or else clause.
                    let mut next_branch = after_block;

                    for clause in stmt_if.elif_else_clauses.iter().rev() {
                        let consequent = self.append_blocks_if_not_empty(&clause.body, after_block);
                        next_branch = if let Some(test) = &clause.test {
                            let next = NextBlock::If {
                                condition: Condition::Test(test),
                                next: consequent,
                                orelse: next_branch,
                                exit: after,
                            };
                            let stmts = std::slice::from_ref(stmt);
                            let block = BasicBlock::new(stmts, next);
                            self.blocks.push(block)
                        } else {
                            consequent
                        };
                    }

                    NextBlock::If {
                        condition: Condition::Test(&stmt_if.test),
                        next: consequent,
                        orelse: next_branch,
                        exit: after,
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
                Stmt::Try(_) => try_block(self, stmt, after),
                Stmt::With(StmtWith { body, .. }) => {
                    let after_block = self.find_next_block_index(after);
                    let with_block = self.append_blocks(body, after);

                    // The with statement is equivalent to a try statement with an except and finally block
                    // However, we do not have access to the except and finally.
                    // We therefore assume that execution may fall through on error.
                    NextBlock::If {
                        condition: Condition::MaybeRaised,
                        next: after_block,  // If exception raised fall through
                        orelse: with_block, // Otherwise execute the with statement
                        exit: after,
                    }
                }
                Stmt::Match(StmtMatch { subject, cases, .. }) => {
                    let after_block = self.find_next_block_index(after);
                    let mut orelse_after_block = after_block;
                    for case in cases.iter().rev() {
                        let block =
                            match_case(self, stmt, subject, case, after_block, orelse_after_block);
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
                    // NOTE: This may be modified in post_process_try.
                    NextBlock::Always(self.create_exception_block())
                }
                Stmt::Assert(stmt) => {
                    // NOTE: This may be modified in post_process_try.
                    let next = self.find_next_block_index(after);
                    let orelse = self.create_exception_block();
                    NextBlock::If {
                        condition: Condition::Test(&stmt.test),
                        next,
                        orelse,
                        exit: after,
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
                        Expr::Named(_)
                        | Expr::Lambda(_)
                        | Expr::If(_)
                        | Expr::ListComp(_)
                        | Expr::SetComp(_)
                        | Expr::DictComp(_)
                        | Expr::Generator(_)
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

            let block = BasicBlock::new(&stmts[start..end], next);
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

    /// Select the next block index from `blocks`.
    /// If there is no next block it will add a fake/empty block.
    fn find_next_block_index(&mut self, after: Option<BlockIndex>) -> BlockIndex {
        if let Some(after) = after {
            // Next block is already determined.
            after
        } else if let Some(idx) = self.blocks.last_index() {
            // Otherwise we either continue with the next block (that is the last
            // block in `blocks`).
            idx
        } else {
            // Or if there are no blocks, add a fake end block.
            self.blocks.push(BasicBlock::EMPTY)
        }
    }

    /// Returns a block index for an `EXCEPTION` block in `blocks`.
    fn create_exception_block(&mut self) -> BlockIndex {
        self.blocks.push(BasicBlock::EXCEPTION.clone())
    }

    /// Returns a block index for an `LOOP_CONTINUE` block in `blocks`.
    fn create_loop_continue_block(&mut self) -> BlockIndex {
        self.blocks.push(BasicBlock::LOOP_CONTINUE.clone())
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

impl std::ops::DerefMut for BasicBlocksBuilder<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.blocks
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{fmt, fs};

    use ruff_python_parser::parse_module;
    use ruff_text_size::Ranged;
    use std::fmt::Write;
    use test_case::test_case;

    use crate::rules::pylint::rules::unreachable::{BasicBlocks, BlockIndex, Condition, NextBlock};

    #[test_case("simple.py")]
    #[test_case("if.py")]
    #[test_case("while.py")]
    #[test_case("for.py")]
    #[test_case("async-for.py")]
    #[test_case("try.py")]
    #[test_case("raise.py")]
    #[test_case("assert.py")]
    #[test_case("match.py")]
    #[test_case("try-finally-nested-if-while.py")]
    fn control_flow_graph(filename: &str) {
        let path = PathBuf::from_iter(["resources/test/fixtures/control-flow-graph", filename]);
        let source = fs::read_to_string(path).expect("failed to read file");
        let stmts = parse_module(&source)
            .unwrap_or_else(|err| panic!("failed to parse source: '{source}': {err}"))
            .into_suite();

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

    /// Type to create a Mermaid graph.
    ///
    /// To learn amount Mermaid see <https://mermaid.js.org/intro>, for the syntax
    /// see <https://mermaid.js.org/syntax/flowchart.html>.
    struct MermaidGraph<'stmt, 'source> {
        graph: &'stmt BasicBlocks<'stmt>,
        source: &'source str,
    }

    impl fmt::Display for MermaidGraph<'_, '_> {
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
                } else if block.is_loop_continue() {
                    write!(f, "Loop continue")?;
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
                        ..
                    } => {
                        let condition_code = match condition {
                            Condition::MaybeRaised => "Exception raised",
                            _ => self.source[condition.range()].trim(),
                        };
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
}
