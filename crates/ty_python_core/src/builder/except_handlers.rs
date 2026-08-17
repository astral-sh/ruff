use crate::reachability_constraints::ScopedReachabilityConstraintId;
use crate::use_def::{ExceptionCheckpointKey, FlowSnapshot, UseDefMapBuilder};

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
pub(super) struct ExceptionContextStackManager {
    stacks: Vec<ExceptionContextStack>,
    /// Number of `try` and `with` contexts still collecting exception checkpoints.
    active_handler_count: usize,
}

impl ExceptionContextStackManager {
    /// Push a new [`ExceptionContextStack`] onto the stack of stacks.
    ///
    /// Each [`ExceptionContextStack`] is only valid for a single scope.
    pub(super) fn enter_nested_scope(&mut self) {
        self.stacks.push(ExceptionContextStack::default());
    }

    /// Pop an [`ExceptionContextStack`] off the stack of stacks.
    ///
    /// Each [`ExceptionContextStack`] is only valid for a single scope.
    pub(super) fn exit_scope(&mut self) {
        let popped_context = self.stacks.pop();
        debug_assert!(
            popped_context.is_some(),
            "exit_scope() should never be called on an empty stack \
(this indicates an unbalanced `enter_nested_scope()`/`exit_scope()` pair of calls)"
        );
    }

    /// Registers a `try` statement on the current scope's exception-context stack.
    ///
    /// Only suites with handlers collect exception checkpoints; a bare handler prevents those
    /// exceptions from propagating to enclosing suites.
    pub(super) fn push_try_context(
        &mut self,
        exception_handlers: ExceptionHandlers,
        has_finally: bool,
    ) {
        self.active_handler_count += usize::from(exception_handlers.is_active());
        self.current_exception_context_stack()
            .push_try_context(exception_handlers, has_finally);
    }

    /// Registers a context manager after it enters but before its target is assigned.
    pub(super) fn push_context_manager_context(&mut self) {
        self.active_handler_count += 1;
        self.current_exception_context_stack()
            .push_context_manager_context();
    }

    /// Removes the innermost context manager and returns the exceptions it could suppress.
    ///
    /// Removing the context before its exit method runs prevents it from suppressing exceptions
    /// raised by its own exit method.
    pub(super) fn finish_context_manager_context(&mut self) -> Vec<FlowSnapshot> {
        let snapshots = self.take_exception_snapshots();
        let context = self.current_exception_context_stack().pop_context();
        debug_assert!(matches!(context.kind, ExceptionContextKind::With));
        snapshots
    }

    /// Removes the current `try` context after its handlers have been deactivated.
    pub(super) fn pop_try_context(&mut self) -> ExceptionContext {
        let context = self.current_exception_context_stack().pop_context();
        debug_assert!(matches!(context.kind, ExceptionContextKind::Try { .. }));
        debug_assert!(!context.exception_handlers.is_active());
        context
    }

    /// Retrieve the [`ExceptionContext`] at the top of the stack, and take all
    /// snapshots recorded while visiting the `try` suite.
    ///
    /// Taking the snapshots deactivates the suite's handlers before their bodies are visited.
    pub(super) fn end_try_suite(&mut self) -> Vec<FlowSnapshot> {
        self.take_exception_snapshots()
    }

