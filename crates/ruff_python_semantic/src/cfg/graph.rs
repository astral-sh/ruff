use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::{ExceptHandler, ExceptHandlerExceptHandler, Expr, MatchCase, Stmt};
use ruff_text_size::{Ranged, TextRange};

/// Returns the control flow graph associated to an array of statements
pub fn build_cfg(stmts: &[Stmt]) -> ControlFlowGraph<'_> {
    let mut builder = CFGBuilder::with_capacity(stmts.len());
    builder.process_stmts(stmts);
    builder.finish()
}

/// Control flow graph
#[derive(Debug)]
pub struct ControlFlowGraph<'stmt> {
    /// Basic blocks - the nodes of the control flow graph
    blocks: IndexVec<BlockId, BlockData<'stmt>>,
    /// Entry point to the control flow graph
    initial: BlockId,
    /// Terminal block - will always be empty
    terminal: BlockId,
}

impl<'stmt> ControlFlowGraph<'stmt> {
    /// Index of entry point to the control flow graph
    pub fn initial(&self) -> BlockId {
        self.initial
    }

    /// Index of terminal block
    pub fn terminal(&self) -> BlockId {
        self.terminal
    }

    /// Number of basic blocks, or nodes, in the graph
    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// Returns the statements comprising the basic block at the given index
    pub fn stmts(&self, block: BlockId) -> &'stmt [Stmt] {
        self.blocks[block].stmts
    }

    /// Returns the range of the statements comprising the basic block at the given index
    pub fn range(&self, block: BlockId) -> TextRange {
        self.blocks[block].range()
    }

    /// Returns the [`Edges`] going out of the basic block at the given index
    pub fn outgoing(&self, block: BlockId) -> &Edges {
        &self.blocks[block].out
    }

    /// Returns an iterator over the indices of the direct predecessors of the block at the given index
    pub fn predecessors(&self, block: BlockId) -> impl ExactSizeIterator<Item = BlockId> + '_ {
        self.blocks[block].parents.iter().copied()
    }

    /// Returns the [`BlockKind`] of the block at the given index
    pub(crate) fn kind(&self, block: BlockId) -> BlockKind {
        self.blocks[block].kind
    }
}

#[newtype_index]
pub struct BlockId;

/// Holds the data of a basic block. A basic block consists of a collection of
/// [`Stmt`]s, together with outgoing edges to other basic blocks.
#[derive(Debug, Default)]
struct BlockData<'stmt> {
    kind: BlockKind,
    /// Slice of statements regarded as executing unconditionally in order
    stmts: &'stmt [Stmt],
    /// Outgoing edges, indicating possible paths of execution after the
    /// block has concluded
    out: Edges<'stmt>,
    /// Collection of indices for basic blocks having the current
    /// block as the target of an edge
    parents: Vec<BlockId>,
}

impl Ranged for BlockData<'_> {
    fn range(&self) -> TextRange {
        let Some(first) = self.stmts.first() else {
            return TextRange::default();
        };
        let Some(last) = self.stmts.last() else {
            return TextRange::default();
        };

        TextRange::new(first.start(), last.end())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) enum BlockKind {
    #[default]
    Generic,
    /// Entry point of the control flow graph
    Start,
    /// Terminal block for the control flow graph
    Terminal,
    LoopGuard,
    ExceptionDispatch,
    Recovery,
}

/// Holds a collection of edges. Each edge is determined by:
///  - a [`Condition`] for traversing the edge, and
///  - a target block, specified by its [`BlockId`].
///
/// The conditions and targets are kept in two separate
/// vectors which must always be kept the same length.
#[derive(Debug, Default, Clone)]
pub struct Edges<'stmt> {
    conditions: Vec<Condition<'stmt>>,
    targets: Vec<BlockId>,
}

impl<'stmt> Edges<'stmt> {
    /// Creates an unconditional edge to the target block
    fn always(target: BlockId) -> Self {
        Self {
            conditions: vec![Condition::Always],
            targets: vec![target],
        }
    }

