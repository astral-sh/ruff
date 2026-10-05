//! Passive measurements of searches performed while binding one initializer.

use std::cell::RefCell;
use std::rc::Rc;

use super::{Observation, SESSION, observe, observing};

thread_local! {
    static CURRENT: RefCell<Option<Rc<RefCell<SearchState>>>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::types) struct SearchStatistics {
    pub(super) scope: usize,
    pub(super) visits: usize,
    pub(super) peak_depth: usize,
    pub(super) stack_span: usize,
    pub(super) debits_before: usize,
    pub(super) debits_after: usize,
    pub(super) expansions_before: usize,
    pub(super) expansions_after: usize,
}

struct SearchState {
    statistics: SearchStatistics,
    depth: usize,
    origin: Option<usize>,
}

pub(in crate::types) struct SearchScope {
    state: Option<Rc<RefCell<SearchState>>>,
    previous: Option<Rc<RefCell<SearchState>>>,
}

fn counts() -> (usize, usize) {
    SESSION.with(|current| {
        current.borrow().as_ref().map_or((0, 0), |state| {
            let state = state.borrow();
            (state.statistics.debits, state.statistics.expansions)
        })
    })
}

pub(in crate::types) fn initializer(owner: Option<salsa::Id>) -> SearchScope {
    if !observing() {
        return SearchScope {
            state: None,
            previous: None,
        };
    }
    let (debits_before, expansions_before) = counts();
    let scope = SESSION.with(|current| {
        current.borrow().as_ref().map_or(0, |state| {
            state
                .borrow()
                .statistics
                .observations
                .as_ref()
                .map_or(0, Vec::len)
        })
    });
    let state = Rc::new(RefCell::new(SearchState {
        statistics: SearchStatistics {
            scope,
            debits_before,
            expansions_before,
            ..SearchStatistics::default()
        },
        depth: 0,
        origin: None,
    }));
    let previous = CURRENT.with(|current| current.replace(Some(Rc::clone(&state))));
    observe(Observation::SearchStarted {
        scope,
        owner,
        parent: previous
            .as_ref()
            .map(|previous| previous.borrow().statistics.scope),
    });
    SearchScope {
        state: Some(state),
        previous,
    }
}

impl Drop for SearchScope {
    fn drop(&mut self) {
        let Some(state) = &self.state else { return };
        let current = CURRENT.with(|current| current.replace(self.previous.take()));
        assert!(
            current
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, state))
        );
        let mut state = state.borrow_mut();
        assert_eq!(state.depth, 0);
        let (debits_after, expansions_after) = counts();
        state.statistics.debits_after = debits_after;
        state.statistics.expansions_after = expansions_after;
        observe(Observation::SearchFinished(state.statistics));
    }
}

pub(in crate::types) struct Visit(Option<Rc<RefCell<SearchState>>>);

pub(in crate::types) fn visit() -> Visit {
    let state = CURRENT.with(|current| current.borrow().clone());
    if let Some(state) = &state {
        let marker = 0u8;
        let position = (&marker as *const u8) as usize;
        let mut state = state.borrow_mut();
        let origin = *state.origin.get_or_insert(position);
        state.depth += 1;
        state.statistics.visits += 1;
        state.statistics.peak_depth = state.statistics.peak_depth.max(state.depth);
        state.statistics.stack_span = state.statistics.stack_span.max(origin.abs_diff(position));
    }
    Visit(state)
}

impl Drop for Visit {
    fn drop(&mut self) {
        if let Some(state) = &self.0 {
            state.borrow_mut().depth -= 1;
        }
    }
}

pub(super) fn assert_released() {
    CURRENT.with(|current| assert!(current.borrow().is_none()));
}
