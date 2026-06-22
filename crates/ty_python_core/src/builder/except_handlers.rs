use smallvec::SmallVec;

use crate::scope::{FileScopeId, ScopeKind};
use crate::use_def::FlowSnapshot;

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

    /// Push a [`TryNodeContext`] onto the [`TryNodeContextStack`] at the top of our stack of
    /// stacks.
    pub(super) fn push_context(&mut self, has_handlers: bool, has_bare_handler: bool) {
        self.current_try_context_stack()
            .push_context(has_handlers, has_bare_handler);
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

    /// Record a checkpoint for every active `try` suite that could handle an exception raised at
    /// the current point in control flow.
    pub(super) fn record_exception_checkpoint(&mut self, builder: &mut SemanticIndexBuilder) {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        let mut crossed_comprehensions = SmallVec::<[FileScopeId; 2]>::new();

        for (scope_stack_index, try_context_stack) in self.0.iter_mut().enumerate().rev() {
            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            let snapshot = if try_context_stack.has_active_exception_handler() {
                builder.exception_checkpoint_snapshot(scope_id, &crossed_comprehensions)
            } else {
                builder.use_def_maps[scope_id].snapshot()
            };

            // Each scope has an independent flow state, so an enclosing scope can still be
            // reachable while we analyze an unreachable nested scope.
            if snapshot.is_always_unreachable() {
                break;
            }

            if !try_context_stack.record_exception_checkpoint(&snapshot)
                || !builder.exception_checkpoint_crosses_scope_boundary(scope_id)
            {
                break;
            }

            if builder.scopes[scope_id].kind() == ScopeKind::Comprehension {
                crossed_comprehensions.push(scope_id);
            }
        }
    }

    pub(super) fn has_active_exception_handler(&self) -> bool {
        self.0
            .iter()
            .any(TryNodeContextStack::has_active_exception_handler)
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
    fn has_active_exception_handler(&self) -> bool {
        self.0
            .iter()
            .any(|context| context.try_suite_snapshots.is_some())
    }

    /// Push a new [`TryNodeContext`] for recording exception checkpoints and terminal entries
    /// while visiting a [`ruff_python_ast::StmtTry`] node.
    fn push_context(&mut self, has_handlers: bool, has_bare_handler: bool) {
        self.0.push(TryNodeContext {
            try_suite_snapshots: has_handlers.then(Vec::new),
            has_bare_handler,
            ..TryNodeContext::default()
        });
    }

    /// Pop a [`TryNodeContext`] off the stack.
    fn pop_context(&mut self) -> TryNodeContext {
        self.0
            .pop()
            .expect("Cannot pop a `try` block off an empty `TryBlockContexts` stack")
    }

    /// Take all snapshots recorded while visiting the `try` suite.
    fn take_try_suite_snapshots(&mut self) -> Vec<FlowSnapshot> {
        let context = self
            .0
            .last_mut()
            .expect("Cannot take snapshots from an empty `TryBlockContexts` stack");
        context.try_suite_snapshots.take().unwrap_or_default()
    }

    /// Records the checkpoint for all enclosing active `try` suites in this scope. Returns whether
    /// the checkpoint should continue propagating to an enclosing scope.
    fn record_exception_checkpoint(&mut self, snapshot: &FlowSnapshot) -> bool {
        for context in self.0.iter_mut().rev() {
            let Some(try_suite_snapshots) = &mut context.try_suite_snapshots else {
                continue;
            };

            try_suite_snapshots.push(snapshot.clone());
            if context.has_bare_handler {
                return false;
            }
        }

        true
    }

    /// Push the snapshot onto the innermost `try` block's terminal-entry snapshots for its
    /// `finally` suite.
    fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        if let Some(context) = self.0.last_mut() {
            context.record_terminal_finally_entry(builder.flow_snapshot());
        }
    }
}

/// Context for tracking exception and `finally` entry states for a single
/// [`ruff_python_ast::StmtTry`] node.
///
/// It will likely be necessary to add more fields to this struct in the future
/// when we add more advanced handling of `finally` branches.
#[derive(Debug, Default)]
pub(super) struct TryNodeContext {
    try_suite_snapshots: Option<Vec<FlowSnapshot>>,
    terminal_finally_entry_snapshots: Vec<FlowSnapshot>,
    has_bare_handler: bool,
}

impl TryNodeContext {
    pub(super) fn into_terminal_finally_entry_snapshots(self) -> Vec<FlowSnapshot> {
        self.terminal_finally_entry_snapshots
    }

    /// Take a record of what the internal state looked like before a terminal control-flow
    /// transfer that will pass through the `finally` suite.
    fn record_terminal_finally_entry(&mut self, snapshot: FlowSnapshot) {
        self.terminal_finally_entry_snapshots.push(snapshot);
    }
}
