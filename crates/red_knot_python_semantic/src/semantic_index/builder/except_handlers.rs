use std::cell::{RefCell, RefMut};

use crate::semantic_index::definition::{Definition, DefinitionCategory};
use crate::semantic_index::symbol::ScopedSymbolId;
use crate::semantic_index::use_def::FlowSnapshot;

use super::SemanticIndexBuilder;

/// An abstraction over the fact that each scope should have its own [`TryNodeContextStack`]
#[derive(Debug, Default)]
pub(super) struct TryNodeContextStackManager<'db>(Vec<TryNodeContextStack<'db>>);

impl<'db> TryNodeContextStackManager<'db> {
    /// Push a new [`TryNodeContextStack`] onto the stack of stacks.
    ///
    /// Each [`TryNodeContextStack`] is only valid for a single scope
    pub(super) fn enter_nested_scope(&mut self) {
        self.0.push(TryNodeContextStack::default());
    }

    /// Retrieve the [`TryNodeContextStack`] that is relevant for the current scope.
    pub(super) fn current_try_context_stack<'s>(&'s self) -> &'s TryNodeContextStack<'db> {
        self.0
            .last()
            .expect("There should always be at least one `TryBlockContexts` on the stack")
    }

    /// Pop a new [`TryNodeContextStack`] off the stack of stacks.
    ///
    /// Each [`TryNodeContextStack`] is only valid for a single scope
    pub(super) fn exit_scope(&mut self) {
        let popped_context = self.0.pop();
        assert!(
            popped_context.is_some(),
            "pop_context() should never be called on an empty stack \
(this indicates an unbalanced `push_context()`/`pop_context()` pair of calls)"
        );
    }
}

/// The contexts of nested `try`/`except` blocks for a single scope
#[derive(Debug, Default)]
pub(super) struct TryNodeContextStack<'db>(RefCell<Vec<TryNodeContext<'db>>>);

impl<'db> TryNodeContextStack<'db> {
    /// Push a new [`TryNodeContext`] for recording intermediate states
    /// while visiting a [`ruff_python_ast::StmtTry`] node that has a `finally` branch.
    pub(super) fn push_context_with_finally(&self) {
        self.0
            .borrow_mut()
            .push(TryNodeContext::with_finally_branch());
    }

    /// Push a new [`TryNodeContext`] for recording intermediate states
    /// while visiting a [`ruff_python_ast::StmtTry`] node
    /// that does not have a `finally` branch.
    pub(super) fn push_context_without_finally(&self) {
        self.0
            .borrow_mut()
            .push(TryNodeContext::without_finally_branch());
    }

    /// Pop a [`TryNodeContext`] off the stack.
    ///
    /// If the [`TryNodeContext`] represented a [`ruff_python_ast::StmtTry`] node
    /// that had a `finally` branch, return a record of all [`Definition`]s that took
    /// place during the `finally` branch. Else, return `None`.
    pub(super) fn pop_context(&self) -> Vec<DefinitionRecord<'db>> {
        let context = self
            .0
            .borrow_mut()
            .pop()
            .expect("Cannot pop a `try` block off an empty `TryBlockContexts` stack");
        if let TryNodeVisitationState::Finally {
            finally_definitions,
        } = context.visitation_state
        {
            finally_definitions
        } else {
            vec![]
        }
    }

    pub(super) fn borrow_mut<'s>(&'s self) -> TryNodeContextStackRefMut<'s, 'db> {
        TryNodeContextStackRefMut(self.0.borrow_mut())
    }
}

#[derive(Debug)]
pub(super) struct TryNodeContextStackRefMut<'s, 'db>(RefMut<'s, Vec<TryNodeContext<'db>>>);

impl<'s, 'db> TryNodeContextStackRefMut<'s, 'db> {
    /// For each `try` block on the stack, push the snapshot onto the `try` block
    pub(super) fn record_definition(
        &mut self,
        builder: &SemanticIndexBuilder,
        symbol: ScopedSymbolId,
        definition: Definition<'db>,
        definition_category: DefinitionCategory,
    ) {
        for context in self.0.iter_mut() {
            context.record_definition(builder, symbol, definition, definition_category);
        }
    }

    fn current_try_node(&mut self) -> &mut TryNodeContext<'db> {
        self.0.last_mut().expect(
            "`current_try_node()` should never be called if there are no `try` blocks on the stack",
        )
    }

    /// Adjust internal state of the innermost `try` node on the stack
    /// to reflect that a `try` block is being exited
    /// in the [`ruff_python_ast::StmtTry`] node being visited.
    ///
    /// ## Panics
    ///
    /// Panics if there are no `try` blocks on the current stack.
    pub(super) fn exit_try_block(&mut self) -> Vec<FlowSnapshot> {
        self.current_try_node().exit_try_block()
    }

    /// Adjust internal state of the innermost `try` block on the stack
    /// to reflect that a `finally` block is being entered
    /// in the [`ruff_python_ast::StmtTry`] node being visited.
    ///
    /// ## Panics
    ///
    /// Panics if there are no `try` blocks on the current stack.
    pub(super) fn enter_finally_block(&mut self) -> Option<Vec<FlowSnapshot>> {
        self.current_try_node().enter_finally_block()
    }
}

