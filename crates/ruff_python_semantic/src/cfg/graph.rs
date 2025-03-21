use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::Stmt;
use smallvec::SmallVec;

/// Returns the control flow graph associated to an array of statements
pub fn build_cfg<'stmt>(stmts: &'stmt [Stmt]) -> CFG<'stmt> {
    let mut builder = CFGBuilder::with_capacity(stmts.len());
    builder.process_stmts(stmts);
    builder.finish()
}

/// Control flow graph
#[derive(Debug)]
pub struct CFG<'stmt> {
    blocks: IndexVec<BlockId, BlockData<'stmt>>,
    initial: BlockId,
    terminal: BlockId,
}

impl<'stmt> CFG<'stmt> {
    pub fn initial(&self) -> BlockId {
        self.initial
    }

    pub fn terminal(&self) -> BlockId {
        self.terminal
    }

    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn stmts(&self, block: BlockId) -> &'stmt [Stmt] {
        self.blocks[block].stmts
    }

    pub fn outgoing(&self, block: BlockId) -> &Edges {
        &self.blocks[block].out
    }

    pub fn predecessors(&self, block: BlockId) -> impl ExactSizeIterator<Item = BlockId> + '_ {
        self.blocks[block].parents.iter().copied()
    }

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
    stmts: &'stmt [Stmt],
    out: Edges,
    parents: SmallVec<[BlockId; 2]>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) enum BlockKind {
    #[default]
    Generic,
    Start,
    Terminal,
}

/// Holds a collection of edges. Each edge is determined by:
///  - a [`Condition`] for traversing the edge, and
///  - a target block, specified by its [`BlockId`].
#[derive(Debug, Default, Clone)]
pub struct Edges {
    conditions: SmallVec<[Condition; 4]>,
    targets: SmallVec<[BlockId; 4]>,
}

impl Edges {
    pub fn targets(&self) -> impl ExactSizeIterator<Item = BlockId> + '_ {
        self.targets.iter().copied()
    }

    pub fn conditions(&self) -> impl ExactSizeIterator<Item = &Condition> {
        self.conditions.iter()
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
    cfg: CFG<'stmt>,
    /// Current basic block index
    current: BlockId,
    /// Exit block index for current control flow
    exit: BlockId,
}

impl<'stmt> CFGBuilder<'stmt> {
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
            cfg: CFG {
                blocks,
                initial,
                terminal,
            },
            current: initial,
            exit: terminal,
        }
    }

    fn process_stmts(&mut self, stmts: &'stmt [Stmt]) {
        todo!()
    }

    fn finish(self) -> CFG<'stmt> {
        self.cfg
    }

    fn current(&self) -> BlockId {
        self.current
    }

    fn exit(&self) -> BlockId {
        self.exit
    }
}
