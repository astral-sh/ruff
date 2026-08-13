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

/// Maintains a separate [`ExceptionContextStack`] for each scope.
#[derive(Debug, Default)]
pub(super) struct ExceptionContextStackManager(Vec<ExceptionContextStack>);

impl ExceptionContextStackManager {
    /// Push a new [`ExceptionContextStack`] onto the stack of stacks.
    ///
    /// Each [`ExceptionContextStack`] is only valid for a single scope.
    pub(super) fn enter_nested_scope(&mut self) {
        self.0.push(ExceptionContextStack::default());
    }

    /// Pop an [`ExceptionContextStack`] off the stack of stacks.
    ///
    /// Each [`ExceptionContextStack`] is only valid for a single scope.
    pub(super) fn exit_scope(&mut self) {
        let popped_context = self.0.pop();
        debug_assert!(
            popped_context.is_some(),
            "exit_scope() should never be called on an empty stack \
(this indicates an unbalanced `enter_nested_scope()`/`exit_scope()` pair of calls)"
        );
    }

    /// Push an [`ExceptionContext`] onto the [`ExceptionContextStack`] at the top of our stack of
    /// stacks.
    ///
    /// Only suites with handlers collect exception checkpoints; a bare handler prevents those
    /// exceptions from propagating to enclosing suites.
    pub(super) fn push_context(&mut self, exception_handlers: ExceptionHandlers) {
        self.current_exception_context_stack()
            .push_context(exception_handlers);
    }

    /// Registers a context manager after it enters but before its target is assigned.
    pub(super) fn push_with_context(&mut self) {
        self.current_exception_context_stack()
            .0
            .push(ExceptionContext {
                exception_handlers: ExceptionHandlers::propagating(),
                kind: ExceptionContextKind::With,
                ..ExceptionContext::default()
            });
    }

    /// Removes the innermost context manager and returns the exceptions it could suppress.
    ///
    /// Removing the context before its exit method runs prevents it from suppressing exceptions
    /// raised by its own exit method.
    pub(super) fn finish_with_context(&mut self) -> Vec<FlowSnapshot> {
        let stack = self.current_exception_context_stack();
        let snapshots = stack.take_exception_snapshots();
        let context = stack.pop_context();
        debug_assert!(matches!(context.kind, ExceptionContextKind::With));
        snapshots
    }

    /// Pop an [`ExceptionContext`] off the stack for the current scope.
    pub(super) fn pop_context(&mut self) -> ExceptionContext {
        self.current_exception_context_stack().pop_context()
    }

    /// Retrieve the [`ExceptionContext`] at the top of the stack, and take all
    /// snapshots recorded while visiting the `try` suite.
    ///
    /// Taking the snapshots deactivates the suite's handlers before their bodies are visited.
    pub(super) fn end_try_suite(&mut self) -> Vec<FlowSnapshot> {
        self.current_exception_context_stack()
            .take_exception_snapshots()
    }

    /// Records a checkpoint for every active `try` or `with` context that could handle an
    /// exception raised at the current point in control flow.
    ///
    /// Crosses eager scopes, but stops at lazy scopes, unreachable flow, and bare handlers.
    pub(super) fn record_exception_checkpoint(&mut self, builder: &mut SemanticIndexBuilder) {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        for (scope_stack_index, exception_context_stack) in self.0.iter_mut().enumerate().rev() {
            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            let use_def_map = &builder.use_def_maps[scope_id];

            // Each scope has an independent flow state, so an enclosing scope can still be
            // reachable while we analyze an unreachable nested scope.
            if use_def_map.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
                break;
            }

            if !exception_context_stack.record_exception_checkpoint(use_def_map) {
                break;
            }

            if !builder.exception_checkpoint_crosses_scope_boundary(scope_id) {
                break;
            }
        }
    }

    /// Returns whether an active `try` or `with` context can receive an exception from this scope.
    ///
    /// A context can remain on the stack for its `finally` suite after its handlers become inactive.
    pub(super) fn has_active_exception_handler(&self, builder: &SemanticIndexBuilder) -> bool {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        for (scope_stack_index, exception_context_stack) in self.0.iter().enumerate().rev() {
            if exception_context_stack.has_active_exception_handler() {
                return true;
            }

            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            if !builder.exception_checkpoint_crosses_scope_boundary(scope_id) {
                return false;
            }
        }

        false
    }

    /// Returns whether an enclosing context manager has already seen an exception checkpoint.
    pub(super) fn has_with_exception_checkpoint(&self) -> bool {
        self.0.last().is_some_and(|stack| {
            stack.0.iter().any(|context| {
                matches!(context.kind, ExceptionContextKind::With)
                    && context.last_checkpoint_key.is_some()
            })
        })
    }

    /// Records the enclosing `try`'s definition cursor when suppression bypasses a terminal body.
    pub(super) fn record_suppressed_terminal_with_exit(
        &mut self,
        next_definition_id: ScopedDefinitionId,
    ) {
        if let Some(context) = self.current_exception_context_stack().innermost_try() {
            context.suppressed_terminal_with_exit = Some(next_definition_id);
        }
    }

    /// Forwards suspended terminal states through a nested `try` without its own cleanup.
    pub(super) fn propagate_suppressed_terminal_with_exit(
        &mut self,
        next_definition_id: ScopedDefinitionId,
        terminal_snapshots: Vec<FlowSnapshot>,
    ) {
        if let Some(context) = self.current_exception_context_stack().innermost_try() {
            context.suppressed_terminal_with_exit = Some(next_definition_id);
            context
                .terminal_finally_entry_snapshots
                .extend(terminal_snapshots);
        }
    }

    /// Removes the suppression marker for the current `try` control-flow branch.
    pub(super) fn take_suppressed_terminal_with_exit(&mut self) -> Option<ScopedDefinitionId> {
        self.current_exception_context_stack()
            .innermost_try()
            .and_then(|context| context.suppressed_terminal_with_exit.take())
    }

    /// Records a terminal entry for the nearest enclosing `try`, skipping `with` contexts.
    pub(super) fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        self.current_exception_context_stack()
            .record_terminal_finally_entry(builder);
    }

    /// Retrieve the [`ExceptionContextStack`] that is relevant for the current scope.
    fn current_exception_context_stack(&mut self) -> &mut ExceptionContextStack {
        self.0
            .last_mut()
            .expect("There should always be at least one `ExceptionContextStack` on the stack")
    }
}

