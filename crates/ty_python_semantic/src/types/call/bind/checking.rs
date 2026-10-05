//! Resumable ordering for checking callable unions, intersections, and constructors.

use super::{Bindings, CallErrorKind, CallableItem, CheckTypesMode};
use crate::Db;

/// The driver supplies the same bindings at each step. Their item positions remain stable until
/// the final intersection filtering; the continuation holds no borrow of the bindings.
pub(super) enum BindingsCheckStep {
    Item(PendingItemCheck),
    KnownCases(PendingKnownCases),
    Downstream(PendingDownstreamCheck),
    Complete(Result<(), CallErrorKind>),
}

impl BindingsCheckStep {
    pub(super) fn start(bindings: &Bindings<'_>, mode: CheckTypesMode) -> Self {
        Self::check_items(bindings, ItemCursor::default(), mode)
    }

    fn check_items(
        bindings: &Bindings<'_>,
        mut remaining: ItemCursor,
        mode: CheckTypesMode,
    ) -> Self {
        if let Some(position) = remaining.next(bindings) {
            return Self::Item(PendingItemCheck {
                position,
                remaining,
                mode,
            });
        }

        // Generic call inference must maintain a stable set of overloads until the final round
        // of fixpoint iteration. Provisional item checks already check their downstream
        // constructors immediately, preserving every constructor binding.
        if mode.is_provisional() {
            Self::Complete(Ok(()))
        } else {
            Self::KnownCases(PendingKnownCases {
                remaining: ItemCursor::default(),
            })
        }
    }

    fn check_downstream<'db>(
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        mut remaining: ItemCursor,
    ) -> Self {
        while let Some(position) = remaining.next(bindings) {
            if position.item(bindings).as_constructor().is_some() {
                return Self::Downstream(PendingDownstreamCheck {
                    position,
                    remaining,
                });
            }
        }

        // For intersection elements with at least one successful binding,
        // filter out the failing bindings after deferred constructor checks.
        for element in &mut bindings.elements {
            element.retain_successful(db);
        }

        Self::Complete(bindings.as_result(db))
    }
}

struct ItemPosition {
    element: usize,
    item: usize,
}

impl ItemPosition {
    fn item<'a, 'db>(&self, bindings: &'a mut Bindings<'db>) -> &'a mut CallableItem<'db> {
        &mut bindings.elements[self.element].items[self.item]
    }
}

#[derive(Default)]
struct ItemCursor {
    element: usize,
    item: usize,
}

impl ItemCursor {
    fn next(&mut self, bindings: &Bindings<'_>) -> Option<ItemPosition> {
        while let Some(element) = bindings.elements.get(self.element) {
            if self.item < element.items.len() {
                let position = ItemPosition {
                    element: self.element,
                    item: self.item,
                };
                self.item += 1;
                return Some(position);
            }
            self.element += 1;
            self.item = 0;
        }
        None
    }
}

pub(super) struct PendingItemCheck {
    position: ItemPosition,
    remaining: ItemCursor,
    mode: CheckTypesMode,
}

impl PendingItemCheck {
    pub(super) fn item<'a, 'db>(
        &self,
        bindings: &'a mut Bindings<'db>,
    ) -> &'a mut CallableItem<'db> {
        self.position.item(bindings)
    }

    pub(super) fn mode(&self) -> CheckTypesMode {
        self.mode
    }

    pub(super) fn resume(self, bindings: &Bindings<'_>) -> BindingsCheckStep {
        BindingsCheckStep::check_items(bindings, self.remaining, self.mode)
    }
}

pub(super) struct PendingKnownCases {
    remaining: ItemCursor,
}

impl PendingKnownCases {
    pub(super) fn resume<'db>(
        self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
    ) -> BindingsCheckStep {
        // Native and property behavior is finalized before checking the downstream constructors
        // retained by instance-returning overloads during the item checks.
        BindingsCheckStep::check_downstream(db, bindings, self.remaining)
    }
}

pub(super) struct PendingDownstreamCheck {
    position: ItemPosition,
    remaining: ItemCursor,
}

impl PendingDownstreamCheck {
    pub(super) fn item<'a, 'db>(
        &self,
        bindings: &'a mut Bindings<'db>,
    ) -> &'a mut CallableItem<'db> {
        self.position.item(bindings)
    }

    pub(super) fn resume<'db>(
        self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
    ) -> BindingsCheckStep {
        BindingsCheckStep::check_downstream(db, bindings, self.remaining)
    }
}
