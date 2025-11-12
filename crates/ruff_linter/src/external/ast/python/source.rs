use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ruff_source_file::SourceFile;
use scopeguard::guard;

use crate::Locator;

thread_local! {
    static CURRENT_SOURCE: RefCell<Option<SourceFileHandle>> = const { RefCell::new(None) };
}

/// Execute `f` with the given `SourceFile` installed as the current source for [`SourceFileHandle`].
/// Restores the previous value when `f` returns.
pub(crate) fn with_source_file<R>(source_file: &SourceFile, f: impl FnOnce() -> R) -> R {
    CURRENT_SOURCE.with(|cell| {
        debug_assert!(
            cell.borrow().as_ref().is_none(),
            "with_source_file is unexpectedly nested"
        );
        let handle = SourceFileHandle::from_source(source_file.clone());
        let previous = cell.replace(Some(handle.clone()));
        let _restore = guard((cell, previous), |(cell, previous)| {
            cell.replace(previous);
        });
        let result = f();
        handle.invalidate();
        result
    })
}

#[derive(Clone, Debug)]
struct SourceState {
    source_file: SourceFile,
    active: Cell<bool>,
}

impl SourceState {
    fn new(source_file: SourceFile) -> Self {
        Self {
            source_file,
            active: Cell::new(true),
        }
    }

    fn source_file(&self) -> &SourceFile {
        &self.source_file
    }

    fn invalidate(&self) {
        self.active.set(false);
    }

    fn is_active(&self) -> bool {
        self.active.get()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceFileHandle {
    inner: Rc<SourceState>,
}

impl SourceFileHandle {
    fn from_source(source_file: SourceFile) -> Self {
        Self {
            inner: Rc::new(SourceState::new(source_file)),
        }
    }

    pub(crate) fn new() -> Self {
        CURRENT_SOURCE.with(|cell| {
            cell.borrow()
                .as_ref()
                .cloned()
                .expect("SourceFileHandle::new called without a current SourceFile")
        })
    }

    pub(crate) fn locator(&self) -> Locator<'_> {
        self.assert_active();

        Locator::with_index(
            self.inner.source_file().source_text(),
            self.inner.source_file().index().clone(),
        )
    }

    pub(crate) fn invalidate(&self) {
        self.inner.invalidate();
    }

    fn assert_active(&self) {
        assert!(
            self.inner.is_active(),
            "source file is no longer valid (store was invalidated)"
        );
    }

    #[cfg(test)]
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_source_file::SourceFileBuilder;

    #[test]
    fn owned_source_uses_current_source_file() {
        let contents = "print('hi')";
        let source_file = SourceFileBuilder::new("test.py", contents).finish();

        with_source_file(&source_file, || {
            let owned = SourceFileHandle::new();
            let recovered = owned.locator();
            assert_eq!(recovered.as_str(), contents);
        });
    }

    #[test]
    fn invalidates_after_scope() {
        let contents = "print('bye')";
        let source_file = SourceFileBuilder::new("test.py", contents).finish();

        let handle = with_source_file(&source_file, || {
            let handle = SourceFileHandle::new();
            // Handle remains active inside the scope.
            assert!(handle.is_active());
            handle
        });

        // Once the scope ends, the handle should report inactive.
        assert!(!handle.is_active());
    }
}