/// The contexts of nested `try` and `with` statements for a single scope.
#[derive(Debug, Default)]
struct ExceptionContextStack(Vec<ExceptionContext>);

impl ExceptionContextStack {
    /// Returns whether a `try` or `with` context is still collecting exception checkpoints.
    fn has_active_exception_handler(&self) -> bool {
        self.0
            .iter()
            .any(|context| context.exception_handlers.is_active())
    }

    /// Push a new [`ExceptionContext`] for recording exception checkpoints and terminal entries
    /// while visiting a [`ruff_python_ast::StmtTry`] node.
    fn push_context(&mut self, exception_handlers: ExceptionHandlers) {
        self.0.push(ExceptionContext {
            exception_handlers,
            ..ExceptionContext::default()
        });
    }

    /// Pop an [`ExceptionContext`] off the stack.
    fn pop_context(&mut self) -> ExceptionContext {
        self.0
            .pop()
            .expect("Cannot pop an exception context off an empty `ExceptionContextStack`")
    }

    /// Takes all snapshots recorded by the innermost context and deactivates its handlers.
    fn take_exception_snapshots(&mut self) -> Vec<FlowSnapshot> {
        let context = self
            .0
            .last_mut()
            .expect("Cannot take snapshots from an empty `ExceptionContextStack`");
        match std::mem::take(&mut context.exception_handlers) {
            ExceptionHandlers::None => Vec::new(),
            ExceptionHandlers::Propagating(snapshots) | ExceptionHandlers::CatchAll(snapshots) => {
                snapshots
            }
        }
    }

    /// Records a checkpoint for every active `try` or `with` context in this scope.
    /// Returns whether the checkpoint should continue propagating to an enclosing scope.
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

    /// Records a terminal entry for the nearest `try` context, skipping intervening `with` contexts.
    fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        if let Some(context) = self.innermost_try() {
            context
                .terminal_finally_entry_snapshots
                .push(builder.flow_snapshot());
        }
    }

    /// Finds the nearest enclosing `try`, skipping context managers.
    fn innermost_try(&mut self) -> Option<&mut ExceptionContext> {
        self.0
            .iter_mut()
            .rev()
            .find(|context| matches!(context.kind, ExceptionContextKind::Try))
    }
}

/// Distinguishes `try` contexts that may own a `finally` suite from `with` contexts.
#[derive(Debug, Default)]
enum ExceptionContextKind {
    #[default]
    Try,
    With,
}

/// Exception-entry states for one `try` or `with` statement.
///
/// Only `try` contexts also collect terminal entries for a `finally` suite.
#[derive(Debug, Default)]
pub(super) struct ExceptionContext {
    exception_handlers: ExceptionHandlers,
    kind: ExceptionContextKind,
    last_checkpoint_key: Option<(ScopedDefinitionId, ControlFlowRevision)>,
    /// Whether an exception escaped this suite and must also propagate after its cleanup.
    has_escaping_exception: bool,
    /// Definition cursor at a possible suppression continuation from a terminal manager body.
    suppressed_terminal_with_exit: Option<ScopedDefinitionId>,
    terminal_finally_entry_snapshots: Vec<FlowSnapshot>,
}

impl ExceptionContext {
    pub(super) fn into_finally_entry_state(self) -> (Vec<FlowSnapshot>, bool) {
        (
            self.terminal_finally_entry_snapshots,
            self.has_escaping_exception,
        )
    }
}
