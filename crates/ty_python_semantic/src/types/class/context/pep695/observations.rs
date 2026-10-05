//! Passive entry-lifetime observations for canonical class-header inference controls.

use std::cell::Cell;
use std::future::{Future, poll_fn};

/// Marks retained source and the actual header future's suspension and exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Stage {
    SourceRetained,
    HeaderEntered,
    HeaderPending,
    HeaderReady,
    HeaderRetired,
    SourceRetiring,
}

type Observer = fn(salsa::Id, Stage);

thread_local! {
    static OBSERVER: Cell<Option<Observer>> = const { Cell::new(None) };
}

pub(in crate::types) fn set_observer(observer: Option<Observer>) -> Option<Observer> {
    OBSERVER.with(|slot| slot.replace(observer))
}

fn observe(class: salsa::Id, stage: Stage) {
    OBSERVER.with(|slot| {
        if let Some(observer) = slot.get() {
            observer(class, stage);
        }
    });
}

/// Records source-owner retirement entry; the catalog may continue retaining the module afterward.
#[derive(Debug)]
pub(in crate::types) struct SourceLifetime(salsa::Id);

impl SourceLifetime {
    pub(in crate::types) fn new(class: salsa::Id) -> Self {
        observe(class, Stage::SourceRetained);
        Self(class)
    }
}

impl Drop for SourceLifetime {
    fn drop(&mut self) {
        observe(self.0, Stage::SourceRetiring);
    }
}

/// Marks exit from a polled header future before its enclosing source owner starts retirement.
#[derive(Debug)]
struct HeaderLifetime(salsa::Id);

impl Drop for HeaderLifetime {
    fn drop(&mut self) {
        observe(self.0, Stage::HeaderRetired);
    }
}

/// Forwards every poll unchanged and records only `Pending` actually returned by the header future.
pub(in crate::types) async fn header<F: Future>(class: salsa::Id, future: F) -> F::Output {
    observe(class, Stage::HeaderEntered);
    let _lifetime = HeaderLifetime(class);
    let mut future = std::pin::pin!(future);
    poll_fn(|context| {
        let result = future.as_mut().poll(context);
        if result.is_pending() {
            observe(class, Stage::HeaderPending);
        } else {
            observe(class, Stage::HeaderReady);
        }
        result
    })
    .await
}
