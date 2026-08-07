use smallvec::SmallVec;

use crate::scope::ScopeKind;
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

    /// Push a `try` context onto the [`TryNodeContextStack`] at the top of our stack of stacks.
    ///
    /// Only suites with handlers collect exception checkpoints; a bare handler prevents those
    /// exceptions from propagating to enclosing suites.
    pub(super) fn push_try_context(&mut self, exception_handlers: ExceptionHandlers) {
        self.current_try_context_stack()
            .push_context(TryNodeContext {
                exception_handlers,
                kind: ContextKind::Try(Vec::new()),
            });
    }

    /// Push a `with` context that collects exception checkpoints but does not own a `finally`.
    pub(super) fn push_with_context(&mut self) {
        self.current_try_context_stack()
            .push_context(TryNodeContext {
                exception_handlers: ExceptionHandlers::propagating(),
                kind: ContextKind::With,
            });
    }

    /// Pop the current context and return any terminal entries belonging to its `finally` suite.
    pub(super) fn pop_context(&mut self) -> Vec<FlowSnapshot> {
        self.current_try_context_stack()
            .pop_context()
            .into_terminal_finally_entry_snapshots()
    }

    /// Retrieve the [`TryNodeContext`] that is currently at the top of the stack, and take all
    /// exception checkpoints recorded while visiting its suite.
    ///
    /// Taking the snapshots deactivates the suite's handlers so they cannot catch later code.
    pub(super) fn take_exception_snapshots(&mut self) -> Vec<FlowSnapshot> {
        self.current_try_context_stack().take_exception_snapshots()
    }

    /// Record a checkpoint for every active `try` or `with` suite that could handle an exception
    /// raised at the current point in control flow.
    ///
    /// Crosses eager scopes, but stops at lazy scopes, unreachable flow, and bare handlers.
    /// Comprehension bindings are materialized only for scopes that actually have active handlers.
    pub(super) fn record_exception_checkpoint(&mut self, builder: &mut SemanticIndexBuilder) {
        debug_assert_eq!(self.0.len(), builder.scope_stack.len());

        let mut crossed_comprehensions = SmallVec::<[usize; 2]>::new();

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
                crossed_comprehensions.push(scope_stack_index);
            }
        }
    }

    /// Returns whether any enclosing `try` or `with` suite can receive exception checkpoints.
    ///
    /// A context can remain on the stack for its `finally` suite after its handlers become inactive.
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

/// The contexts of nested `try` and `with` suites for a single scope.
#[derive(Debug, Default)]
struct TryNodeContextStack(Vec<TryNodeContext>);

impl TryNodeContextStack {
    /// Returns whether a suite in this scope is still collecting exception checkpoints.
    fn has_active_exception_handler(&self) -> bool {
        self.0.iter().any(|context| {
            matches!(
                context.exception_handlers,
                ExceptionHandlers::Propagating(_) | ExceptionHandlers::CatchAll(_)
            )
        })
    }

    /// Push a context for recording exception checkpoints and, for `try`, terminal entries.
    fn push_context(&mut self, context: TryNodeContext) {
        self.0.push(context);
    }

    /// Pop a [`TryNodeContext`] off the stack.
    fn pop_context(&mut self) -> TryNodeContext {
        self.0
            .pop()
            .expect("Cannot pop a `try` block off an empty `TryBlockContexts` stack")
    }

    /// Take all snapshots recorded while visiting the suite and deactivate its handlers.
    fn take_exception_snapshots(&mut self) -> Vec<FlowSnapshot> {
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

    /// Records the checkpoint for all active handlers in this scope. Returns whether the
    /// checkpoint should continue propagating to an enclosing scope.
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

    /// Push the snapshot onto the innermost `try` block, skipping intervening `with` contexts.
    fn record_terminal_finally_entry(&mut self, builder: &SemanticIndexBuilder) {
        for context in self.0.iter_mut().rev() {
            if let ContextKind::Try(terminal_finally_entry_snapshots) = &mut context.kind {
                terminal_finally_entry_snapshots.push(builder.flow_snapshot());
                break;
            }
        }
    }
}

/// Whether an exception-handling context also owns terminal entries for a `finally` suite.
#[derive(Debug)]
enum ContextKind {
    Try(Vec<FlowSnapshot>),
    With,
}

/// Exception checkpoints for a `try` or `with` suite.
#[derive(Debug)]
struct TryNodeContext {
    exception_handlers: ExceptionHandlers,
    kind: ContextKind,
}

impl TryNodeContext {
    fn into_terminal_finally_entry_snapshots(self) -> Vec<FlowSnapshot> {
        match self.kind {
            ContextKind::Try(snapshots) => snapshots,
            ContextKind::With => Vec::new(),
        }
    }
}
