use crate::use_def::{FlowSnapshot, UseDefMapBuilder};

use super::SemanticIndexBuilder;

/// An abstraction over the fact that each scope should have its own [`TryNodeContextStack`]
#[derive(Debug, Default)]
pub(super) struct TryNodeContextStackManager(Vec<TryNodeContextStack>);

impl TryNodeContextStackManager {
    /// Push a new [`TryNodeContextStack`] onto the stack of stacks.
    ///
    /// Each [`TryNodeContextStack`] is only valid for a single scope
    pub(super) fn enter_nested_scope(&mut self) {
        self.0.push(TryNodeContextStack::default());
    }

    /// Pop a new [`TryNodeContextStack`] off the stack of stacks.
    ///
    /// Each [`TryNodeContextStack`] is only valid for a single scope
    pub(super) fn exit_scope(&mut self) {
        let popped_context = self.0.pop();
        debug_assert!(
            popped_context.is_some(),
            "exit_scope() should never be called on an empty stack \
(this indicates an unbalanced `enter_nested_scope()`/`exit_scope()` pair of calls)"
        );
    }

    /// Push a [`TryNodeContext`] onto the [`TryNodeContextStack`]
    /// at the top of our stack of stacks
    pub(super) fn push_context(&mut self) {
        self.current_try_context_stack().push_context();
    }

    /// Pop a [`TryNodeContext`] off the [`TryNodeContextStack`] at the top of our stack of stacks.
    pub(super) fn pop_context(&mut self) -> TryNodeContext {
        self.current_try_context_stack().pop_context()
    }

    /// Retrieve the [`TryNodeContext`] that is currently at the top of the stack, and take all
    /// snapshots recorded while visiting the `try` suite.
    pub(super) fn take_try_suite_snapshots(&mut self) -> Vec<FlowSnapshot> {
        self.current_try_context_stack().take_try_suite_snapshots()
    }

    /// Record a definition in the try-node context at `scope_index`.
    pub(super) fn record_definition(
        &mut self,
        scope_index: usize,
        use_def_map: &UseDefMapBuilder<'_>,
    ) {
        self.0[scope_index].record_definition(use_def_map);
    }

    /// Retrieve the stack that is at the top of our stack of stacks.
    /// Push the snapshot onto the innermost `try` block's terminal-entry snapshots for its
    /// `finally` suite.
    pub(super) fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        self.current_try_context_stack()
            .record_terminal_finally_entry(builder);
    }

    /// Retrieve the [`TryNodeContextStack`] that is relevant for the current scope.
    fn current_try_context_stack(&mut self) -> &mut TryNodeContextStack {
        self.0
            .last_mut()
            .expect("There should always be at least one `TryBlockContexts` on the stack")
    }
}

/// The contexts of nested `try`/`except` blocks for a single scope
#[derive(Debug, Default)]
struct TryNodeContextStack(Vec<TryNodeContext>);

impl TryNodeContextStack {
    /// Push a new [`TryNodeContext`] for recording intermediate states
    /// while visiting a [`ruff_python_ast::StmtTry`] node that has a `finally` branch.
    fn push_context(&mut self) {
        self.0.push(TryNodeContext::default());
    }

    /// Pop a [`TryNodeContext`] off the stack.
    fn pop_context(&mut self) -> TryNodeContext {
        self.0
            .pop()
            .expect("Cannot pop a `try` block off an empty `TryBlockContexts` stack")
    }

    /// Take all snapshots recorded while visiting the `try` suite.
    fn take_try_suite_snapshots(&mut self) -> Vec<FlowSnapshot> {
        std::mem::take(
            &mut self
                .0
                .last_mut()
                .expect("Cannot take snapshots from an empty `TryBlockContexts` stack")
                .try_suite_snapshots,
        )
    }

    /// For each `try` block on the stack, create a snapshot and push it.
    fn record_definition(&mut self, use_def_map: &UseDefMapBuilder<'_>) {
        for context in &mut self.0 {
            context.record_definition(use_def_map.snapshot());
        }
    }

    /// Push the snapshot onto the innermost `try` block's terminal-entry snapshots for its
    /// `finally` suite.
    fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        if let Some(context) = self.0.last_mut() {
            context.record_terminal_finally_entry(builder.flow_snapshot());
        }
    }
}

/// Context for tracking definitions over the course of a single
/// [`ruff_python_ast::StmtTry`] node
///
/// It will likely be necessary to add more fields to this struct in the future
/// when we add more advanced handling of `finally` branches.
#[derive(Debug, Default)]
pub(super) struct TryNodeContext {
    try_suite_snapshots: Vec<FlowSnapshot>,
    terminal_finally_entry_snapshots: Vec<FlowSnapshot>,
}

impl TryNodeContext {
    pub(super) fn into_terminal_finally_entry_snapshots(self) -> Vec<FlowSnapshot> {
        self.terminal_finally_entry_snapshots
    }

    /// Take a record of what the internal state looked like after a definition
    fn record_definition(&mut self, snapshot: FlowSnapshot) {
        self.try_suite_snapshots.push(snapshot);
    }

    /// Take a record of what the internal state looked like before a terminal control-flow
    /// transfer that will pass through the `finally` suite.
    fn record_terminal_finally_entry(&mut self, snapshot: FlowSnapshot) {
        self.terminal_finally_entry_snapshots.push(snapshot);
    }
}