    /// Returns iterator over indices of blocks targeted by given edges
    pub fn targets(&self) -> impl ExactSizeIterator<Item = BlockId> + '_ {
        self.targets.iter().copied()
    }

    /// Returns iterator over [`Condition`]s which must be satisfied to traverse corresponding edge
    pub fn conditions(&self) -> impl ExactSizeIterator<Item = &Condition<'stmt>> {
        self.conditions.iter()
    }

    fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn filter_targets_by_conditions<'a: 'stmt, T: FnMut(&Condition) -> bool + 'a>(
        &'a self,
        mut predicate: T,
    ) -> impl Iterator<Item = BlockId> + 'a {
        self.conditions()
            .zip(self.targets())
            .filter(move |(cond, _)| predicate(cond))
            .map(|(_, block)| block)
    }
}

/// Represents a condition to be tested in a multi-way branch
#[derive(Debug, Clone)]
pub enum Condition<'stmt> {
    /// Unconditional edge
    Always,
    /// A boolean test expression
    Test(&'stmt Expr),
    /// A match case with its subject expression
    Match {
        subject: &'stmt Expr,
        case: &'stmt MatchCase,
    },
    /// Test whether `next` on iterator gives `StopIteration`
    NotStopIter(&'stmt Expr),
    /// A fallback case (else/wildcard case/etc.)
    Else,
    /// An except handler for try/except blocks
    ExceptHandler(&'stmt ExceptHandlerExceptHandler),
    /// An uncaught exception
    UncaughtException,
    /// Deferred
    Deferred(&'stmt Stmt),
}

struct CFGBuilder<'stmt> {
    /// Control flow graph under construction
    cfg: ControlFlowGraph<'stmt>,
    /// Current basic block index
    current: BlockId,
    /// Exit block index for current control flow
    exit: BlockId,
    /// Loop contexts
    loops: Vec<LoopContext>,
    /// Try contexts
    try_contexts: Vec<TryContext<'stmt>>,
}

impl<'stmt> CFGBuilder<'stmt> {
    /// Returns [`CFGBuilder`] with vector of blocks initialized at given capacity and with both initial and terminal blocks populated.
    fn with_capacity(capacity: usize) -> Self {
        let mut blocks = IndexVec::with_capacity(capacity);
        let initial = blocks.push(BlockData {
            kind: BlockKind::Start,
            ..BlockData::default()
        });
        let terminal = blocks.push(BlockData {
            kind: BlockKind::Terminal,
            ..BlockData::default()
        });

        Self {
            cfg: ControlFlowGraph {
                blocks,
                initial,
                terminal,
            },
            current: initial,
            exit: terminal,
            loops: Vec::default(),
            try_contexts: Vec::default(),
        }
    }

    /// Runs the core logic for the builder.
    fn process_stmts(&mut self, stmts: &'stmt [Stmt]) {
        // SAFETY With notation as below, we always maintain the invariant
        // `start <= end + 1`. Since `end <= stmts.len() -1` we conclude that
        // `start <= stmts.len()`. It is therefore always safe to use `start` as
        // the beginning of a range for the purposes of slicing into `stmts`.
        let mut start = 0;
        for (end, stmt) in stmts.iter().enumerate() {
            let cache_exit = self.exit();
            match stmt {
                Stmt::FunctionDef(_)
                | Stmt::ClassDef(_)
                | Stmt::Assign(_)
                | Stmt::AugAssign(_)
                | Stmt::AnnAssign(_)
                | Stmt::TypeAlias(_)
                | Stmt::Import(_)
                | Stmt::ImportFrom(_)
                | Stmt::Global(_)
                | Stmt::Nonlocal(_)
                | Stmt::Expr(_)
                | Stmt::Pass(_)
                | Stmt::Delete(_)
                | Stmt::IpyEscapeCommand(_) => {}
                // Loops
                Stmt::While(stmt_while) => {
                    // Block to move to after processing loop
                    let next_block = self.next_or_default_block(&stmts[end + 1..], self.exit);

                    // Blocks for guard, body, and (optional) else clause
                    let guard = self.new_loop_guard();
                    let body = self.new_block();
                    let orelse = if stmt_while.orelse.is_empty() {
                        None
                    } else {
                        Some(self.new_block())
                    };

                    // Finish current block and move to guard
                    if self.current != guard {
                        self.set_current_block_stmts(&stmts[start..end]);
                        self.set_current_block_edges(Edges::always(guard));
                        self.move_to(guard);
                    }

                    // Finish guard and push loop context
                    let guard_target = orelse.unwrap_or(next_block);
                    let targets = vec![body, guard_target];
                    let conditions = vec![Condition::Test(&stmt_while.test), Condition::Else];
                    let edges = Edges {
                        conditions,
                        targets,
                    };
                    self.set_current_block_stmts(&stmts[end..=end]);
                    self.set_current_block_edges(edges);
                    self.push_loop(guard, next_block);

                    // Process body
                    self.update_exit(guard);
                    self.move_to(body);
                    self.process_stmts(&stmt_while.body);

                    // Process (optional) else
                    if let Some(orelse) = orelse {
                        self.update_exit(next_block);
                        self.move_to(orelse);
                        self.process_stmts(&stmt_while.orelse);
                    }

                    // Cleanup
                    self.pop_loop();
                    self.move_to(next_block);
                    start = end + 1;
                }
                Stmt::For(stmt_for) => {
                    // Block to move to after processing loop
                    let next_block = self.next_or_default_block(&stmts[end + 1..], self.exit);

                    // Blocks for guard, body, and (optional) else clause
                    let guard = self.new_loop_guard();
                    let body = self.new_block();
                    let orelse = if stmt_for.orelse.is_empty() {
                        None
                    } else {
                        Some(self.new_block())
                    };

                    // Finish current block and move to guard
                    if self.current != guard {
                        self.set_current_block_stmts(&stmts[start..end]);
                        self.set_current_block_edges(Edges::always(guard));
                        self.move_to(guard);
                    }

                    // Finish guard and push loop context
                    let guard_target = orelse.unwrap_or(next_block);
                    let targets = vec![body, guard_target];
                    let conditions = vec![Condition::NotStopIter(&stmt_for.iter), Condition::Else];
                    let edges = Edges {
                        conditions,
                        targets,
                    };
                    self.set_current_block_stmts(&stmts[end..=end]);
                    self.set_current_block_edges(edges);
                    self.push_loop(guard, next_block);

                    // Process body
                    self.update_exit(guard);
                    self.move_to(body);
                    self.process_stmts(&stmt_for.body);

                    // Process (optional) else
                    if let Some(orelse) = orelse {
                        self.update_exit(next_block);
                        self.move_to(orelse);
                        self.process_stmts(&stmt_for.orelse);
                    }

                    // Cleanup
                    self.pop_loop();
                    self.move_to(next_block);
                    start = end + 1;
                }

                // Switch statements
                Stmt::If(stmt_if) => {
                    // Block to move to after processing loop
                    let next_block = self.next_or_default_block(&stmts[end + 1..], self.exit);

                    // Create a block for the if-test
                    let if_block = self.new_block();

                    // Create a block for each elif clause
                    let mut case_blocks = Vec::with_capacity(stmt_if.elif_else_clauses.len() + 1);
                    case_blocks.push(if_block);
                    for _ in 0..stmt_if.elif_else_clauses.len() {
                        case_blocks.push(self.new_block());
                    }

                    // Create edges to match cases and fallthrough
                    // (depending on whether wildcard case is found)
                    let mut conditions = Vec::with_capacity(stmt_if.elif_else_clauses.len() + 2);
                    let mut has_else = false;
                    conditions.push(Condition::Test(&stmt_if.test));
                    for case in &stmt_if.elif_else_clauses {
                        if let Some(test) = &case.test {
                            conditions.push(Condition::Test(test));
                        } else {
                            has_else = true;
                            conditions.push(Condition::Else);
                        }
                    }

                    if has_else {
                        let edges = Edges {
                            conditions,
                            targets: case_blocks.clone(),
                        };
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                    } else {
                        conditions.push(Condition::Else);
                        let edges = Edges {
                            conditions,
                            targets: [case_blocks.as_slice(), &[next_block]].concat(),
                        };
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                    }

                    // Process if-branch
                    self.move_to(if_block);
                    self.update_exit(next_block);
                    self.process_stmts(&stmt_if.body);

                    // Process each case
                    for (block, case) in case_blocks
                        .iter()
                        // Skip `if` block
                        .skip(1)
                        .zip(stmt_if.elif_else_clauses.iter())
                    {
                        self.move_to(*block);
                        self.update_exit(next_block);
                        self.process_stmts(&case.body);
                    }

                    // Cleanup
                    self.move_to(next_block);
                    start = end + 1;
                }
                Stmt::Match(stmt_match) => {
                    // Block to move to after processing loop
                    let next_block = self.next_or_default_block(&stmts[end + 1..], self.exit);

                    // Create a block for each case
                    let mut case_blocks = Vec::with_capacity(stmt_match.cases.len());
                    for _ in 0..stmt_match.cases.len() {
                        case_blocks.push(self.new_block());
                    }

                    // Create edges to match cases and fallthrough
                    // (depending on whether wildcard case is found)
                    let mut conditions = Vec::with_capacity(stmt_match.cases.len() + 1);
                    let mut has_wildcard = false;
                    for case in &stmt_match.cases {
                        if case.pattern.is_wildcard() {
                            has_wildcard = true;
                        }
                        conditions.push(Condition::Match {
                            subject: &stmt_match.subject,
                            case,
                        });
                    }

                    if has_wildcard {
                        let edges = Edges {
                            conditions,
                            targets: case_blocks.clone(),
                        };
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                    } else {
                        conditions.push(Condition::Else);
                        let edges = Edges {
                            conditions,
                            targets: [case_blocks.as_slice(), &[next_block]].concat(),
                        };
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                    }

                    // Process each case
                    for (block, case) in case_blocks.iter().zip(stmt_match.cases.iter()) {
                        self.move_to(*block);
                        self.update_exit(next_block);
                        self.process_stmts(&case.body);
                    }

                    // Cleanup
                    self.move_to(next_block);
                    start = end + 1;
                }

                // Exception handling statements
                Stmt::Try(stmt_try) => {
                    let try_kind = match (
                        !stmt_try.handlers.is_empty(),
                        !stmt_try.orelse.is_empty(),
                        !stmt_try.finalbody.is_empty(),
                    ) {
                        (true, false, false) => TryKind::TryExcept,
                        (false, false, true) => TryKind::TryFinally,
                        (true, true, false) => TryKind::TryExceptElse,
                        (true, false, true) => TryKind::TryExceptFinally,
                        (true, true, true) => TryKind::TryExceptElseFinally,
                        _ => {
                            unreachable!("Invalid try statement.")
                        }
                    };
                    self.push_try_context(try_kind);

                    let try_block = self.new_block_if_nonempty();
                    if self.current != try_block {
                        self.set_current_block_stmts(&stmts[start..end]);
                        self.set_current_block_edges(Edges::always(try_block));
                        self.move_to(try_block);
                    }
                    let next_block = self.next_or_default_block(&stmts[end + 1..], self.exit);

                    match try_kind {
                        TryKind::TryFinally => {
                            let finally_block = self.new_block();
                            let recovery_block = self.new_recovery();

                            // Process try clause
                            self.update_exit(finally_block);
                            self.process_stmts(&stmt_try.body);

                            // Process finally clause
                            self.move_to(finally_block);
                            self.set_try_state(TryState::Finally);
                            self.update_exit(recovery_block);
                            self.process_stmts(&stmt_try.finalbody);

                            // Process recovery
                            self.move_to(recovery_block);
                            self.set_try_state(TryState::Recovery);
                            self.update_exit(next_block);
                            let edges = self.resolve_deferred_jumps();
                            self.set_current_block_edges(edges);
                        }
                        TryKind::TryExcept => {
                            let dispatch_block = self.new_exception_dispatch();
                            self.update_exit(dispatch_block);
                            self.process_stmts(&stmt_try.body);

                            self.move_to(dispatch_block);
                            self.set_try_state(TryState::Dispatch);
                            self.update_exit(cache_exit);

                            // Create a vec of conditions and their target blocks
                            let mut conditions = Vec::with_capacity(stmt_try.handlers.len());
                            let mut except_blocks = Vec::with_capacity(stmt_try.handlers.len());

                            for ExceptHandler::ExceptHandler(handler) in stmt_try.handlers.iter() {
                                except_blocks.push(self.new_block());
                                conditions.push(Condition::ExceptHandler(handler));
                            }

                            let edges = Edges {
                                conditions,
                                targets: except_blocks.clone(),
                            };

                            self.set_current_block_edges(edges);

                            // Process each case's body
                            self.set_try_state(TryState::Except);
                            for (ExceptHandler::ExceptHandler(handler), block) in
                                stmt_try.handlers.iter().zip(except_blocks)
                            {
                                self.move_to(block);
                                self.update_exit(next_block);
                                self.process_stmts(&handler.body);
                            }
                            self.try_contexts.pop();
                        }
                        TryKind::TryExceptElse => {
                            let dispatch_block = self.new_exception_dispatch();
                            self.update_exit(dispatch_block);
                            self.process_stmts(&stmt_try.body);

                            self.move_to(dispatch_block);
                            self.set_try_state(TryState::Dispatch);
                            self.update_exit(cache_exit);

                            // Create a vec of conditions and their target blocks
                            let mut conditions = Vec::with_capacity(stmt_try.handlers.len() + 1);
                            let mut except_blocks = Vec::with_capacity(stmt_try.handlers.len());

                            for ExceptHandler::ExceptHandler(handler) in stmt_try.handlers.iter() {
                                except_blocks.push(self.new_block());
                                conditions.push(Condition::ExceptHandler(handler));
                            }

                            let else_block = self.new_block();
                            conditions.push(Condition::Else);

                            let edges = Edges {
                                conditions,
                                targets: [except_blocks.as_slice(), &[else_block]].concat(),
                            };

                            self.set_current_block_edges(edges);

                            // Process each case's body
                            self.set_try_state(TryState::Except);
                            for (ExceptHandler::ExceptHandler(handler), block) in
                                stmt_try.handlers.iter().zip(except_blocks)
                            {
                                self.move_to(block);
                                self.update_exit(next_block);
                                self.process_stmts(&handler.body);
                            }

                            // Process else body
                            self.set_try_state(TryState::Else);
                            self.move_to(else_block);
                            self.process_stmts(&stmt_try.orelse);
                            self.try_contexts.pop();
                        }
                        TryKind::TryExceptFinally => {
                            let dispatch_block = self.new_exception_dispatch();
                            let finally_block = self.new_block();
                            let recovery_block = self.new_recovery();

                            self.update_exit(dispatch_block);
                            self.process_stmts(&stmt_try.body);

                            self.move_to(dispatch_block);
                            self.set_try_state(TryState::Dispatch);
                            // Create a vec of conditions and their target blocks
                            let mut conditions = Vec::with_capacity(stmt_try.handlers.len() + 1);
                            let mut except_blocks = Vec::with_capacity(stmt_try.handlers.len());

                            for ExceptHandler::ExceptHandler(handler) in stmt_try.handlers.iter() {
                                except_blocks.push(self.new_block());
                                conditions.push(Condition::ExceptHandler(handler));
                            }

                            conditions.push(Condition::Else);

                            let edges = Edges {
                                conditions,
                                targets: [except_blocks.as_slice(), &[finally_block]].concat(),
                            };

                            self.set_current_block_edges(edges);

                            // Process each case's body
                            self.set_try_state(TryState::Except);
                            for (ExceptHandler::ExceptHandler(handler), block) in
                                stmt_try.handlers.iter().zip(except_blocks)
                            {
                                self.move_to(block);
                                self.update_exit(finally_block);
                                self.process_stmts(&handler.body);
                            }

                            // Process finally clause
                            self.move_to(finally_block);
                            self.set_try_state(TryState::Finally);
                            self.update_exit(recovery_block);
                            self.process_stmts(&stmt_try.finalbody);

                            // Process recovery
                            self.move_to(recovery_block);
                            self.set_try_state(TryState::Recovery);
                            self.update_exit(next_block);
                            let edges = self.resolve_deferred_jumps();
                            self.set_current_block_edges(edges);
                        }
                        TryKind::TryExceptElseFinally => {
                            let dispatch_block = self.new_exception_dispatch();
                            let finally_block = self.new_block();
                            let recovery_block = self.new_recovery();

                            self.update_exit(dispatch_block);
                            self.process_stmts(&stmt_try.body);

                            self.move_to(dispatch_block);
                            self.set_try_state(TryState::Dispatch);

                            // Create a vec of conditions and their target blocks
                            let mut conditions = Vec::with_capacity(stmt_try.handlers.len() + 1);
                            let mut except_blocks = Vec::with_capacity(stmt_try.handlers.len());

                            for ExceptHandler::ExceptHandler(handler) in stmt_try.handlers.iter() {
                                except_blocks.push(self.new_block());
                                conditions.push(Condition::ExceptHandler(handler));
                            }

                            let else_block = self.new_block();
                            conditions.push(Condition::Else);

                            let edges = Edges {
                                conditions,
                                targets: [except_blocks.as_slice(), &[else_block]].concat(),
                            };

                            self.set_current_block_edges(edges);

                            // Process each case's body
                            self.set_try_state(TryState::Except);
                            for (ExceptHandler::ExceptHandler(handler), block) in
                                stmt_try.handlers.iter().zip(except_blocks)
                            {
                                self.move_to(block);
                                self.update_exit(finally_block);
                                self.process_stmts(&handler.body);
                            }

                            // Process else body
                            self.move_to(else_block);
                            self.set_try_state(TryState::Else);
                            self.process_stmts(&stmt_try.orelse);

                            // Process finally clause
                            self.move_to(finally_block);
                            self.set_try_state(TryState::Finally);
                            self.update_exit(recovery_block);
                            self.process_stmts(&stmt_try.finalbody);

                            // Process recovery
                            self.move_to(recovery_block);
                            self.set_try_state(TryState::Recovery);
                            self.update_exit(next_block);
                            let edges = self.resolve_deferred_jumps();
                            self.set_current_block_edges(edges);
                        }
                    }

                    self.move_to(next_block);
                    start = end + 1;
                }
                Stmt::With(_) => {}

                // Jumps
                Stmt::Return(_) => {
                    self.defer_jump(stmt).unwrap_or_else(|| {
                        let edges = Edges::always(self.cfg.terminal());
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                        start = end + 1;
                    });

                    if stmts.get(start).is_some() {
                        let next_block = self.new_block();
                        self.move_to(next_block);
                    }
                }
                Stmt::Break(_) => {
                    self.defer_jump(stmt).unwrap_or_else(|| {
                        let edges = Edges::always(
                            self.loop_exit()
                                .expect("`break` should only occur inside loop context"),
                        );
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                        start = end + 1;
                    });

                    if stmts.get(start).is_some() {
                        let next_block = self.new_block();
                        self.move_to(next_block);
                    }
                }
                Stmt::Continue(_) => {
                    self.defer_jump(stmt).unwrap_or_else(|| {
                        let edges = Edges::always(
                            self.loop_guard()
                                .expect("`continue` should only occur inside loop context"),
                        );
                        self.set_current_block_stmts(&stmts[start..=end]);
                        self.set_current_block_edges(edges);
                        start = end + 1;
                    });

                    if stmts.get(start).is_some() {
                        let next_block = self.new_block();
                        self.move_to(next_block);
                    }
                }
                Stmt::Raise(_) => {
                    let edges = Edges::always(self.cfg.terminal());
                    self.set_current_block_stmts(&stmts[start..=end]);
                    self.set_current_block_edges(edges);
                    start = end + 1;

                    if stmts.get(start).is_some() {
                        let next_block = self.new_block();
                        self.move_to(next_block);
                    }
                }

                // An `assert` is a mixture of a switch and a jump.
                Stmt::Assert(_) => {}
            }
            // Restore exit
            self.update_exit(cache_exit);
        }
        // It can happen that we have statements left over
        // and not yet occupying a block. In that case,
        // `self.current` should be pointing to an empty block
        // and we push the remaining statements to it here.
        if start < stmts.len() {
            self.set_current_block_stmts(&stmts[start..]);
        }
        // Add edge to exit if not already present
        // and not _already_ at exit
        if self.current != self.exit && self.cfg.blocks[self.current].out.is_empty() {
            let edges = Edges::always(self.exit());
            self.set_current_block_edges(edges);
        }
        self.move_to(self.exit());
    }

    /// Returns finished control flow graph
    fn finish(self) -> ControlFlowGraph<'stmt> {
        self.cfg
    }

    /// Current exit block, which may change during construction
    fn exit(&self) -> BlockId {
        self.exit
    }

    /// Point the current exit to block at provided index
    fn update_exit(&mut self, new_exit: BlockId) {
        self.exit = new_exit;
    }

    /// Moves current block to provided index
    fn move_to(&mut self, block: BlockId) {
        self.current = block;
    }

    /// Makes new block and returns index
    fn new_block(&mut self) -> BlockId {
        self.cfg.blocks.push(BlockData::default())
    }

    /// Returns index of block where control flow should proceed
    /// at the current depth.
    ///
    /// Creates a new block if there are remaining statements, otherwise
    /// returns provided default.
    fn next_or_default_block(
        &mut self,
        remaining_stmts: &'stmt [Stmt],
        default: BlockId,
    ) -> BlockId {
        if remaining_stmts.is_empty() {
            default
        } else {
            self.new_block()
        }
    }

    fn new_block_if_nonempty(&mut self) -> BlockId {
        let Some(currblock) = self.cfg.blocks.get_mut(self.current) else {
            return self.new_block();
        };
        if currblock.stmts.is_empty() {
            self.current
        } else {
            self.new_block()
        }
    }

    /// Creates a new block to handle entering and exiting a loop body.
    fn new_loop_guard(&mut self) -> BlockId {
        let Some(currblock) = self.cfg.blocks.get_mut(self.current) else {
            return self.cfg.blocks.push(BlockData {
                kind: BlockKind::LoopGuard,
                ..BlockData::default()
            });
        };
        if matches!(currblock.kind, BlockKind::Generic) && currblock.stmts.is_empty() {
            currblock.kind = BlockKind::LoopGuard;
            self.current
        } else {
            self.cfg.blocks.push(BlockData {
                kind: BlockKind::LoopGuard,
                ..BlockData::default()
            })
        }
    }

    /// Returns the current loop exit block without removing it.
    fn loop_exit(&self) -> Option<BlockId> {
        self.loops.last().map(|ctxt| ctxt.exit)
    }
    /// Returns the current loop guard block without removing it.
    fn loop_guard(&self) -> Option<BlockId> {
        self.loops.last().map(|ctxt| ctxt.guard)
    }

    /// Pushes a block onto the loop exit stack.
    /// This block represents where control should flow when encountering a
    /// 'break' statement within a loop.
    fn push_loop(&mut self, guard: BlockId, exit: BlockId) {
        self.loops.push(LoopContext { guard, exit });
    }

    /// Pops and returns the most recently pushed loop exit block.
    /// This is called when finishing the processing of a loop construct.
    fn pop_loop(&mut self) -> Option<LoopContext> {
        self.loops.pop()
    }

    /// Creates a new block to handle dispatching control flow at the end
    /// of a `try` block.
    fn new_exception_dispatch(&mut self) -> BlockId {
        self.cfg.blocks.push(BlockData {
            kind: BlockKind::ExceptionDispatch,
            ..BlockData::default()
        })
    }

    fn new_recovery(&mut self) -> BlockId {
        self.cfg.blocks.push(BlockData {
            kind: BlockKind::Recovery,
            ..BlockData::default()
        })
    }

    fn push_try_context(&mut self, kind: TryKind) {
        self.try_contexts.push(TryContext::new(kind));
    }

    fn set_try_state(&mut self, state: TryState) {
        let ctxt = self
            .try_contexts
            .last_mut()
            .expect("try contexts should be nonempty to set try state");
        ctxt.state = state;
    }

    fn resolve_deferred_jumps(&mut self) -> Edges<'stmt> {
        let try_context = self
            .try_contexts
            .pop()
            .expect("resolve deferred jumps only in try context");
        let deferred_jumps = try_context.deferred_jumps;
        // We may be nested inside _another_ try context, then we
        // don't resolve any jumps and keep deferring them.
        if self.should_defer_jumps() {
            self.extend_deferred_jumps(deferred_jumps);
            Edges::always(self.exit)
        } else {
            let mut conditions = Vec::with_capacity(deferred_jumps.len() + 1);
            let mut targets = Vec::with_capacity(deferred_jumps.len() + 1);
            for jump in deferred_jumps {
                conditions.push(Condition::Deferred(jump));
                let target = match jump {
                    Stmt::Return(_) | Stmt::Raise(_) => self.cfg.terminal,
                    Stmt::Break(_) => self.loop_exit().expect("break to be inside loop context"),
                    Stmt::Continue(_) => self
                        .loop_guard()
                        .expect("continue to be inside of loop context"),
                    _ => {
                        unreachable!(
                            "deferred jump statements must be return,break,raise, or continue"
                        )
                    }
                };
                targets.push(target);
            }
            conditions.push(Condition::Always);
            targets.push(self.exit);
            Edges {
                conditions,
                targets,
            }
        }
    }

    fn should_defer_jumps(&self) -> bool {
        self.try_contexts
            .iter()
            .any(|try_ctxt| match try_ctxt.state {
                TryState::Try => true,
                TryState::Except | TryState::Else if try_ctxt.has_finally() => true,
                _ => false,
            })
    }

    fn extend_deferred_jumps(&mut self, jumps: Vec<&'stmt Stmt>) {
        let Some(try_ctxt) = self.try_contexts.last_mut() else {
            return;
        };
        try_ctxt.deferred_jumps.extend(jumps);
    }

    fn defer_jump(&mut self, jump: &'stmt Stmt) -> Option<()> {
        if self.should_defer_jumps() {
            let ctxt = self
                .try_contexts
                .last_mut()
                .expect("try contexts nonempty if we should defer jumps");
            ctxt.deferred_jumps.push(jump);
            Some(())
        } else {
            None
        }
    }

    /// Populates the current basic block with the given set of statements.
    ///
    /// This should only be called once on any given block.
    fn set_current_block_stmts(&mut self, stmts: &'stmt [Stmt]) {
        debug_assert!(
            self.cfg.blocks[self.current].stmts.is_empty(),
            "Attempting to set statements on an already populated basic block."
        );
        self.cfg.blocks[self.current].stmts = stmts;
    }

    /// Draws provided edges out of the current basic block.
    ///
    /// This should only be called once on any given block.
    fn set_current_block_edges(&mut self, edges: Edges<'stmt>) {
        debug_assert!(
            self.cfg.blocks[self.current].out.is_empty(),
            "Attempting to set edges on a basic block that already has an outgoing edge."
        );
        self.cfg.blocks[self.current].out = edges;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LoopContext {
    guard: BlockId,
    exit: BlockId,
}

#[derive(Debug, Clone)]
pub struct TryContext<'stmt> {
    kind: TryKind,
    state: TryState,
    deferred_jumps: Vec<&'stmt Stmt>,
}

impl<'stmt> TryContext<'stmt> {
    pub fn new(kind: TryKind) -> Self {
        Self {
            kind,
            state: TryState::Try,
            deferred_jumps: Vec::new(),
        }
    }

    fn has_except(&self) -> bool {
        matches!(
            self.kind,
            TryKind::TryExcept
                | TryKind::TryExceptElse
                | TryKind::TryExceptFinally
                | TryKind::TryExceptElseFinally
        )
    }

    fn has_else(&self) -> bool {
        matches!(
            self.kind,
            TryKind::TryExceptElse | TryKind::TryExceptElseFinally
        )
    }

    fn has_finally(&self) -> bool {
        matches!(
            self.kind,
            TryKind::TryFinally | TryKind::TryExceptFinally | TryKind::TryExceptElseFinally
        )
    }

    fn in_try(&self) -> bool {
        matches!(self.state, TryState::Try)
    }
    fn in_dispatch(&self) -> bool {
        matches!(self.state, TryState::Dispatch)
    }
    fn in_except(&self) -> bool {
        matches!(self.state, TryState::Except)
    }
    fn in_else(&self) -> bool {
        matches!(self.state, TryState::Else)
    }
    fn in_finally(&self) -> bool {
        matches!(self.state, TryState::Finally)
    }
    fn in_recovery(&self) -> bool {
        matches!(self.state, TryState::Recovery)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TryKind {
    TryFinally,
    TryExcept,
    TryExceptElse,
    TryExceptFinally,
    TryExceptElseFinally,
}

#[derive(Debug, Clone, Copy)]
pub enum TryState {
    Try,
    Dispatch,
    Except,
    Else,
    Finally,
    Recovery,
}
