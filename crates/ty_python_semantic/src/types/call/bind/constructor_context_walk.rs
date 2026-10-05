//! Iterative propagation of constructor instance types and class generic contexts.
//!
//! Each callable's overloads precede its downstream constructor, and the complete downstream
//! subtree precedes the next callable. Slice iterators retain disjoint mutable borrows into the
//! caller's binding tree; no binding owner moves into the traversal stack. Calls to
//! [`ReceiverBindingEffects::normalized_return`] borrow only the selected overload's signature;
//! [`ReceiverBindingEffects::merge_generic_context`] receives copied context handles.

use std::alloc::Layout;
use std::slice;

use super::constructor::ConstructorContext;
use super::constructor_preparation::ReceiverBindingEffects;
use super::{Binding, Bindings, BindingsElement, CallableItem};
use crate::Db;
use crate::types::Type;
use crate::types::generics::GenericContext;

/// Chooses the context propagated through the complete constructor subtree.
#[derive(Clone, Copy, Debug)]
enum Update<'db> {
    Instance(Type<'db>),
    ClassContext(GenericContext<'db>),
}

/// Carries the context used to update one callable's overloads.
#[derive(Clone, Copy, Debug)]
enum OverloadUpdate<'db> {
    Instance(ConstructorContext<'db>),
    ClassContext(GenericContext<'db>),
}

/// Retains the unvisited siblings at one depth without rescanning the path from the root.
#[derive(Debug)]
enum Frame<'bindings, 'db> {
    Elements(slice::IterMut<'bindings, BindingsElement<'db>>),
    Items(slice::IterMut<'bindings, CallableItem<'db>>),
    Overloads {
        overloads: slice::IterMut<'bindings, Binding<'db>>,
        update: OverloadUpdate<'db>,
    },
}

/// Holds disjoint entry and downstream borrows while their traversal frames are admitted.
#[derive(Debug)]
struct ItemFrames<'bindings, 'db> {
    overloads: Option<Frame<'bindings, 'db>>,
    downstream: Option<Frame<'bindings, 'db>>,
}

/// Selects overload and downstream frames for the requested update.
/// Instance updates first set the constructor entry's instance context and skip regular callables.
/// Class-context updates also select regular callable overloads without changing them yet.
fn item_frames<'bindings, 'db>(
    item: &'bindings mut CallableItem<'db>,
    update: Update<'db>,
) -> ItemFrames<'bindings, 'db> {
    match item {
        CallableItem::Regular(binding) => ItemFrames {
            overloads: match update {
                Update::Instance(_) => None,
                Update::ClassContext(context) => Some(Frame::Overloads {
                    overloads: binding.overloads.iter_mut(),
                    update: OverloadUpdate::ClassContext(context),
                }),
            },
            downstream: None,
        },
        CallableItem::Constructor(constructor) => {
            let update = match update {
                Update::Instance(instance) => {
                    constructor.set_constructed_instance_type(instance);
                    OverloadUpdate::Instance(constructor.context())
                }
                Update::ClassContext(context) => OverloadUpdate::ClassContext(context),
            };
            let overloads = Frame::Overloads {
                overloads: constructor.entry.overloads.iter_mut(),
                update,
            };
            // Deferred downstream constructor bindings still need constructor instance
            // context for generic specialization inference (including literal
            // promotion).
            let downstream = constructor
                .downstream_constructor
                .as_deref_mut()
                .map(|child| Frame::Elements(child.elements.iter_mut()));
            ItemFrames {
                overloads: Some(overloads),
                downstream,
            }
        }
    }
}

/// Pushes a traversal frame after admitting growth, relocation, transfers, and stack retirement.
/// Storage grows geometrically when the current capacity is full.
/// Frames contain only borrowed iterators, so retiring them never traverses binding owners.
async fn push_frame<'bindings, 'db, E: ReceiverBindingEffects<'db>>(
    frames: &mut Vec<Frame<'bindings, 'db>>,
    frame: Frame<'bindings, 'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let (length, capacity) = effects
        .local(Some(2), Some(0), || (frames.len(), frames.capacity()))
        .await?;
    let additional = if length == capacity {
        capacity.max(4)
    } else {
        0
    };
    let (work, bytes) = if additional != 0 {
        (
            length.checked_mul(2).and_then(|work| work.checked_add(12)),
            length
                .checked_add(additional)
                .and_then(|capacity| Layout::array::<Frame<'bindings, 'db>>(capacity).ok())
                .map(|layout| layout.size())
                .and_then(|bytes| {
                    bytes.checked_add(length.checked_mul(size_of::<Frame<'bindings, 'db>>())?)
                })
                .and_then(|bytes| bytes.checked_add(size_of::<Frame<'bindings, 'db>>())),
        )
    } else {
        (Some(4), Some(size_of::<Frame<'bindings, 'db>>()))
    };
    effects
        .local(work, bytes, || {
            if additional != 0 {
                frames.reserve_exact(additional);
            }
            frames.push(frame);
        })
        .await
}

/// Applies one context update while the root continues to own the selected overload.
async fn update_overload_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    overload: &mut Binding<'db>,
    update: OverloadUpdate<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    match update {
        OverloadUpdate::Instance(context) => {
            effects
                .local(
                    Some(1),
                    Some(size_of::<Option<ConstructorContext<'db>>>()),
                    || {
                        overload.constructor_context = Some(context);
                    },
                )
                .await?;
            let return_ty = effects
                .normalized_return(
                    db,
                    &overload.signature,
                    Some((context.kind(), context.instance_type())),
                )
                .await?;
            effects
                .local(Some(1), Some(size_of::<Type<'db>>()), || {
                    overload.return_ty = return_ty;
                })
                .await?;
        }
        OverloadUpdate::ClassContext(incoming) => {
            let existing = effects
                .local(Some(1), Some(0), || overload.signature.generic_context)
                .await?;
            let context = effects
                .merge_generic_context(db, existing, incoming)
                .await?;
            effects
                .local(
                    Some(1),
                    Some(size_of::<Option<GenericContext<'db>>>()),
                    || {
                        overload.signature.generic_context = Some(context);
                    },
                )
                .await?;
        }
    }
    Ok(())
}

/// Walks the binding tree once in source order, applying the update to eligible overloads.
/// Property-error owners are not constructor descendants and do not receive these updates.
async fn walk_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    bindings: &mut Bindings<'db>,
    update: Update<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let mut frames = effects.local(Some(1), Some(0), Vec::new).await?;
    let root = effects
        .local(Some(2), Some(0), || {
            Frame::Elements(bindings.elements.iter_mut())
        })
        .await?;
    push_frame(&mut frames, root, effects).await?;
    loop {
        let Some(frame) = effects.local(Some(2), Some(0), || frames.pop()).await? else {
            return Ok(());
        };
        match frame {
            Frame::Elements(mut elements) => {
                let element = effects.local(Some(2), Some(0), || elements.next()).await?;
                if let Some(element) = element {
                    push_frame(&mut frames, Frame::Elements(elements), effects).await?;
                    let items = effects
                        .local(Some(2), Some(0), || Frame::Items(element.items.iter_mut()))
                        .await?;
                    push_frame(&mut frames, items, effects).await?;
                }
            }
            Frame::Items(mut items) => {
                let item = effects.local(Some(2), Some(0), || items.next()).await?;
                if let Some(item) = item {
                    push_frame(&mut frames, Frame::Items(items), effects).await?;
                    let selected = effects
                        .local(Some(12), Some(0), || item_frames(item, update))
                        .await?;
                    if let Some(downstream) = selected.downstream {
                        push_frame(&mut frames, downstream, effects).await?;
                    }
                    if let Some(overloads) = selected.overloads {
                        push_frame(&mut frames, overloads, effects).await?;
                    }
                }
            }
            Frame::Overloads {
                mut overloads,
                update,
            } => {
                let overload = effects.local(Some(2), Some(0), || overloads.next()).await?;
                if let Some(overload) = overload {
                    push_frame(&mut frames, Frame::Overloads { overloads, update }, effects)
                        .await?;
                    update_overload_with(db, overload, update, effects).await?;
                }
            }
        }
    }
}

/// Updates every constructor entry and downstream overload to infer the supplied instance type.
/// Regular callable items are unchanged. Failure leaves earlier mutations in the retained root.
/// The caller keeps that root alive until pending effect futures have completed or been dropped,
/// following the [`ConstructorBindingStorageEffects`](super::constructor_preparation::ConstructorBindingStorageEffects)
/// ownership contract.
pub(in crate::types) async fn set_constructor_instance_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    bindings: &mut Bindings<'db>,
    instance_type: Type<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    walk_with(db, bindings, Update::Instance(instance_type), effects).await
}

/// Merges the class context into regular and constructor overloads in depth-first source order.
/// An absent class context performs no traversal. A refused merge leaves earlier updates in the
/// caller's root. As required by [`ConstructorBindingStorageEffects`](super::constructor_preparation::ConstructorBindingStorageEffects),
/// that owner remains alive until pending effect futures have completed or been dropped.
pub(in crate::types) async fn apply_class_context_with<'db, E: ReceiverBindingEffects<'db>>(
    db: &'db dyn Db,
    bindings: &mut Bindings<'db>,
    context: Option<GenericContext<'db>>,
    effects: &E,
) -> Result<(), E::Error> {
    let context = effects.local(Some(1), Some(0), || context).await?;
    if let Some(context) = context {
        walk_with(db, bindings, Update::ClassContext(context), effects).await?;
    }
    Ok(())
}
