use std::cell::{RefCell, RefMut};

use super::use_def::FlowSnapshot;
use super::SemanticIndexBuilder;

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
        self.0.pop();
    }
}

#[derive(Debug, Default)]
pub(super) struct TryBlockContext {
    snapshots: Vec<FlowSnapshot>,
    visiting_nested_try_stmt: bool,
}

impl TryBlockContext {
    pub(super) fn record_definition(&mut self, builder: &SemanticIndexBuilder) {
        if !self.visiting_nested_try_stmt {
            self.snapshots.push(builder.flow_snapshot());
        }
    }

    pub(super) fn record_visiting_nested_try_stmt(&mut self) {
        self.visiting_nested_try_stmt = true;
    }

    pub(super) fn record_exiting_nested_try_stmt(&mut self) {
        self.visiting_nested_try_stmt = false;
    }

    pub(super) fn snapshots(&self) -> &[FlowSnapshot] {
        &self.snapshots
    }

    pub(super) fn into_snapshots(self) -> Vec<FlowSnapshot> {
        self.snapshots
    }
}

#[derive(Debug)]
pub(super) struct TryBlockContextsRefMut<'a>(RefMut<'a, Vec<TryBlockContext>>);

impl<'a> TryBlockContextsRefMut<'a> {
    pub(super) fn current_try_block(&mut self) -> Option<&mut TryBlockContext> {
        self.0.last_mut()
    }
}

#[derive(Debug, Default)]
pub(super) struct TryBlockContexts(RefCell<Vec<TryBlockContext>>);

impl TryBlockContexts {
    pub(super) fn push_try_block(&self) {
        self.0.borrow_mut().push(TryBlockContext::default());
    }

    pub(super) fn pop_try_block(&self) -> Option<TryBlockContext> {
        self.0.borrow_mut().pop()
    }

    pub(super) fn borrow_mut(&self) -> TryBlockContextsRefMut {
        TryBlockContextsRefMut(self.0.borrow_mut())
    }
}