/// Context for tracking definitions over the course of a single `StmtTry` node
#[derive(Debug)]
struct TryNodeContext<'db> {
    has_finally_branch: bool,

    visitation_state: TryNodeVisitationState<'db>,
}

impl<'db> TryNodeContext<'db> {
    /// Create a new [`TryNodeContext`] object for tracking intermediate states
    /// while visiting a [`ruff_python_ast::StmtTry`] node that has a `finally` branch.
    fn with_finally_branch() -> Self {
        Self {
            has_finally_branch: true,
            visitation_state: TryNodeVisitationState::default(),
        }
    }

    /// Create a new [`TryNodeContext`] object for tracking intermediate states
    /// while visiting a [`ruff_python_ast::StmtTry`] node
    /// that does not have a `finally` branch.
    fn without_finally_branch() -> Self {
        Self {
            has_finally_branch: false,
            visitation_state: TryNodeVisitationState::default(),
        }
    }

    /// Adjust internal state to reflect that we are no longer visiting the
    /// `try` block of the [`ruff_python_ast::StmtTry`] node.
    ///
    /// Return a `Vec` of [`FlowSnapshot`]s representing the state after
    /// each definition that took place in the `try` block.
    fn exit_try_block(&mut self) -> Vec<FlowSnapshot> {
        let previous_state = std::mem::replace(
            &mut self.visitation_state,
            TryNodeVisitationState::ExceptElse {
                try_except_else_snapshots: self.has_finally_branch.then_some(vec![]),
            },
        );
        let TryNodeVisitationState::TryBlock {
            try_block_snapshots,
        } = previous_state
        else {
            panic!("There is no try block to exit from");
        };
        if let TryNodeVisitationState::ExceptElse {
            try_except_else_snapshots: Some(snapshots),
        } = &mut self.visitation_state
        {
            snapshots.extend(try_block_snapshots.iter().cloned());
        }
        try_block_snapshots
    }

    /// Adjust internal state to reflect that we are entering the
    /// `finally` block of the [`ruff_python_ast::StmtTry`] node.
    ///
    /// Return a `Vec` of [`FlowSnapshot`]s representing the state after
    /// each definition that took place in the `try`, `except` and `else` blocks.
    fn enter_finally_block(&mut self) -> Option<Vec<FlowSnapshot>> {
        let previous_state = std::mem::replace(
            &mut self.visitation_state,
            TryNodeVisitationState::Finally {
                finally_definitions: vec![],
            },
        );
        let TryNodeVisitationState::ExceptElse {
            try_except_else_snapshots,
        } = previous_state
        else {
            panic!(
                "Cannot enter a `finally` branch without having visited the `except` and `else` branches"
            );
        };
        try_except_else_snapshots
    }

    /// Take a record of what the internal state looked like after a definition
    fn record_definition(
        &mut self,
        builder: &SemanticIndexBuilder,
        symbol: ScopedSymbolId,
        definition: Definition<'db>,
        definition_category: DefinitionCategory,
    ) {
        match &mut self.visitation_state {
            TryNodeVisitationState::TryBlock {
                try_block_snapshots,
            } => {
                try_block_snapshots.push(builder.flow_snapshot());
            }
            TryNodeVisitationState::ExceptElse {
                try_except_else_snapshots: Some(snapshots),
            } => {
                snapshots.push(builder.flow_snapshot());
            }
            TryNodeVisitationState::ExceptElse {
                try_except_else_snapshots: None,
            } => {}
            TryNodeVisitationState::Finally {
                finally_definitions,
            } => {
                finally_definitions.push(DefinitionRecord {
                    symbol,
                    definition,
                    category: definition_category,
                });
            }
        }
    }
}

/// The state in which `TryNodeContext` could be in at any one point in time
#[derive(Debug)]
enum TryNodeVisitationState<'db> {
    /// We're currently visiting the `try` block
    TryBlock {
        try_block_snapshots: Vec<FlowSnapshot>,
    },

    /// We're currently visiting one of the `except` or `else` branches.
    ///
    /// It's only necessary to keep track of what the `try`-block snapshots were,
    /// and to keep snapshotting in the `except`/`else` branches,
    /// if we have a `finally` block. That's why this field is an `Option`.
    ExceptElse {
        try_except_else_snapshots: Option<Vec<FlowSnapshot>>,
    },

    /// We're currently visiting the `finally` branch
    Finally {
        finally_definitions: Vec<DefinitionRecord<'db>>,
    },
}

impl<'db> Default for TryNodeVisitationState<'db> {
    fn default() -> Self {
        Self::TryBlock {
            try_block_snapshots: vec![],
        }
    }
}

/// A record of a [`Definition`] that took place
/// during a `finally` block of a [`ruff_python_ast::StmtTry`] node.
#[derive(Debug)]
pub(super) struct DefinitionRecord<'db> {
    pub(super) symbol: ScopedSymbolId,
    pub(super) definition: Definition<'db>,
    pub(super) category: DefinitionCategory,
}
