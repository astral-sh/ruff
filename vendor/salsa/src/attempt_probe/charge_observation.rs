use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RefusedCharge {
    pub(crate) units: usize,
    pub(crate) remaining: usize,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) first: Cell<Option<RefusedCharge>>,
}

thread_local! {
    static OBSERVER: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

/// Records the first work debit rejected for insufficient remaining allowance.
/// The observer only copies scalar data; it cannot accept or reject a charge.
/// Retain the returned guard to keep observing this thread; dropping it restores the prior observer.
pub(crate) fn install(state: Rc<State>) -> impl Drop {
    struct Reset(Option<Rc<State>>);

    impl Drop for Reset {
        fn drop(&mut self) {
            OBSERVER.with_borrow_mut(|slot| *slot = self.0.take());
        }
    }

    Reset(OBSERVER.with_borrow_mut(|slot| slot.replace(state)))
}

pub(super) fn refused(units: usize, remaining: usize) {
    OBSERVER.with_borrow(|slot| {
        if let Some(state) = slot
            && state.first.get().is_none()
        {
            state.first.set(Some(RefusedCharge { units, remaining }));
        }
    });
}
