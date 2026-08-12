use crate::use_def::FlowSnapshot;

use super::SemanticIndexBuilder;

/// Active exception handlers and the flow states from which they can be entered.
#[derive(Debug, Default)]
pub(super) enum ExceptionHandlers {
    /// No handlers are active, including after their snapshots have been taken.
    #[default]
    None,
    /// Handlers may catch the exception, but it can also propagate to an enclosing handler.
    Propagating(Vec<FlowSnapshot>),
    /// A bare handler catches every exception and stops outward propagation.
    CatchAll(Vec<FlowSnapshot>),
}

impl ExceptionHandlers {
    pub(super) fn propagating() -> Self {
        Self::Propagating(Vec::new())
    }

    pub(super) fn catch_all() -> Self {
        Self::CatchAll(Vec::new())
    }
}

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
    ///
    /// Only suites with handlers collect exception checkpoints; a bare handler prevents those
    /// exceptions from propagating to enclosing suites.
    pub(super) fn push_context(&mut self, exception_handlers: ExceptionHandlers) {
        self.current_try_context_stack()
            .push_context(exception_handlers);
    }

    /// Pop a [`TryNodeContext`] off the [`TryNodeContextStack`] at the top of our stack of stacks.
    pub(super) fn pop_context(&mut self) -> TryNodeContext {
        self.current_try_context_stack().pop_context()
    }

    /// Retrieve the [`TryNodeContext`] that is currently at the top of the stack, and take all
    /// snapshots recorded while visiting the `try` suite.
    ///
    /// Taking the snapshots deactivates the suite's handlers before their bodies are visited.
    pub(super) fn take_try_suite_snapshots(&mut self) -> Vec<FlowSnapshot> {
        self.current_try_context_stack().take_try_suite_snapshots()
    }

    /// Record a checkpoint for every active `try` suite that could handle an exception raised at
    /// the current point in control flow.
    ///
    /// Crosses eager scopes, but stops at lazy scopes, unreachable flow, and bare handlers.
    pub(super) fn record_exception_checkpoint(&mut self, builder: &mut SemanticIndexBuilder) {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        for (scope_stack_index, try_context_stack) in self.0.iter_mut().enumerate().rev() {
            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            let snapshot = builder.use_def_maps[scope_id].snapshot();

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
        }
    }

    /// Returns whether an active `try` suite can receive an exception from the current scope.
    ///
    /// A context can remain on the stack for its `finally` suite after its handlers become inactive.
    pub(super) fn has_active_exception_handler(&self, builder: &SemanticIndexBuilder) -> bool {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        for (scope_stack_index, try_context_stack) in self.0.iter().enumerate().rev() {
            if try_context_stack.has_active_exception_handler() {
                return true;
            }

            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            if !builder.exception_checkpoint_crosses_scope_boundary(scope_id) {
                return false;
            }
        }

        false
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
    /// Returns whether a `try` suite in this scope is still collecting exception checkpoints.
    fn has_active_exception_handler(&self) -> bool {
        self.0.iter().any(|context| {
            matches!(
                context.exception_handlers,
                ExceptionHandlers::Propagating(_) | ExceptionHandlers::CatchAll(_)
            )
        })
    }

    /// Push a new [`TryNodeContext`] for recording exception checkpoints and terminal entries
    /// while visiting a [`ruff_python_ast::StmtTry`] node.
    fn push_context(&mut self, exception_handlers: ExceptionHandlers) {
        self.0.push(TryNodeContext {
            exception_handlers,
            ..TryNodeContext::default()
        });
    }

    /// Pop a [`TryNodeContext`] off the stack.
    fn pop_context(&mut self) -> TryNodeContext {
        self.0
            .pop()
            .expect("Cannot pop a `try` block off an empty `TryBlockContexts` stack")
    }

    /// Take all snapshots recorded while visiting the `try` suite and deactivate its handlers.
    fn take_try_suite_snapshots(&mut self) -> Vec<FlowSnapshot> {
        let context = self
            .0
            .last_mut()
            .expect("Cannot take snapshots from an empty `TryBlockContexts` stack");
        match std::mem::take(&mut context.exception_handlers) {
            ExceptionHandlers::None => Vec::new(),
            ExceptionHandlers::Propagating(snapshots) | ExceptionHandlers::CatchAll(snapshots) => {
                snapshots
            }
        }
    }

    /// Records the checkpoint for all enclosing active `try` suites in this scope. Returns whether
    /// the checkpoint should continue propagating to an enclosing scope.
    ///
    /// A bare handler consumes the exception, preventing any outer handler from seeing it.
    fn record_exception_checkpoint(&mut self, snapshot: &FlowSnapshot) -> bool {
        for context in self.0.iter_mut().rev() {
            match &mut context.exception_handlers {
                ExceptionHandlers::None => {}
                ExceptionHandlers::Propagating(snapshots) => snapshots.push(snapshot.clone()),
                ExceptionHandlers::CatchAll(snapshots) => {
                    snapshots.push(snapshot.clone());
                    return false;
                }
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
    exception_handlers: ExceptionHandlers,
    terminal_finally_entry_snapshots: Vec<FlowSnapshot>,
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
