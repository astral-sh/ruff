use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};
use smallvec::{smallvec, SmallVec};

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
    out: Edges,
    /// Collection of indices for basic blocks having the current
    /// block as the target of an edge
    parents: SmallVec<[BlockId; 2]>,
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
}

/// Holds a collection of edges. Each edge is determined by:
///  - a [`Condition`] for traversing the edge, and
///  - a target block, specified by its [`BlockId`].
///
/// The conditions and targets are kept in two separate
/// vectors which must always be kept the same length.
#[derive(Debug, Default, Clone)]
pub struct Edges {
    conditions: SmallVec<[Condition; 4]>,
    targets: SmallVec<[BlockId; 4]>,
}

impl Edges {
    /// Creates an unconditional edge to the target block
    fn always(target: BlockId) -> Self {
        Self {
            conditions: smallvec![Condition::Always],
            targets: smallvec![target],
        }
    }

    /// Returns iterator over indices of blocks targeted by given edges
    pub fn targets(&self) -> impl ExactSizeIterator<Item = BlockId> + '_ {
        self.targets.iter().copied()
    }

    /// Returns iterator over [`Condition`]s which must be satisfied to traverse corresponding edge
    pub fn conditions(&self) -> impl ExactSizeIterator<Item = &Condition> {
        self.conditions.iter()
    }

    fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn filter_targets_by_conditions<'a, T: FnMut(&Condition) -> bool + 'a>(
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
pub enum Condition {
    /// Unconditional edge
    Always,
}

struct CFGBuilder<'stmt> {
    /// Control flow graph under construction
    cfg: ControlFlowGraph<'stmt>,
    /// Current basic block index
    current: BlockId,
    /// Exit block index for current control flow
    exit: BlockId,
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
                Stmt::While(_) => {}
                Stmt::For(_) => {}

                // Switch statements
                Stmt::If(_) => {}
                Stmt::Match(_) => {}

                // Exception handling statements
                Stmt::Try(_) => {}
                Stmt::With(_) => {}

                // Jumps
                Stmt::Return(_) => {
                    let edges = Edges::always(self.cfg.terminal());
                    self.set_current_block_stmts(&stmts[start..=end]);
                    self.set_current_block_edges(edges);
                    start = end + 1;

                    if stmts.get(start).is_some() {
                        let next_block = self.new_block();
                        self.move_to(next_block);
                    }
                }
                Stmt::Break(_) => {}
                Stmt::Continue(_) => {}
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
        if self.cfg.blocks[self.current].out.is_empty() {
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
    fn set_current_block_edges(&mut self, edges: Edges) {
        debug_assert!(
            self.cfg.blocks[self.current].out.is_empty(),
            "Attempting to set edges on a basic block that already has an outgoing edge."
        );
        self.cfg.blocks[self.current].out = edges;
    }
}
