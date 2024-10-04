use std::cell::{RefCell, RefMut};

use super::SemanticIndexBuilder;
use crate::semantic_index::use_def::FlowSnapshot;

#[derive(Debug, Default)]
pub(super) struct TryBlockContextsStack(Vec<TryBlockContexts>);

impl TryBlockContextsStack {
    pub(super) fn push_context(&mut self) {
        self.0.push(TryBlockContexts::default());
    }

    pub(super) fn current_try_block_context(&self) -> &TryBlockContexts {
        self.0
            .last()
            .expect("There should always be at least one `TryBlockContexts` on the stack")
    }

    pub(super) fn pop_context(&mut self) {
        let popped_context = self.0.pop();
        assert_ne!(
            popped_context, None,
            "pop_context() should never be called on an empty stack \
(this indicates an unbalanced `push_context()`/`pop_context()` pair of calls)"
        );
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TryBlockContexts(RefCell<Vec<TryBlockContext>>);

impl TryBlockContexts {
    pub(super) fn push_try_block(&self) {
        self.0.borrow_mut().push(TryBlockContext::default());
    }

    pub(super) fn pop_try_block(&self) -> Option<TryBlockContext> {
        self.0.borrow_mut().pop()
    }

    pub(super) fn borrow_mut(&self) -> TryBlockContextRefMut {
        TryBlockContextRefMut(self.0.borrow_mut())
    }
}

#[derive(Debug)]
pub(super) struct TryBlockContextRefMut<'a>(RefMut<'a, Vec<TryBlockContext>>);

impl<'a> TryBlockContextRefMut<'a> {
    /// For each `try` block on the stack, push the snapshot onto the `try` block
    pub(super) fn record_definitions_state(&mut self, builder: &SemanticIndexBuilder) {
        if self.0.is_empty() {
            return;
        }
        let snapshot = builder.flow_snapshot();
        for context in self.0.iter_mut() {
            context.push_snapshot(snapshot.clone());
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TryBlockContext(Vec<FlowSnapshot>);

impl TryBlockContext {
    fn push_snapshot(&mut self, snapshot: FlowSnapshot) {
        self.0.push(snapshot);
    }
}
