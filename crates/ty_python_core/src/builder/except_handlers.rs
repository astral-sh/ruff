use crate::reachability_constraints::ScopedReachabilityConstraintId;
use crate::use_def::{ControlFlowRevision, FlowSnapshot, ScopedDefinitionId, UseDefMapBuilder};

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

    fn is_active(&self) -> bool {
        !matches!(self, Self::None)
    }

    fn is_catch_all(&self) -> bool {
        matches!(self, Self::CatchAll(_))
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
    pub(super) fn end_try_suite(&mut self) -> Vec<FlowSnapshot> {
        self.current_try_context_stack().end_try_suite()
    }

    /// Record a checkpoint for every active `try` suite that could handle an exception raised at
    /// the current point in control flow.
    ///
    /// Crosses eager scopes, but stops at lazy scopes, unreachable flow, and bare handlers.
    pub(super) fn record_exception_checkpoint(&mut self, builder: &mut SemanticIndexBuilder) {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        for (scope_stack_index, try_context_stack) in self.0.iter_mut().enumerate().rev() {
            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            let use_def_map = &builder.use_def_maps[scope_id];

            // Each scope has an independent flow state, so an enclosing scope can still be
            // reachable while we analyze an unreachable nested scope.
            if use_def_map.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
                break;
            }

            if !try_context_stack.record_exception_checkpoint(use_def_map) {
                break;
            }

            if !builder.exception_checkpoint_crosses_scope_boundary(scope_id) {
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
        self.0
            .iter()
            .any(|context| context.exception_handlers.is_active())
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
    fn end_try_suite(&mut self) -> Vec<FlowSnapshot> {
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
    /// A bare handler consumes the exception, preventing any outer handler from seeing it. The
    /// snapshot is constructed only if a handler has not already observed the current flow state.
    fn record_exception_checkpoint(&mut self, use_def_map: &UseDefMapBuilder<'_>) -> bool {
        let checkpoint_key = use_def_map.exception_checkpoint_key();
        let mut snapshot = None;

        for context in self.0.iter_mut().rev() {
            match &mut context.exception_handlers {
                ExceptionHandlers::None => context.has_escaping_exception = true,
                ExceptionHandlers::Propagating(snapshots)
                | ExceptionHandlers::CatchAll(snapshots) => {
                    if context.last_checkpoint_key != Some(checkpoint_key) {
                        snapshots.push(
                            snapshot
                                .get_or_insert_with(|| use_def_map.snapshot())
                                .clone(),
                        );
                        context.last_checkpoint_key = Some(checkpoint_key);
                    }
                    if context.exception_handlers.is_catch_all() {
                        return false;
                    }
                    context.has_escaping_exception = true;
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
    last_checkpoint_key: Option<(ScopedDefinitionId, ControlFlowRevision)>,
    /// Whether an exception escaped this suite and must also propagate after its cleanup.
    has_escaping_exception: bool,
    terminal_finally_entry_snapshots: Vec<FlowSnapshot>,
}

impl TryNodeContext {
    pub(super) fn into_finally_entry_state(self) -> (Vec<FlowSnapshot>, bool) {
        (
            self.terminal_finally_entry_snapshots,
            self.has_escaping_exception,
        )
    }

    /// Take a record of what the internal state looked like before a terminal control-flow
    /// transfer that will pass through the `finally` suite.
    fn record_terminal_finally_entry(&mut self, snapshot: FlowSnapshot) {
        self.terminal_finally_entry_snapshots.push(snapshot);
    }
}