    /// Records a checkpoint for every active `try` or `with` context that could handle an
    /// exception raised at the current point in control flow.
    ///
    /// Crosses eager scopes, but stops at lazy scopes, unreachable flow, and bare handlers.
    pub(super) fn record_exception_checkpoint(&mut self, builder: &mut SemanticIndexBuilder) {
        debug_assert_eq!(self.stacks.len(), builder.scope_stack.len());

        let mut has_intervening_finally = false;
        for (scope_stack_index, exception_context_stack) in self.stacks.iter_mut().enumerate().rev()
        {
            let scope_id = builder.scope_stack[scope_stack_index].file_scope_id;
            let use_def_map = &builder.use_def_maps[scope_id];

            // Each scope has an independent flow state, so an enclosing scope can still be
            // reachable while we analyze an unreachable nested scope.
            if use_def_map.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
                break;
            }

            if !exception_context_stack
                .record_exception_checkpoint(use_def_map, &mut has_intervening_finally)
            {
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
        if self.active_handler_count == 0 {
            return false;
        }

        debug_assert_eq!(self.stacks.len(), builder.scope_stack.len());

        for (scope_stack_index, exception_context_stack) in self.stacks.iter().enumerate().rev() {
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
    pub(super) fn has_context_manager_exception_checkpoint(&self) -> bool {
        self.stacks.last().is_some_and(|stack| {
            stack.0.iter().any(|context| {
                matches!(context.kind, ExceptionContextKind::With)
                    && context.last_checkpoint_key.is_some()
            })
        })
    }

    /// Records that a context manager makes an apparently terminal control-flow path possibly
    /// non-terminal because it may silence an earlier exception. Whether it actually suppresses
    /// exceptions is determined during type inference.
    pub(super) fn record_deferred_terminal_context_manager_exit(&mut self) {
        if let Some(context) = self.current_exception_context_stack().innermost_try() {
            context.has_deferred_terminal_context_manager_exit = true;
        }
    }

    /// Forwards a deferred terminal state to the nearest enclosing `try`.
    pub(super) fn propagate_deferred_terminal_context_manager_exit(
        &mut self,
        terminal_snapshot: FlowSnapshot,
    ) {
        if let Some(context) = self.current_exception_context_stack().innermost_try() {
            context.has_deferred_terminal_context_manager_exit = true;
            context
                .terminal_finally_entry_snapshots
                .push(terminal_snapshot);
        }
    }

    /// Records a terminal entry for the nearest enclosing `try`, skipping `with` contexts.
    pub(super) fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        self.current_exception_context_stack()
            .record_terminal_finally_entry(builder);
    }

    /// Takes the current context's snapshots and updates the number of active handlers.
    fn take_exception_snapshots(&mut self) -> Vec<FlowSnapshot> {
        if let Some(snapshots) = self
            .current_exception_context_stack()
            .take_exception_snapshots()
        {
            self.active_handler_count -= 1;
            snapshots
        } else {
            Vec::new()
        }
    }

    /// Retrieve the [`ExceptionContextStack`] that is relevant for the current scope.
    fn current_exception_context_stack(&mut self) -> &mut ExceptionContextStack {
        self.stacks
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

    /// Registers a `try` statement and whether exceptions must first pass through cleanup.
    fn push_try_context(&mut self, exception_handlers: ExceptionHandlers, has_finally: bool) {
        self.0.push(ExceptionContext::new(
            ExceptionContextKind::Try { has_finally },
            exception_handlers,
        ));
    }

    /// Registers a context manager that may receive exceptions from its body.
    fn push_context_manager_context(&mut self) {
        self.0.push(ExceptionContext::new(
            ExceptionContextKind::With,
            ExceptionHandlers::propagating(),
        ));
    }

    /// Pop an [`ExceptionContext`] off the stack.
    fn pop_context(&mut self) -> ExceptionContext {
        self.0
            .pop()
            .expect("Cannot pop an exception context off an empty `ExceptionContextStack`")
    }

    /// Takes the innermost context's snapshots if it has active handlers, deactivating them.
    fn take_exception_snapshots(&mut self) -> Option<Vec<FlowSnapshot>> {
        let context = self
            .0
            .last_mut()
            .expect("Cannot take snapshots from an empty `ExceptionContextStack`");
        match std::mem::take(&mut context.exception_handlers) {
            ExceptionHandlers::None => None,
            ExceptionHandlers::Propagating(snapshots) | ExceptionHandlers::CatchAll(snapshots) => {
                Some(snapshots)
            }
        }
    }

    /// Records a checkpoint for every active `try` or `with` context in this scope.
    /// Returns whether the checkpoint should continue propagating to an enclosing scope.
    ///
    /// A bare handler consumes the exception, preventing any outer handler from seeing it. A
    /// `finally` suite prevents enclosing context managers from receiving a checkpoint until its
    /// cleanup has run, while preserving existing outer-`try` handler behavior. The snapshot is
    /// constructed only if a handler has not already observed the current flow state.
    fn record_exception_checkpoint(
        &mut self,
        use_def_map: &UseDefMapBuilder<'_>,
        has_intervening_finally: &mut bool,
    ) -> bool {
        let checkpoint_key = use_def_map.exception_checkpoint_key();

        for context in self.0.iter_mut().rev() {
            if *has_intervening_finally && matches!(context.kind, ExceptionContextKind::With) {
                continue;
            }

            match &mut context.exception_handlers {
                ExceptionHandlers::None => context.has_escaping_exception = true,
                ExceptionHandlers::Propagating(snapshots)
                | ExceptionHandlers::CatchAll(snapshots) => {
                    if context.last_checkpoint_key != Some(checkpoint_key) {
                        snapshots.push(use_def_map.snapshot());
                        context.last_checkpoint_key = Some(checkpoint_key);
                    }
                    if context.exception_handlers.is_catch_all() {
                        return false;
                    }
                    context.has_escaping_exception = true;
                }
            }

            *has_intervening_finally |= matches!(
                context.kind,
                ExceptionContextKind::Try { has_finally: true }
            );
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
            .find(|context| matches!(context.kind, ExceptionContextKind::Try { .. }))
    }
}

/// Distinguishes `try` exception contexts from `with` exception contexts.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ExceptionContextKind {
    Try { has_finally: bool },
    With,
}

/// Exception-entry states for one `try` or `with` statement.
///
/// Only `try` contexts also collect terminal entries for a `finally` suite.
#[derive(Debug)]
pub(super) struct ExceptionContext {
    exception_handlers: ExceptionHandlers,
    kind: ExceptionContextKind,
    last_checkpoint_key: Option<ExceptionCheckpointKey>,
    /// Whether an exception escaped this suite and must also propagate after its cleanup.
    has_escaping_exception: bool,
    /// Whether apparently terminal control flow in a nested context-manager body, such as a
    /// `return` or `raise`, may become non-terminal if type inference determines that the context
    /// manager suppresses exceptions. This flag belongs to the enclosing `try` context because it
    /// affects control flow into its `finally` suite.
    has_deferred_terminal_context_manager_exit: bool,
    terminal_finally_entry_snapshots: Vec<FlowSnapshot>,
}

impl ExceptionContext {
    fn new(kind: ExceptionContextKind, exception_handlers: ExceptionHandlers) -> Self {
        Self {
            exception_handlers,
            kind,
            last_checkpoint_key: None,
            has_escaping_exception: false,
            has_deferred_terminal_context_manager_exit: false,
            terminal_finally_entry_snapshots: Vec::new(),
        }
    }

    pub(super) fn into_finally_entry_state(self) -> (Vec<FlowSnapshot>, bool, bool) {
        (
            self.terminal_finally_entry_snapshots,
            self.has_escaping_exception,
            self.has_deferred_terminal_context_manager_exit,
        )
    }
}
