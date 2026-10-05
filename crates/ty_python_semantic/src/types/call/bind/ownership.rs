//! Iterative ownership of complete call-binding trees.
//!
//! Constructor downstreams and property-accessor errors both own child `Bindings`. Cloning visits
//! those edges with an admitted frame vector. Destruction reuses the child boxes as a worklist,
//! so cancellation requires no new allocation. Producers prepay all retirement work; cleanup
//! itself cannot request admission after cancellation.

use std::alloc::Layout;
use std::convert::Infallible;
use std::mem;

use smallvec::{Array, SmallVec};

use super::constructor::ConstructorBinding;
use super::{
    Binding, BindingError, Bindings, BindingsElement, CallableBinding, CallableItem,
    ExpandedOverloadCall, MatchedArgument, MatchedParameter, OverloadCallResult, ParameterContext,
    ParameterContexts, PropertyAccessorCallError, SpecializationError,
};
use crate::types::signatures::Signature;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{BindingContext, Type};

/// Admissions for copying a complete binding tree, including signature-owned storage.
/// `local` additionally admits the full closure and result representations before execution.
/// Controlled providers reject a missing quote before invoking its operation; ordinary providers
/// execute immediately. Any owned value captured by the operation already has prepaid retirement.
pub(in crate::types) trait CloneBindingsEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Clones one signature after funding its storage and complete eventual retirement.
    async fn signature_clone(&self, source: &Signature<'db>)
    -> Result<Signature<'db>, Self::Error>;

    #[cfg(test)]
    fn clone_before(&self) {}

    #[cfg(test)]
    fn clone_after(&self) {}

    /// Observes a new partial `Bindings` owner after its frame is installed.
    #[cfg(test)]
    fn clone_node_created(&self) {}

    #[cfg(test)]
    fn downstream_install_before(&self) {}

    #[cfg(test)]
    fn downstream_install_after(&self) {}
}

/// Immediately ready provider for the ordinary `Clone` implementation.
#[derive(Debug)]
pub(in crate::types) struct InlineCloneBindingsEffects;

impl<'db> CloneBindingsEffects<'db> for InlineCloneBindingsEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(operation())
    }

    async fn signature_clone(&self, source: &Signature<'db>) -> Result<Signature<'db>, Infallible> {
        Ok(source.clone())
    }
}

/// Clones every semantic field without recursively cloning child bindings.
pub(super) fn clone_bindings<'db>(source: &Bindings<'db>) -> Bindings<'db> {
    legacy_inline(clone_bindings_with(source, &InlineCloneBindingsEffects))
}

/// Prepays cleanup of one pristine root, element and item with empty local binding buffers.
/// Signature owners are charged by their producer. The root allowance is 32 operations; each
/// element and item adds eight, and each overload adds 16 for both detach scans and final teardown.
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) const fn pristine_bindings_retirement_work(overloads: usize) -> Option<usize> {
    match overloads.checked_mul(16) {
        Some(overloads) => 48usize.checked_add(overloads),
        None => None,
    }
}

/// Retires all child binding nodes without allocating a cleanup worklist.
/// The private cleanup link is understood on entry as well, so unwinding can retire a pending
/// list by dropping its first box. Error vectors are consumed to take required property boxes
/// without constructing replacement errors or dummy bindings.
pub(super) fn drop_bindings(bindings: &mut Bindings<'_>) {
    #[cfg(test)]
    let observation = observations::begin(bindings);
    let mut pending = bindings.cleanup_next.take();
    detach_children(bindings, &mut pending);
    while let Some(mut node) = pending.take() {
        pending = node.cleanup_next.take();
        detach_children(&mut node, &mut pending);
        drop(node);
    }
    drop(mem::take(&mut bindings.elements));
    drop(bindings.enclosing_binding_contexts.take());
    #[cfg(test)]
    observations::complete(observation);
}

/// Transfers existing semantic child boxes into the destructor's private owning list.
fn detach_children<'db>(bindings: &mut Bindings<'db>, pending: &mut Option<Box<Bindings<'db>>>) {
    for element in &mut bindings.elements {
        for item in &mut element.items {
            if let CallableItem::Constructor(constructor) = item
                && let Some(child) = constructor.downstream_constructor.take()
            {
                push_cleanup(child, pending);
            }
            for overload in &mut item.callable_mut().overloads {
                for error in mem::take(&mut overload.errors) {
                    match error {
                        BindingError::PropertyGetterCallError(error)
                        | BindingError::PropertySetterCallError(error) => {
                            push_cleanup(error.bindings, pending);
                        }
                        BindingError::InvalidArgumentType { .. }
                        | BindingError::InvalidKeyType { .. }
                        | BindingError::MissingArguments { .. }
                        | BindingError::UnknownArgument { .. }
                        | BindingError::UnknownKeywordVariadicArgument { .. }
                        | BindingError::PositionalOnlyParameterAsKwarg { .. }
                        | BindingError::TooManyPositionalArguments { .. }
                        | BindingError::ParameterAlreadyAssigned { .. }
                        | BindingError::SpecializationError { .. }
                        | BindingError::PropertyHasNoGetter(_)
                        | BindingError::PropertyHasNoSetter(_)
                        | BindingError::PropertyHasNoDeleter(_)
                        | BindingError::InternalCallError(_)
                        | BindingError::UnmatchedOverload
                        | BindingError::CalledTopCallable(_)
                        | BindingError::InvalidDataclassApplication(_)
                        | BindingError::InvalidDataclassArgument(_) => {}
                    }
                }
            }
        }
    }
}

/// Links a detached semantic child, whose cleanup link is empty, without losing ownership.
fn push_cleanup<'db>(mut child: Box<Bindings<'db>>, pending: &mut Option<Box<Bindings<'db>>>) {
    child.cleanup_next = pending.take();
    *pending = Some(child);
}

/// Semantic location in a parent that receives the next completed child.
#[derive(Clone, Copy, Debug)]
enum ChildTarget {
    Downstream,
    Getter { argument_index_offset: usize },
    Setter { argument_index_offset: usize },
}

/// Source position to resume after a completed child is installed.
#[derive(Clone, Copy, Debug)]
enum Resume {
    Item {
        element: usize,
        item: usize,
    },
    Error {
        element: usize,
        item: usize,
        overload: usize,
        error: usize,
    },
}

impl Resume {
    const fn cursor(self) -> CloneCursor {
        match self {
            Self::Item { element, item } => CloneCursor::Item { element, item },
            Self::Error {
                element,
                item,
                overload,
                error,
            } => CloneCursor::Error {
                element,
                item,
                overload,
                error,
            },
        }
    }
}

/// Position for one bounded traversal step within a borrowed source node.
#[derive(Clone, Copy, Debug)]
enum CloneCursor {
    Element {
        element: usize,
    },
    Item {
        element: usize,
        item: usize,
    },
    Overload {
        element: usize,
        item: usize,
        overload: usize,
    },
    Error {
        element: usize,
        item: usize,
        overload: usize,
        error: usize,
    },
    Downstream {
        element: usize,
        item: usize,
    },
    Waiting {
        target: ChildTarget,
        resume: Resume,
    },
}

/// Keeps a borrowed source and a plain, safely droppable partial output across child suspension.
#[derive(Debug)]
struct CloneFrame<'source, 'db> {
    source: &'source Bindings<'db>,
    output: Bindings<'db>,
    cursor: CloneCursor,
}

impl<'source, 'db> CloneFrame<'source, 'db> {
    /// Starts a node without semantic children; enclosing contexts are the only owned root leaf.
    fn new(source: &'source Bindings<'db>) -> Self {
        Self {
            source,
            output: Bindings {
                callable_type: source.callable_type,
                implicit_dunder_new_is_possibly_unbound: source
                    .implicit_dunder_new_is_possibly_unbound,
                implicit_dunder_init_is_possibly_unbound: source
                    .implicit_dunder_init_is_possibly_unbound,
                elements: SmallVec::new(),
                enclosing_binding_contexts: source.enclosing_binding_contexts.clone(),
                cleanup_next: None,
            },
            cursor: CloneCursor::Element { element: 0 },
        }
    }

    fn current_callable_mut(&mut self) -> &mut CallableBinding<'db> {
        let element = require_clone_state(self.output.elements.last_mut());
        require_clone_state(element.items.last_mut()).callable_mut()
    }

    fn current_overload_mut(&mut self) -> &mut Binding<'db> {
        require_clone_state(self.current_callable_mut().overloads.last_mut())
    }

    /// Advances at most one element, item, overload or error boundary per admitted step.
    fn next(&mut self) -> CloneStep<'source, 'db> {
        let source = self.source;
        match self.cursor {
            CloneCursor::Element { element } => {
                if let Some(source) = source.elements.get(element) {
                    self.cursor = CloneCursor::Item { element, item: 0 };
                    CloneStep::Element(source)
                } else {
                    CloneStep::Complete
                }
            }
            CloneCursor::Item { element, item } => {
                if let Some(source) = source
                    .elements
                    .get(element)
                    .and_then(|element| element.items.get(item))
                {
                    self.cursor = CloneCursor::Overload {
                        element,
                        item,
                        overload: 0,
                    };
                    CloneStep::Item(source)
                } else {
                    self.cursor = CloneCursor::Element {
                        element: element + 1,
                    };
                    CloneStep::Advance
                }
            }
            CloneCursor::Overload {
                element,
                item,
                overload,
            } => {
                if let Some(source) = source
                    .elements
                    .get(element)
                    .and_then(|element| element.items.get(item))
                    .and_then(|item| item.callable().overloads.get(overload))
                {
                    self.cursor = CloneCursor::Error {
                        element,
                        item,
                        overload,
                        error: 0,
                    };
                    CloneStep::Overload(source)
                } else {
                    self.cursor = CloneCursor::Downstream { element, item };
                    CloneStep::Advance
                }
            }
            CloneCursor::Error {
                element,
                item,
                overload,
                error,
            } => {
                if let Some(source) = source
                    .elements
                    .get(element)
                    .and_then(|element| element.items.get(item))
                    .and_then(|item| item.callable().overloads.get(overload))
                    .and_then(|overload| overload.errors.get(error))
                {
                    let resume = Resume::Error {
                        element,
                        item,
                        overload,
                        error: error + 1,
                    };
                    match source {
                        BindingError::PropertyGetterCallError(property) => {
                            self.cursor = CloneCursor::Waiting {
                                target: ChildTarget::Getter {
                                    argument_index_offset: property.argument_index_offset,
                                },
                                resume,
                            };
                            CloneStep::Child(&property.bindings)
                        }
                        BindingError::PropertySetterCallError(property) => {
                            self.cursor = CloneCursor::Waiting {
                                target: ChildTarget::Setter {
                                    argument_index_offset: property.argument_index_offset,
                                },
                                resume,
                            };
                            CloneStep::Child(&property.bindings)
                        }
                        BindingError::InvalidArgumentType { .. }
                        | BindingError::InvalidKeyType { .. }
                        | BindingError::MissingArguments { .. }
                        | BindingError::UnknownArgument { .. }
                        | BindingError::UnknownKeywordVariadicArgument { .. }
                        | BindingError::PositionalOnlyParameterAsKwarg { .. }
                        | BindingError::TooManyPositionalArguments { .. }
                        | BindingError::ParameterAlreadyAssigned { .. }
                        | BindingError::SpecializationError { .. }
                        | BindingError::PropertyHasNoGetter(_)
                        | BindingError::PropertyHasNoSetter(_)
                        | BindingError::PropertyHasNoDeleter(_)
                        | BindingError::InternalCallError(_)
                        | BindingError::UnmatchedOverload
                        | BindingError::CalledTopCallable(_)
                        | BindingError::InvalidDataclassApplication(_)
                        | BindingError::InvalidDataclassArgument(_) => {
                            self.cursor = resume.cursor();
                            CloneStep::Error(source)
                        }
                    }
                } else {
                    self.cursor = CloneCursor::Overload {
                        element,
                        item,
                        overload: overload + 1,
                    };
                    CloneStep::Advance
                }
            }
            CloneCursor::Downstream { element, item } => {
                let child = source
                    .elements
                    .get(element)
                    .and_then(|element| element.items.get(item))
                    .and_then(CallableItem::as_constructor)
                    .and_then(|constructor| constructor.downstream_constructor.as_deref());
                let resume = Resume::Item {
                    element,
                    item: item + 1,
                };
                if let Some(child) = child {
                    self.cursor = CloneCursor::Waiting {
                        target: ChildTarget::Downstream,
                        resume,
                    };
                    CloneStep::Child(child)
                } else {
                    self.cursor = resume.cursor();
                    CloneStep::Advance
                }
            }
            CloneCursor::Waiting { .. } => require_clone_state(None),
        }
    }
}

/// Owns every partial node and any completed child until attachment or final transfer succeeds.
#[derive(Debug)]
struct CloneState<'source, 'db> {
    frames: Vec<CloneFrame<'source, 'db>>,
    completed: Option<Bindings<'db>>,
}

/// Next operation selected by the cursor, before its copying or allocation is admitted.
#[derive(Clone, Copy, Debug)]
enum CloneStep<'source, 'db> {
    Advance,
    Element(&'source BindingsElement<'db>),
    Item(&'source CallableItem<'db>),
    Overload(&'source Binding<'db>),
    Error(&'source BindingError<'db>),
    Child(&'source Bindings<'db>),
    Complete,
    Install,
    Transfer,
}

/// Clones every binding node and leaf through one iterative traversal for both providers.
/// A frame owns its partial output until a completed child can be installed. Each cursor step
/// advances only after the preceding append succeeded; a refused operation exits while the
/// frame vector and completed-child slot still own all partial results. No source-depth limit
/// is assumed, including across property-call errors.
pub(in crate::types) async fn clone_bindings_with<'source, 'db, E: CloneBindingsEffects<'db>>(
    source: &'source Bindings<'db>,
    effects: &E,
) -> Result<Bindings<'db>, E::Error> {
    let mut state = effects
        .local(Some(3), Some(0), || CloneState {
            frames: Vec::new(),
            completed: None,
        })
        .await?;
    #[cfg(test)]
    effects.clone_before();
    effects
        .local(Some(5), Some(size_of::<CloneFrame<'source, 'db>>()), || {
            state.frames.reserve_exact(1);
            #[cfg(test)]
            effects.clone_after();
        })
        .await?;
    push_frame(&mut state, source, effects).await?;
    loop {
        let step = effects
            .local(Some(12), Some(0), || {
                if state.completed.is_some() {
                    if state.frames.is_empty() {
                        CloneStep::Transfer
                    } else {
                        CloneStep::Install
                    }
                } else {
                    require_clone_state(state.frames.last_mut()).next()
                }
            })
            .await?;
        match step {
            CloneStep::Advance => {}
            CloneStep::Child(source) => push_frame(&mut state, source, effects).await?,
            CloneStep::Element(source) => {
                let count = source.items.len();
                effects
                    .local(
                        Some(16),
                        smallvec_bytes::<CallableItem<'db>>(count)
                            .and_then(|bytes| bytes.checked_add(size_of::<BindingsElement<'db>>())),
                        || {
                            let mut items = SmallVec::new();
                            items.reserve_exact(count);
                            let frame = require_clone_state(state.frames.last_mut());
                            frame.output.elements.push(BindingsElement {
                                callable_type: source.callable_type,
                                items,
                            });
                        },
                    )
                    .await?;
            }
            CloneStep::Item(source) => {
                let quote = effects
                    .local(Some(32), Some(0), || callable_leaf_quote(source.callable()))
                    .await?;
                let count = source.callable().overloads.len();
                let work = quote
                    .and_then(LeafQuote::admission_work)
                    .and_then(|n| n.checked_add(16));
                let bytes = quote.and_then(|quote| {
                    smallvec_bytes::<Binding<'db>>(count)
                        .and_then(|size| quote.clone_bytes.checked_add(size))
                        .and_then(|bytes| bytes.checked_add(size_of::<CallableItem<'db>>()))
                });
                effects
                    .local(work, bytes, || {
                        let mut callable = clone_callable_without_overloads(source.callable());
                        callable.overloads.reserve_exact(count);
                        let item = match source {
                            CallableItem::Regular(_) => CallableItem::Regular(callable),
                            CallableItem::Constructor(constructor) => {
                                CallableItem::Constructor(ConstructorBinding {
                                    entry: callable,
                                    constructor_context: constructor.constructor_context,
                                    downstream_constructor: None,
                                })
                            }
                        };
                        let frame = require_clone_state(state.frames.last_mut());
                        require_clone_state(frame.output.elements.last_mut())
                            .items
                            .push(item);
                    })
                    .await?;
            }
            CloneStep::Overload(source) => {
                let quote = effects
                    .local(
                        source
                            .argument_matches
                            .len()
                            .checked_mul(32)
                            .and_then(|work| work.checked_add(64)),
                        Some(0),
                        || binding_leaf_quote(source),
                    )
                    .await?;
                let signature = effects.signature_clone(&source.signature).await?;
                let count = source.errors.len();
                let work = quote
                    .and_then(LeafQuote::admission_work)
                    .and_then(|n| n.checked_add(25));
                let bytes = quote.and_then(|quote| {
                    array_bytes::<BindingError<'db>>(count)
                        .and_then(|size| quote.clone_bytes.checked_add(size))
                });
                effects
                    .local(work, bytes, || {
                        let mut binding = clone_binding_without_errors(source, signature);
                        binding.errors.reserve_exact(count);
                        let frame = require_clone_state(state.frames.last_mut());
                        frame.current_callable_mut().overloads.push(binding);
                    })
                    .await?;
            }
            CloneStep::Error(source) => {
                let quote = effects
                    .local(Some(32), Some(0), || binding_error_leaf_quote(source))
                    .await?
                    .flatten();
                effects
                    .local(
                        quote
                            .and_then(LeafQuote::admission_work)
                            .and_then(|n| n.checked_add(4)),
                        quote.map(|quote| quote.clone_bytes),
                        || {
                            let error = require_clone_state(clone_binding_error_leaf(source));
                            let frame = require_clone_state(state.frames.last_mut());
                            frame.current_overload_mut().errors.push(error);
                        },
                    )
                    .await?;
            }
            CloneStep::Complete => {
                effects
                    .local(
                        Some(5),
                        size_of::<CloneFrame<'source, 'db>>()
                            .checked_add(size_of::<Option<Bindings<'db>>>()),
                        || {
                            let frame = require_clone_state(state.frames.pop());
                            state.completed = Some(frame.output);
                        },
                    )
                    .await?;
            }
            CloneStep::Install => {
                effects
                    .local(
                        Some(24),
                        size_of::<Bindings<'db>>()
                            .checked_mul(2)
                            .and_then(|bytes| {
                                bytes.checked_add(size_of::<PropertyAccessorCallError<'db>>())
                            })
                            .and_then(|bytes| bytes.checked_add(size_of::<BindingError<'db>>()))
                            .and_then(|bytes| {
                                bytes.checked_add(size_of::<Option<Box<Bindings<'db>>>>())
                            }),
                        || {
                            let parent = require_clone_state(state.frames.last_mut());
                            let (target, resume) = match parent.cursor {
                                CloneCursor::Waiting { target, resume } => (target, resume),
                                CloneCursor::Element { .. }
                                | CloneCursor::Item { .. }
                                | CloneCursor::Overload { .. }
                                | CloneCursor::Error { .. }
                                | CloneCursor::Downstream { .. } => require_clone_state(None),
                            };
                            let child = require_clone_state(state.completed.take());
                            match target {
                                ChildTarget::Downstream => {
                                    let element =
                                        require_clone_state(parent.output.elements.last_mut());
                                    let item = require_clone_state(element.items.last_mut());
                                    let constructor =
                                        require_clone_state(item.as_constructor_mut());
                                    constructor.downstream_constructor = Some(Box::new(child));
                                }
                                ChildTarget::Getter {
                                    argument_index_offset,
                                } => {
                                    parent.current_overload_mut().errors.push(
                                        BindingError::PropertyGetterCallError(
                                            PropertyAccessorCallError {
                                                bindings: Box::new(child),
                                                argument_index_offset,
                                            },
                                        ),
                                    );
                                }
                                ChildTarget::Setter {
                                    argument_index_offset,
                                } => {
                                    parent.current_overload_mut().errors.push(
                                        BindingError::PropertySetterCallError(
                                            PropertyAccessorCallError {
                                                bindings: Box::new(child),
                                                argument_index_offset,
                                            },
                                        ),
                                    );
                                }
                            }
                            parent.cursor = resume.cursor();
                        },
                    )
                    .await?;
            }
            CloneStep::Transfer => {
                effects
                    .local(Some(3), Some(size_of::<Bindings<'db>>()), || ())
                    .await?;
                return Ok(require_clone_state(state.completed.take()));
            }
        }
    }
}

/// Checks internal clone state established by the preceding successful append or cursor step.
/// The source indices cannot prove this relationship to Rust while a frame moves across awaits.
/// A mismatch is an implementation bug; it must neither discard an output field nor spin forever.
fn require_clone_state<T>(value: Option<T>) -> T {
    value.expect("the clone cursor must address its successfully constructed output")
}

/// Creates and installs a partial binding frame, admitting any required frame-vector growth first.
async fn push_frame<'source, 'db, E: CloneBindingsEffects<'db>>(
    state: &mut CloneState<'source, 'db>,
    source: &'source Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let (len, capacity) = effects
        .local(Some(2), Some(0), || {
            (state.frames.len(), state.frames.capacity())
        })
        .await?;
    if len == capacity {
        let target = capacity
            .checked_mul(2)
            .and_then(|capacity| len.checked_add(1).map(|needed| capacity.max(needed)));
        let bytes = target
            .and_then(|target| target.checked_add(capacity))
            .and_then(array_bytes::<CloneFrame<'source, 'db>>);
        effects
            .local(len.checked_add(4), bytes, || {
                let target = require_clone_state(target);
                state.frames.reserve_exact(target - len);
            })
            .await?;
    }
    let contexts = source
        .enclosing_binding_contexts
        .as_deref()
        .map(<[_]>::len)
        .unwrap_or(0);
    let elements = source.elements.len();
    let work = contexts.checked_mul(2).and_then(|n| n.checked_add(43));
    let bytes = array_bytes::<BindingContext<'db>>(contexts)
        .and_then(|bytes| bytes.checked_mul(2))
        .and_then(|contexts| {
            smallvec_bytes::<BindingsElement<'db>>(elements)
                .and_then(|elements| contexts.checked_add(elements))
        })
        .and_then(|leaves| leaves.checked_add(size_of::<CloneFrame<'source, 'db>>()));
    effects
        .local(work, bytes, || {
            let mut frame = CloneFrame::new(source);
            frame.output.elements.reserve_exact(elements);
            state.frames.push(frame);
            #[cfg(test)]
            effects.clone_node_created();
        })
        .await
}

/// Requested backing for an exact-reserved `SmallVec` whose inline capacity is one.
fn smallvec_bytes<T>(len: usize) -> Option<usize> {
    if len > 1 {
        array_bytes::<T>(len)
    } else {
        Some(0)
    }
}

/// Quotes cloning a nonrecursive value and prepays cleanup of that clone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LeafQuote {
    clone_work: usize,
    clone_bytes: usize,
    retirement_work: usize,
}

impl LeafQuote {
    const fn fixed<T>(clone_work: usize, retirement_work: usize) -> Self {
        Self {
            clone_work,
            clone_bytes: size_of::<T>(),
            retirement_work,
        }
    }

    fn admission_work(self) -> Option<usize> {
        self.clone_work.checked_add(self.retirement_work)
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            clone_work: self.clone_work.checked_add(other.clone_work)?,
            clone_bytes: self.clone_bytes.checked_add(other.clone_bytes)?,
            retirement_work: self.retirement_work.checked_add(other.retirement_work)?,
        })
    }
}

/// Returns requested array bytes after validating the allocator layout.
fn array_bytes<T>(len: usize) -> Option<usize> {
    Layout::array::<T>(len).ok()?;
    len.checked_mul(size_of::<T>())
}

/// Quotes the backing requested by SmallVec 1.15.2's unspecialized Clone implementation.
fn smallvec_clone_backing_bytes<A: Array>(value: &SmallVec<A>) -> Option<usize> {
    if value.len() <= value.inline_size() {
        Some(0)
    } else {
        array_bytes::<A::Item>(value.len().checked_next_power_of_two()?)
    }
}

/// Quotes one argument match, including clone backing and cleanup of its matched parameters.
fn matched_argument_leaf_quote(value: &MatchedArgument<'_>) -> Option<LeafQuote> {
    let len = value.parameters.len();
    let spills = usize::from(len > value.parameters.inline_size());
    Some(LeafQuote {
        // Dispatch, bool copy, SmallVec construction, and iteration plus four fields per item.
        clone_work: 3usize.checked_add(len.checked_mul(5)?)?,
        clone_bytes: size_of::<MatchedArgument<'_>>()
            .checked_add(smallvec_clone_backing_bytes(&value.parameters)?)?
            .checked_add(array_bytes::<MatchedParameter<'_>>(len)?)?,
        retirement_work: 2usize.checked_add(len)?.checked_add(spills)?,
    })
}

/// Quotes the shallow binding's local leaves; the completed signature is supplied separately.
fn binding_leaf_quote(value: &Binding<'_>) -> Option<LeafQuote> {
    let arguments = value.argument_matches.len();
    let parameters = value.parameter_tys.len();
    let mut quote = LeafQuote {
        // Fifteen fields and construction; signature cloning is a separate admitted child.
        clone_work: 16usize.checked_add(arguments)?.checked_add(parameters)?,
        clone_bytes: size_of::<Binding<'_>>()
            .checked_add(array_bytes::<MatchedArgument<'_>>(arguments)?)?
            .checked_add(array_bytes::<Option<Type<'_>>>(parameters)?.checked_mul(2)?)?,
        // Local fields, two slice traversals, and their nonempty backing releases.
        retirement_work: 16usize
            .checked_add(arguments)?
            .checked_add(parameters)?
            .checked_add(usize::from(arguments != 0))?
            .checked_add(usize::from(parameters != 0))?,
    };
    for argument in value.argument_matches.iter() {
        quote = quote.checked_add(matched_argument_leaf_quote(argument)?)?;
    }
    Some(quote)
}

/// Clones a binding's local fields with an already admitted signature and an empty error buffer.
fn clone_binding_without_errors<'db>(
    value: &Binding<'db>,
    signature: Signature<'db>,
) -> Binding<'db> {
    Binding {
        signature,
        source_overload_index: value.source_overload_index,
        source_parameter_index_offset: value.source_parameter_index_offset,
        callable_type: value.callable_type,
        signature_type: value.signature_type,
        return_ty: value.return_ty,
        return_origin: value.return_origin,
        constructor_context: value.constructor_context,
        inferable_typevars: value.inferable_typevars,
        inference: value.inference,
        is_partial_application: value.is_partial_application,
        argument_matches: value.argument_matches.clone(),
        variadic_argument_matched_to_variadic_parameter: value
            .variadic_argument_matched_to_variadic_parameter,
        parameter_tys: value.parameter_tys.clone(),
        errors: Vec::new(),
    }
}

/// Quotes the optional nonrecursive result, including an expansion box and selected-index buffer.
fn overload_result_leaf_quote(value: &Option<OverloadCallResult<'_>>) -> Option<LeafQuote> {
    let base = LeafQuote::fixed::<Option<OverloadCallResult<'_>>>(2, 1);
    match value {
        None | Some(OverloadCallResult::Ambiguous) => Some(base),
        Some(OverloadCallResult::ArgumentTypeExpansionLimitReached(_)) => {
            base.checked_add(LeafQuote::fixed::<usize>(1, 1))
        }
        Some(OverloadCallResult::ArgumentTypeExpansion(expansion)) => {
            let len = expansion.selected_overloads.len();
            base.checked_add(LeafQuote {
                clone_work: 4usize.checked_add(len)?,
                clone_bytes: size_of::<ExpandedOverloadCall<'_>>()
                    .checked_mul(2)?
                    .checked_add(smallvec_clone_backing_bytes(&expansion.selected_overloads)?)?
                    .checked_add(array_bytes::<usize>(len)?)?,
                retirement_work: 3usize.checked_add(len)?.checked_add(usize::from(
                    len > expansion.selected_overloads.inline_size(),
                ))?,
            })
        }
    }
}

/// Quotes a callable skeleton's local fields and overload result, excluding overload children.
fn callable_leaf_quote(value: &CallableBinding<'_>) -> Option<LeafQuote> {
    LeafQuote::fixed::<CallableBinding<'_>>(9, 8)
        .checked_add(overload_result_leaf_quote(&value.overload_call_result)?)
}

/// Clones the callable's nonrecursive metadata and leaves overload reconstruction to its owner.
fn clone_callable_without_overloads<'db>(value: &CallableBinding<'db>) -> CallableBinding<'db> {
    CallableBinding {
        callable_type: value.callable_type,
        signature_type: value.signature_type,
        dunder_call_is_possibly_unbound: value.dunder_call_is_possibly_unbound,
        bound_type: value.bound_type,
        descriptor_origin: value.descriptor_origin,
        overload_call_result: value.overload_call_result.clone(),
        matching_overload_before_type_checking: value.matching_overload_before_type_checking,
        overloads: SmallVec::new(),
    }
}

/// Quotes a diagnostic parameter context; its Name clone shares any existing text allocation.
fn parameter_context_leaf_quote() -> LeafQuote {
    LeafQuote::fixed::<ParameterContext>(7, 6)
}

/// Quotes the cloned contexts Vec and every context, without inspecting name text.
fn parameter_contexts_leaf_quote(value: &ParameterContexts) -> Option<LeafQuote> {
    let len = value.0.len();
    let item = parameter_context_leaf_quote();
    Some(LeafQuote {
        clone_work: 2usize
            .checked_add(value.0.len().checked_mul(item.clone_work.checked_add(1)?)?)?,
        clone_bytes: size_of::<ParameterContexts>()
            .checked_add(array_bytes::<ParameterContext>(len)?)?
            .checked_add(value.0.len().checked_mul(item.clone_bytes)?)?,
        retirement_work: (1usize + usize::from(len != 0)).checked_add(
            value
                .0
                .len()
                .checked_mul(item.retirement_work.checked_add(1)?)?,
        )?,
    })
}

/// Quotes a specialization error, whose current variants contain only Copy type handles.
fn specialization_error_leaf_quote(value: &SpecializationError<'_>) -> LeafQuote {
    match value {
        SpecializationError::MismatchedBound {
            bound_typevar: _,
            argument: _,
        }
        | SpecializationError::MismatchedConstraint {
            bound_typevar: _,
            argument: _,
        } => LeafQuote::fixed::<SpecializationError<'_>>(3, 1),
    }
}

/// Quotes a nonrecursive error: `Some(Some(quote))` identifies a leaf, `Some(None)` a property-call
/// child for structural traversal, and `None` an overflowing quotation.
fn binding_error_leaf_quote(value: &BindingError<'_>) -> Option<Option<LeafQuote>> {
    let base = LeafQuote::fixed::<BindingError<'_>>(1, 1);
    let payload = match value {
        BindingError::InvalidArgumentType { .. } => {
            LeafQuote::fixed::<()>(6, 1).checked_add(parameter_context_leaf_quote())
        }
        BindingError::InvalidKeyType { .. } => Some(LeafQuote::fixed::<()>(2, 1)),
        BindingError::MissingArguments { parameters, .. } => {
            parameter_contexts_leaf_quote(parameters)
                .and_then(|quote| quote.checked_add(LeafQuote::fixed::<()>(1, 1)))
        }
        BindingError::UnknownArgument { .. } => {
            Some(LeafQuote::fixed::<ruff_python_ast::name::Name>(4, 4))
        }
        BindingError::UnknownKeywordVariadicArgument { .. } => Some(LeafQuote::fixed::<()>(1, 1)),
        BindingError::PositionalOnlyParameterAsKwarg { .. }
        | BindingError::ParameterAlreadyAssigned { .. } => {
            parameter_context_leaf_quote().checked_add(LeafQuote::fixed::<()>(1, 1))
        }
        BindingError::TooManyPositionalArguments { .. } => Some(LeafQuote::fixed::<()>(3, 1)),
        BindingError::SpecializationError { error, .. } => {
            specialization_error_leaf_quote(error).checked_add(LeafQuote::fixed::<()>(1, 1))
        }
        BindingError::PropertyHasNoGetter(_)
        | BindingError::PropertyHasNoSetter(_)
        | BindingError::PropertyHasNoDeleter(_)
        | BindingError::InternalCallError(_)
        | BindingError::CalledTopCallable(_)
        | BindingError::InvalidDataclassApplication(_)
        | BindingError::InvalidDataclassArgument(_) => Some(LeafQuote::fixed::<()>(1, 1)),
        BindingError::UnmatchedOverload => Some(LeafQuote::fixed::<()>(1, 1)),
        BindingError::PropertyGetterCallError(_) | BindingError::PropertySetterCallError(_) => {
            return Some(None);
        }
    };
    payload
        .and_then(|payload| base.checked_add(payload))
        .map(Some)
}

/// Clones only the error variants whose owned data cannot contain another Bindings tree.
fn clone_binding_error_leaf<'db>(value: &BindingError<'db>) -> Option<BindingError<'db>> {
    match value {
        BindingError::InvalidArgumentType { .. }
        | BindingError::InvalidKeyType { .. }
        | BindingError::MissingArguments { .. }
        | BindingError::UnknownArgument { .. }
        | BindingError::UnknownKeywordVariadicArgument { .. }
        | BindingError::PositionalOnlyParameterAsKwarg { .. }
        | BindingError::TooManyPositionalArguments { .. }
        | BindingError::ParameterAlreadyAssigned { .. }
        | BindingError::SpecializationError { .. }
        | BindingError::PropertyHasNoGetter(_)
        | BindingError::PropertyHasNoSetter(_)
        | BindingError::PropertyHasNoDeleter(_)
        | BindingError::InternalCallError(_)
        | BindingError::UnmatchedOverload
        | BindingError::CalledTopCallable(_)
        | BindingError::InvalidDataclassApplication(_)
        | BindingError::InvalidDataclassArgument(_) => Some(value.clone()),
        BindingError::PropertyGetterCallError(_) | BindingError::PropertySetterCallError(_) => None,
    }
}

/// Passive test observations of real owner retirement, without retaining an owner or guard.
#[cfg(test)]
pub(in crate::types) mod observations {
    use std::cell::Cell;

    use super::{Bindings, CallableItem, ConstructorBinding, Type};
    use crate::types::call::bind::constructor::ConstructorCallableKind;

    /// Distinguishes destructor entry from release of all semantic children and local backing.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum RetirementPhase {
        Begin,
        Complete,
    }

    /// Identifies one destructor call using its scalar semantic header and a thread-local id.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::types) struct RetirementEvent<'db> {
        pub(in crate::types) id: usize,
        pub(in crate::types) phase: RetirementPhase,
        pub(in crate::types) callable_type: Type<'db>,
        pub(in crate::types) constructor_kind: Option<ConstructorCallableKind>,
        pub(in crate::types) constructed_instance: Option<Type<'db>>,
    }

    pub(in crate::types) type RetirementObserver = for<'db> fn(RetirementEvent<'db>);

    thread_local! {
        static OBSERVER: Cell<Option<RetirementObserver>> = const { Cell::new(None) };
        static NEXT_ID: Cell<usize> = const { Cell::new(0) };
    }

    /// Installs a passive observer for this thread and returns the previous observer for restoration.
    pub(in crate::types) fn set_retirement_observer(
        observer: Option<RetirementObserver>,
    ) -> Option<RetirementObserver> {
        OBSERVER.replace(observer)
    }

    /// Captures a scalar header before destructive traversal changes the first callable item.
    pub(super) fn begin<'db>(bindings: &Bindings<'db>) -> RetirementEvent<'db> {
        let constructor = bindings
            .elements
            .first()
            .and_then(|element| element.items.first())
            .and_then(CallableItem::as_constructor);
        let event = RetirementEvent {
            id: NEXT_ID.get(),
            phase: RetirementPhase::Begin,
            callable_type: bindings.callable_type,
            constructor_kind: constructor.map(|constructor| constructor.context().kind()),
            constructed_instance: constructor.map(ConstructorBinding::constructed_instance_type),
        };
        NEXT_ID.set(event.id.wrapping_add(1));
        emit(event);
        event
    }

    /// Emits completion only after children, signatures and all local collection backing are released.
    pub(super) fn complete(mut event: RetirementEvent<'_>) {
        event.phase = RetirementPhase::Complete;
        emit(event);
    }

    fn emit(event: RetirementEvent<'_>) {
        if let Some(observer) = OBSERVER.get() {
            observer(event);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Checks complete binding-tree ownership without recursively comparing its child edges.

    use std::cell::Cell;
    #[cfg(feature = "experimental-analysis")]
    use std::cell::RefCell;

    use ruff_python_ast::name::Name;
    use smallvec::{SmallVec, smallvec};

    #[cfg(feature = "experimental-analysis")]
    use self::controlled::{BudgetCloneEffects, run_controlled_clone};
    #[cfg(feature = "experimental-analysis")]
    use super::CloneBindingsEffects;
    use super::observations::{
        RetirementEvent, RetirementObserver, RetirementPhase, set_retirement_observer,
    };
    use super::{InlineCloneBindingsEffects, clone_bindings_with};
    use crate::db::tests::{TestDb, setup_db};
    use crate::types::call::bind::constructor::{
        ConstructorBinding, ConstructorCallableKind, ConstructorContext,
    };
    use crate::types::call::bind::{
        Binding, BindingError, Bindings, BindingsElement, CallableBinding, CallableItem,
        ExpandedOverloadCall, InvalidArgumentTypeProvenance, InvalidDataclassArgument,
        InvalidDataclassTarget, MatchedArgument, MatchedParameter, OverloadCallResult,
        ParameterContext, ParameterContexts, PropertyAccessorCallError,
    };
    use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
    use crate::types::generics::{SpecializationError, TypeVarInference};
    use crate::types::signatures::effects::legacy_inline;
    use crate::types::signatures::{Parameter, Parameters, Signature};
    use crate::types::typevar::{
        TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce, TypeVarSet,
    };
    use crate::types::{
        BindingContext, BoundTypeVarInstance, DescriptorOrigin, PropertyInstanceType, Type,
    };

    /// Selects the ordinary Clone implementation or its controlled shared traversal.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum CloneRoute {
        Ordinary,
        #[cfg(feature = "experimental-analysis")]
        Controlled,
    }

    /// Clones through the requested public entry into the shared ownership traversal.
    fn clone_tree<'db>(source: &Bindings<'db>, route: CloneRoute) -> anyhow::Result<Bindings<'db>> {
        match route {
            CloneRoute::Ordinary => Ok(source.clone()),
            #[cfg(feature = "experimental-analysis")]
            CloneRoute::Controlled => {
                run_controlled_clone(source, &BudgetCloneEffects::unlimited()).map_err(|error| {
                    anyhow::anyhow!("unexpected unlimited clone refusal: {error:?}")
                })
            }
        }
    }

    /// Captures one argument match, including metadata that MatchedParameter does not compare.
    #[derive(Debug, Eq, PartialEq)]
    struct FlatMatchedArgument<'db> {
        matched: bool,
        parameters: Vec<FlatMatchedParameter<'db>>,
    }

    /// Records the scalar fields of one matched parameter.
    #[derive(Debug, Eq, PartialEq)]
    struct FlatMatchedParameter<'db> {
        index: usize,
        argument_type: Option<Type<'db>>,
        expected_type: Option<Type<'db>>,
        provenance: InvalidArgumentTypeProvenance,
    }

    /// Replaces property-owned bindings with their index in the flat node sequence.
    #[derive(Debug, Eq, PartialEq)]
    enum FlatError<'a, 'db> {
        Leaf(&'a BindingError<'db>),
        Getter {
            child: usize,
            argument_index_offset: usize,
        },
        Setter {
            child: usize,
            argument_index_offset: usize,
        },
    }

    /// Borrows all overload fields while keeping property-call children outside this value.
    #[derive(Debug, Eq, PartialEq)]
    struct FlatBinding<'a, 'db> {
        signature: &'a Signature<'db>,
        source_overload_index: usize,
        source_parameter_index_offset: usize,
        callable_type: Type<'db>,
        signature_type: Type<'db>,
        return_ty: Type<'db>,
        return_origin: DescriptorOrigin<'db>,
        constructor_context: Option<ConstructorContext<'db>>,
        inferable_typevars: TypeVarSet<'db>,
        inference: Option<TypeVarInference<'db>>,
        is_partial_application: bool,
        argument_matches: Vec<FlatMatchedArgument<'db>>,
        variadic_argument_matched_to_variadic_parameter: bool,
        parameter_tys: &'a [Option<Type<'db>>],
        errors: Vec<FlatError<'a, 'db>>,
    }

    /// Borrows the selected overload indices without relying on OverloadCallResult equality.
    #[derive(Debug, Eq, PartialEq)]
    enum FlatOverloadResult<'a, 'db> {
        Expanded {
            return_type: Type<'db>,
            selected_overloads: &'a [usize],
        },
        LimitReached(usize),
        Ambiguous,
    }

    /// Preserves the distinction between regular and constructor callables and their child edge.
    #[derive(Debug, Eq, PartialEq)]
    enum FlatItemKind<'db> {
        Regular,
        Constructor {
            context: ConstructorContext<'db>,
            downstream: Option<usize>,
        },
    }

    /// Captures every callable field and its local overload sequence.
    #[derive(Debug, Eq, PartialEq)]
    struct FlatCallable<'a, 'db> {
        kind: FlatItemKind<'db>,
        callable_type: Type<'db>,
        signature_type: Type<'db>,
        dunder_call_is_possibly_unbound: bool,
        bound_type: Option<Type<'db>>,
        descriptor_origin: DescriptorOrigin<'db>,
        overload_call_result: Option<FlatOverloadResult<'a, 'db>>,
        matching_overload_before_type_checking: Option<usize>,
        overloads: Vec<FlatBinding<'a, 'db>>,
    }

    /// Captures one union element and the ordering of its callable items.
    #[derive(Debug, Eq, PartialEq)]
    struct FlatElement<'a, 'db> {
        callable_type: Type<'db>,
        items: Vec<FlatCallable<'a, 'db>>,
    }

    /// Captures one binding node; all owning child edges are flat integer indices.
    #[derive(Debug, Eq, PartialEq)]
    struct FlatNode<'a, 'db> {
        callable_type: Type<'db>,
        implicit_dunder_new_is_possibly_unbound: bool,
        implicit_dunder_init_is_possibly_unbound: bool,
        enclosing_binding_contexts: Option<&'a [BindingContext<'db>]>,
        elements: Vec<FlatElement<'a, 'db>>,
        cleanup_link_is_empty: bool,
    }

    /// Appends a borrowed child to the breadth-first sequence and returns its stable index.
    fn enqueue<'a, 'db>(pending: &mut Vec<&'a Bindings<'db>>, child: &'a Bindings<'db>) -> usize {
        let index = pending.len();
        pending.push(child);
        index
    }

    /// Captures an error without invoking Clone, Debug, or PartialEq on a property-owned subtree.
    fn flat_error<'a, 'db>(
        error: &'a BindingError<'db>,
        pending: &mut Vec<&'a Bindings<'db>>,
    ) -> FlatError<'a, 'db> {
        match error {
            BindingError::PropertyGetterCallError(error) => FlatError::Getter {
                child: enqueue(pending, &error.bindings),
                argument_index_offset: error.argument_index_offset,
            },
            BindingError::PropertySetterCallError(error) => FlatError::Setter {
                child: enqueue(pending, &error.bindings),
                argument_index_offset: error.argument_index_offset,
            },
            BindingError::InvalidArgumentType { .. }
            | BindingError::InvalidKeyType { .. }
            | BindingError::MissingArguments { .. }
            | BindingError::UnknownArgument { .. }
            | BindingError::UnknownKeywordVariadicArgument { .. }
            | BindingError::PositionalOnlyParameterAsKwarg { .. }
            | BindingError::TooManyPositionalArguments { .. }
            | BindingError::ParameterAlreadyAssigned { .. }
            | BindingError::SpecializationError { .. }
            | BindingError::PropertyHasNoGetter(_)
            | BindingError::PropertyHasNoSetter(_)
            | BindingError::PropertyHasNoDeleter(_)
            | BindingError::InternalCallError(_)
            | BindingError::UnmatchedOverload
            | BindingError::CalledTopCallable(_)
            | BindingError::InvalidDataclassApplication(_)
            | BindingError::InvalidDataclassArgument(_) => FlatError::Leaf(error),
        }
    }

    /// Captures every semantic field through a breadth-first walk whose equality has fixed depth.
    fn flat_snapshot<'a, 'db>(source: &'a Bindings<'db>) -> Vec<FlatNode<'a, 'db>> {
        let mut pending = vec![source];
        let mut result = Vec::new();
        let mut next = 0;
        while let Some(node) = pending.get(next).copied() {
            next += 1;
            let mut elements = Vec::new();
            for element in &node.elements {
                let mut items = Vec::new();
                for item in &element.items {
                    let kind = match item {
                        CallableItem::Regular(_) => FlatItemKind::Regular,
                        CallableItem::Constructor(constructor) => FlatItemKind::Constructor {
                            context: constructor.constructor_context,
                            downstream: constructor
                                .downstream_constructor
                                .as_deref()
                                .map(|child| enqueue(&mut pending, child)),
                        },
                    };
                    let callable = item.callable();
                    let overload_call_result = match &callable.overload_call_result {
                        None => None,
                        Some(OverloadCallResult::ArgumentTypeExpansion(expanded)) => {
                            Some(FlatOverloadResult::Expanded {
                                return_type: expanded.return_type,
                                selected_overloads: &expanded.selected_overloads,
                            })
                        }
                        Some(OverloadCallResult::ArgumentTypeExpansionLimitReached(limit)) => {
                            Some(FlatOverloadResult::LimitReached(*limit))
                        }
                        Some(OverloadCallResult::Ambiguous) => Some(FlatOverloadResult::Ambiguous),
                    };
                    let overloads = callable
                        .overloads
                        .iter()
                        .map(|binding| FlatBinding {
                            signature: &binding.signature,
                            source_overload_index: binding.source_overload_index,
                            source_parameter_index_offset: binding.source_parameter_index_offset,
                            callable_type: binding.callable_type,
                            signature_type: binding.signature_type,
                            return_ty: binding.return_ty,
                            return_origin: binding.return_origin,
                            constructor_context: binding.constructor_context,
                            inferable_typevars: binding.inferable_typevars,
                            inference: binding.inference,
                            is_partial_application: binding.is_partial_application,
                            argument_matches: binding
                                .argument_matches
                                .iter()
                                .map(|argument| FlatMatchedArgument {
                                    matched: argument.matched,
                                    parameters: argument
                                        .parameters
                                        .iter()
                                        .map(|parameter| FlatMatchedParameter {
                                            index: parameter.index,
                                            argument_type: parameter.argument_type,
                                            expected_type: parameter.expected_type,
                                            provenance: parameter.provenance,
                                        })
                                        .collect(),
                                })
                                .collect(),
                            variadic_argument_matched_to_variadic_parameter: binding
                                .variadic_argument_matched_to_variadic_parameter,
                            parameter_tys: &binding.parameter_tys,
                            errors: binding
                                .errors
                                .iter()
                                .map(|error| flat_error(error, &mut pending))
                                .collect(),
                        })
                        .collect();
                    items.push(FlatCallable {
                        kind,
                        callable_type: callable.callable_type,
                        signature_type: callable.signature_type,
                        dunder_call_is_possibly_unbound: callable.dunder_call_is_possibly_unbound,
                        bound_type: callable.bound_type,
                        descriptor_origin: callable.descriptor_origin,
                        overload_call_result,
                        matching_overload_before_type_checking: callable
                            .matching_overload_before_type_checking,
                        overloads,
                    });
                }
                elements.push(FlatElement {
                    callable_type: element.callable_type,
                    items,
                });
            }
            result.push(FlatNode {
                callable_type: node.callable_type,
                implicit_dunder_new_is_possibly_unbound: node
                    .implicit_dunder_new_is_possibly_unbound,
                implicit_dunder_init_is_possibly_unbound: node
                    .implicit_dunder_init_is_possibly_unbound,
                enclosing_binding_contexts: node.enclosing_binding_contexts.as_deref(),
                elements,
                cleanup_link_is_empty: node.cleanup_next.is_none(),
            });
        }
        result
    }

    /// Counts real destructor entry and completion, including maximum synchronous nesting.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct RetirementCounts {
        begun: usize,
        completed: usize,
        active: usize,
        maximum_active: usize,
    }

    thread_local! {
        static RETIREMENT_COUNTS: Cell<RetirementCounts> = const { Cell::new(RetirementCounts {
            begun: 0, completed: 0, active: 0, maximum_active: 0,
        }) };
    }

    /// Records scalar events without retaining a binding, guard, or borrowed owner.
    fn observe_retirement(event: RetirementEvent<'_>) {
        let mut counts = RETIREMENT_COUNTS.get();
        match event.phase {
            RetirementPhase::Begin => {
                counts.begun += 1;
                counts.active += 1;
                counts.maximum_active = counts.maximum_active.max(counts.active);
            }
            RetirementPhase::Complete => {
                counts.completed += 1;
                counts.active -= 1;
            }
        }
        RETIREMENT_COUNTS.set(counts);
    }

    /// Restores the previous observer even if a test assertion unwinds.
    #[derive(Debug)]
    struct RetirementProbe(Option<RetirementObserver>);

    impl RetirementProbe {
        /// Starts a fresh observation interval on this test thread.
        fn start() -> Self {
            RETIREMENT_COUNTS.set(RetirementCounts::default());
            Self(set_retirement_observer(Some(observe_retirement)))
        }

        fn counts(&self) -> RetirementCounts {
            RETIREMENT_COUNTS.get()
        }
    }

    impl Drop for RetirementProbe {
        fn drop(&mut self) {
            set_retirement_observer(self.0);
        }
    }

    /// Creates a small constructor node with distinct scalar metadata and no child bindings.
    fn plain_node<'db>(index: usize) -> Bindings<'db> {
        let context = ConstructorContext::new(Type::any(), ConstructorCallableKind::New);
        let binding = Binding {
            signature: Signature::new(Parameters::empty(), Type::unknown()),
            source_overload_index: index,
            source_parameter_index_offset: index + 1,
            callable_type: Type::any(),
            signature_type: Type::unknown(),
            return_ty: Type::Never,
            return_origin: DescriptorOrigin {
                incomplete: true,
                return_contains_recursive_recovery: true,
                ..DescriptorOrigin::default()
            },
            constructor_context: Some(context),
            inferable_typevars: TypeVarSet::None,
            inference: None,
            is_partial_application: true,
            argument_matches: Box::from([]),
            variadic_argument_matched_to_variadic_parameter: true,
            parameter_tys: Box::from([]),
            errors: Vec::new(),
        };
        let entry = CallableBinding {
            callable_type: Type::unknown(),
            signature_type: Type::any(),
            dunder_call_is_possibly_unbound: true,
            bound_type: Some(Type::Never),
            descriptor_origin: binding.return_origin,
            overload_call_result: None,
            matching_overload_before_type_checking: Some(0),
            overloads: smallvec![binding],
        };
        Bindings {
            callable_type: Type::unknown(),
            implicit_dunder_new_is_possibly_unbound: true,
            implicit_dunder_init_is_possibly_unbound: true,
            elements: smallvec![BindingsElement {
                callable_type: Type::Never,
                items: smallvec![CallableItem::Constructor(ConstructorBinding {
                    entry,
                    constructor_context: context,
                    downstream_constructor: None,
                })],
            }],
            enclosing_binding_contexts: None,
            cleanup_next: None,
        }
    }

    /// Selects one of the three owning child edges handled by the iterative traversal.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ChildEdge {
        Downstream,
        Getter,
        Setter,
    }

    /// Builds a deep chain iteratively, alternating constructor and both property-error edges.
    fn deep_alternating_tree(depth: usize) -> Bindings<'static> {
        let mut tree = plain_node(depth);
        for (index, edge) in [ChildEdge::Downstream, ChildEdge::Getter, ChildEdge::Setter]
            .into_iter()
            .cycle()
            .take(depth)
            .enumerate()
        {
            let mut parent = plain_node(index);
            match edge {
                ChildEdge::Downstream => {
                    let constructor = super::require_clone_state(
                        parent.elements[0].items[0].as_constructor_mut(),
                    );
                    constructor.downstream_constructor = Some(Box::new(tree));
                }
                ChildEdge::Getter => {
                    parent.elements[0].items[0].callable_mut().overloads[0]
                        .errors
                        .push(BindingError::PropertyGetterCallError(
                            PropertyAccessorCallError {
                                bindings: Box::new(tree),
                                argument_index_offset: index + 11,
                            },
                        ));
                }
                ChildEdge::Setter => {
                    parent.elements[0].items[0].callable_mut().overloads[0]
                        .errors
                        .push(BindingError::PropertySetterCallError(
                            PropertyAccessorCallError {
                                bindings: Box::new(tree),
                                argument_index_offset: index + 11,
                            },
                        ));
                }
            }
            tree = parent;
        }
        tree
    }

    /// Supplies real Salsa handles and a signature retaining a source-overload index and receiver constraints.
    #[derive(Debug)]
    struct LeafInputs<'db> {
        context: BindingContext<'db>,
        typevar: BoundTypeVarInstance<'db>,
        property: PropertyInstanceType<'db>,
        signature: Signature<'db>,
        inferable: TypeVarSet<'db>,
    }

    impl<'db> LeafInputs<'db> {
        /// Creates fixture handles and a signature with a source-overload index and nonterminal receiver constraints.
        fn new(db: &'db TestDb) -> Self {
            let env = db.program_environment();
            let context = BindingContext::Synthetic(env.program(db));
            let typevar = BoundTypeVarInstance::new(
                db,
                TypeVarInstance::new(
                    db,
                    TypeVarIdentity::new(db, Name::new("T"), None, TypeVarKind::LegacyTypeVar),
                    None,
                    None,
                    None,
                ),
                context,
                None,
                TypeVarNonce::NONE,
            );
            let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
                ConstraintSet::constrain_typevar_equivalence_bound(
                    db,
                    &env,
                    builder,
                    typevar,
                    Type::Never,
                )
            });
            assert!(constraints.native_comparison_type_pairs().len() > 0);
            let signature = Signature::new(
                Parameters::standard([
                    Parameter::positional_only(Some(Name::new("named_positional_parameter")))
                        .with_annotated_type(Type::any())
                        .with_default_type(Type::Never),
                    Parameter::variadic(Name::new("variadic_parameter_with_a_shared_long_name"))
                        .with_annotated_type(Type::unknown())
                        .with_starred_annotation(),
                    Parameter::keyword_only(Name::new("keyword_parameter_with_a_shared_long_name"))
                        .with_annotated_type(Type::Never),
                ]),
                Type::any(),
            )
            .with_source_overload_index(Some(7))
            .with_probe_receiver_constraints(constraints)
            .with_recursion_recovery();
            Self {
                context,
                typevar,
                property: PropertyInstanceType::new(db, None, None, None),
                signature,
                inferable: TypeVarSet::from_typevars(db, [typevar]),
            }
        }
    }

    /// Creates a named diagnostic parameter with nondefault source and signature positions.
    fn parameter_context(index: usize) -> ParameterContext {
        let parameter =
            Parameter::variadic(Name::new("diagnostic_parameter_with_a_shared_long_name"));
        let mut context = ParameterContext::new(&parameter, index, false);
        context.source_parameter_index = Some(index + 9);
        context
    }

    /// Creates every nonrecursive error variant, including owned names and parameter-context Vecs.
    fn leaf_errors<'db>(inputs: &LeafInputs<'db>) -> Vec<BindingError<'db>> {
        let mut contexts = Vec::with_capacity(11);
        contexts.extend([
            parameter_context(2),
            parameter_context(5),
            parameter_context(8),
        ]);
        let mut errors = Vec::with_capacity(37);
        errors.extend([
            BindingError::InvalidArgumentType {
                parameter: parameter_context(1),
                argument_index: Some(2),
                last_argument_index: Some(6),
                expected_ty: Type::Never,
                provided_ty: Type::any(),
                provenance: InvalidArgumentTypeProvenance::OpenTypedDictExtraItems,
                parameter_source: None,
            },
            BindingError::InvalidKeyType {
                argument_index: Some(3),
                provided_ty: Type::Never,
            },
            BindingError::MissingArguments {
                parameters: ParameterContexts(contexts),
                paramspec: None,
            },
            BindingError::UnknownArgument {
                argument_name: Name::new("unknown_argument_with_a_shared_long_name"),
                argument_index: Some(4),
            },
            BindingError::UnknownKeywordVariadicArgument {
                argument_index: Some(5),
            },
            BindingError::PositionalOnlyParameterAsKwarg {
                argument_index: Some(6),
                parameter: parameter_context(3),
            },
            BindingError::TooManyPositionalArguments {
                first_excess_argument_index: Some(7),
                expected_positional_count: 2,
                provided_positional_count: 8,
            },
            BindingError::ParameterAlreadyAssigned {
                argument_index: Some(8),
                parameter: parameter_context(4),
            },
            BindingError::SpecializationError {
                error: SpecializationError::MismatchedBound {
                    bound_typevar: inputs.typevar,
                    argument: Type::Never,
                },
                argument_index: Some(9),
            },
            BindingError::SpecializationError {
                error: SpecializationError::MismatchedConstraint {
                    bound_typevar: inputs.typevar,
                    argument: Type::any(),
                },
                argument_index: None,
            },
            BindingError::PropertyHasNoGetter(inputs.property),
            BindingError::PropertyHasNoSetter(inputs.property),
            BindingError::PropertyHasNoDeleter(inputs.property),
            BindingError::InternalCallError("owned binding fixture"),
            BindingError::UnmatchedOverload,
            BindingError::CalledTopCallable(Type::any()),
            BindingError::InvalidDataclassApplication(InvalidDataclassTarget::NamedTuple),
            BindingError::InvalidDataclassApplication(InvalidDataclassTarget::TypedDict),
            BindingError::InvalidDataclassApplication(InvalidDataclassTarget::Enum),
            BindingError::InvalidDataclassApplication(InvalidDataclassTarget::Protocol),
            BindingError::InvalidDataclassArgument(InvalidDataclassArgument::OrderRequiresEq),
            BindingError::InvalidDataclassArgument(
                InvalidDataclassArgument::WeakrefSlotRequiresSlots,
            ),
        ]);
        errors
    }

    /// Fills all owned overload leaves, with spilled and retained-spill parameter-match vectors.
    fn owned_binding<'db>(inputs: &LeafInputs<'db>, index: usize) -> Binding<'db> {
        let parameter = MatchedParameter {
            index: 2,
            argument_type: Some(Type::any()),
            expected_type: Some(Type::Never),
            provenance: InvalidArgumentTypeProvenance::OpenTypedDictExtraItems,
        };
        let mut spilled = SmallVec::with_capacity(9);
        spilled.extend([
            parameter,
            MatchedParameter {
                index: 5,
                ..parameter
            },
            MatchedParameter {
                index: 8,
                argument_type: None,
                expected_type: None,
                provenance: InvalidArgumentTypeProvenance::Argument,
            },
        ]);
        let mut retained_spill = SmallVec::with_capacity(7);
        retained_spill.push(parameter);
        Binding {
            signature: inputs.signature.clone(),
            source_overload_index: index + 2,
            source_parameter_index_offset: index + 3,
            callable_type: Type::unknown(),
            signature_type: Type::Never,
            return_ty: Type::any(),
            return_origin: DescriptorOrigin {
                incomplete: true,
                return_contains_recursive_recovery: true,
                ..DescriptorOrigin::default()
            },
            constructor_context: Some(ConstructorContext::new(
                Type::any(),
                ConstructorCallableKind::Init,
            )),
            inferable_typevars: inputs.inferable,
            inference: None,
            is_partial_application: true,
            argument_matches: Box::from([
                MatchedArgument {
                    parameters: spilled,
                    matched: false,
                },
                MatchedArgument {
                    parameters: retained_spill,
                    matched: true,
                },
                MatchedArgument::default(),
            ]),
            variadic_argument_matched_to_variadic_parameter: true,
            parameter_tys: Box::from([None, Some(Type::Never), Some(Type::any())]),
            errors: leaf_errors(inputs),
        }
    }

    /// Chooses all overload-result shapes, including both spilled and inline selected-index lists.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ResultShape {
        ExpandedSpilled,
        ExpandedInline,
        LimitReached,
        Ambiguous,
        Absent,
    }

    /// Builds one callable with the requested number of independently owned overloads.
    fn owned_callable<'db>(
        inputs: &LeafInputs<'db>,
        overloads: usize,
        shape: ResultShape,
    ) -> CallableBinding<'db> {
        let overload_call_result = match shape {
            ResultShape::ExpandedSpilled => {
                let mut selected_overloads = SmallVec::with_capacity(11);
                selected_overloads.extend([2, 0, 1]);
                Some(OverloadCallResult::ArgumentTypeExpansion(Box::new(
                    ExpandedOverloadCall {
                        return_type: Type::Never,
                        selected_overloads,
                    },
                )))
            }
            ResultShape::ExpandedInline => Some(OverloadCallResult::ArgumentTypeExpansion(
                Box::new(ExpandedOverloadCall {
                    return_type: Type::any(),
                    selected_overloads: smallvec![0, 1],
                }),
            )),
            ResultShape::LimitReached => {
                Some(OverloadCallResult::ArgumentTypeExpansionLimitReached(17))
            }
            ResultShape::Ambiguous => Some(OverloadCallResult::Ambiguous),
            ResultShape::Absent => None,
        };
        CallableBinding {
            callable_type: Type::any(),
            signature_type: Type::unknown(),
            dunder_call_is_possibly_unbound: true,
            bound_type: Some(Type::Never),
            descriptor_origin: DescriptorOrigin {
                incomplete: true,
                return_contains_recursive_recovery: true,
                ..DescriptorOrigin::default()
            },
            overload_call_result,
            matching_overload_before_type_checking: Some(0),
            overloads: (0..overloads)
                .map(|index| owned_binding(inputs, index))
                .collect(),
        }
    }

    /// Builds a wide tree with repeated elements, items, overloads, and all three owning edges.
    fn wide_owned_leaf_fixture<'db>(inputs: &LeafInputs<'db>) -> Bindings<'db> {
        let mut elements = SmallVec::with_capacity(7);
        for element_index in 0..3 {
            let mut items = SmallVec::with_capacity(9);
            for (item_index, shape) in [
                ResultShape::ExpandedSpilled,
                ResultShape::ExpandedInline,
                ResultShape::LimitReached,
                ResultShape::Ambiguous,
                ResultShape::Absent,
            ]
            .into_iter()
            .enumerate()
            {
                let mut callable = owned_callable(inputs, 3, shape);
                callable.overloads[1]
                    .errors
                    .push(BindingError::PropertyGetterCallError(
                        PropertyAccessorCallError {
                            bindings: Box::new(plain_node(element_index + 100)),
                            argument_index_offset: item_index + 13,
                        },
                    ));
                callable.overloads[2]
                    .errors
                    .push(BindingError::PropertySetterCallError(
                        PropertyAccessorCallError {
                            bindings: Box::new(plain_node(element_index + 200)),
                            argument_index_offset: item_index + 19,
                        },
                    ));
                items.push(if item_index == 0 {
                    CallableItem::Constructor(ConstructorBinding {
                        entry: callable,
                        constructor_context: ConstructorContext::new(
                            Type::Never,
                            ConstructorCallableKind::MetaclassCall,
                        ),
                        downstream_constructor: Some(Box::new(plain_node(element_index + 300))),
                    })
                } else {
                    CallableItem::Regular(callable)
                });
            }
            elements.push(BindingsElement {
                callable_type: Type::any(),
                items,
            });
        }
        Bindings {
            callable_type: Type::Never,
            implicit_dunder_new_is_possibly_unbound: true,
            implicit_dunder_init_is_possibly_unbound: false,
            elements,
            enclosing_binding_contexts: Some(Box::from([inputs.context, inputs.context])),
            cleanup_next: None,
        }
    }

    /// Verifies deep mixed child edges preserve semantic fields and retire with bounded destructor nesting.
    #[test_case::test_case(CloneRoute::Ordinary; "ordinary")]
    #[cfg_attr(feature = "experimental-analysis", test_case::test_case(CloneRoute::Controlled; "controlled"))]
    fn deep_alternating_children_clone_and_drop(route: CloneRoute) -> anyhow::Result<()> {
        const DEPTH: usize = 4_096;
        let source = deep_alternating_tree(DEPTH);
        let cloned = clone_tree(&source, route)?;
        let source_snapshot = flat_snapshot(&source);
        assert_eq!(source_snapshot.len(), DEPTH + 1);
        assert_eq!(flat_snapshot(&cloned), source_snapshot);
        drop(source_snapshot);
        let probe = RetirementProbe::start();
        drop(source);
        drop(cloned);
        let counts = probe.counts();
        assert_eq!(counts.begun, 2 * (DEPTH + 1));
        assert_eq!(counts.completed, counts.begun);
        assert_eq!(counts.active, 0);
        assert!(
            counts.maximum_active <= 2,
            "recursive ownership drop: {counts:?}"
        );
        Ok(())
    }

    /// Verifies all semantic leaf fields survive a wide clone after the source is destroyed.
    #[test_case::test_case(CloneRoute::Ordinary; "ordinary")]
    #[cfg_attr(feature = "experimental-analysis", test_case::test_case(CloneRoute::Controlled; "controlled"))]
    fn wide_tree_preserves_all_owned_leaves(route: CloneRoute) -> anyhow::Result<()> {
        let db = setup_db();
        let inputs = LeafInputs::new(&db);
        let source = wide_owned_leaf_fixture(&inputs);
        let cloned = clone_tree(&source, route)?;
        assert_eq!(flat_snapshot(&cloned), flat_snapshot(&source));
        let nodes = flat_snapshot(&source).len();
        let probe = RetirementProbe::start();
        drop(source);
        // Rebuilding the expected value after source destruction also checks retained Name and Arc owners.
        let expected = wide_owned_leaf_fixture(&inputs);
        assert_eq!(flat_snapshot(&cloned), flat_snapshot(&expected));
        drop(inputs);
        drop(expected);
        drop(cloned);
        let counts = probe.counts();
        assert_eq!(counts.begun, 3 * nodes);
        assert_eq!(counts.completed, counts.begun);
        assert_eq!(counts.active, 0);
        assert!(
            counts.maximum_active <= 2,
            "recursive ownership drop: {counts:?}"
        );
        Ok(())
    }

    /// Verifies the immediately ready provider agrees with the ordinary Clone entry point.
    #[test]
    fn inline_provider_matches_ordinary_clone() {
        let source = deep_alternating_tree(6);
        let inline = legacy_inline(clone_bindings_with(&source, &InlineCloneBindingsEffects));
        let ordinary = source.clone();
        assert_eq!(flat_snapshot(&inline), flat_snapshot(&ordinary));
    }

    #[cfg(feature = "experimental-analysis")]
    mod controlled {
        //! Exercises numeric work and byte refusals through the immediately ready `BudgetCloneEffects` provider.

        use super::*;

        /// Records independently accumulated work and bytes at one successful admission.
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        struct Cost {
            work: usize,
            bytes: usize,
        }

        /// Identifies the budget dimension that refused an operation, or invalid quote arithmetic.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(super) enum Refusal {
            Work,
            Bytes,
            Overflow,
        }

        /// Enforces numeric limits before executing closures and records the admitted prefix.
        #[derive(Debug)]
        pub(super) struct BudgetCloneEffects {
            limit: Cost,
            used: Cell<Cost>,
            attempted: Cell<usize>,
            completed: Cell<usize>,
            prefix: RefCell<Vec<Cost>>,
            clone_before: Cell<usize>,
            clone_after: Cell<usize>,
            signatures: Cell<usize>,
            nodes_created: Cell<usize>,
        }

        impl BudgetCloneEffects {
            const fn new(limit: Cost) -> Self {
                Self {
                    limit,
                    used: Cell::new(Cost { work: 0, bytes: 0 }),
                    attempted: Cell::new(0),
                    completed: Cell::new(0),
                    prefix: RefCell::new(Vec::new()),
                    clone_before: Cell::new(0),
                    clone_after: Cell::new(0),
                    signatures: Cell::new(0),
                    nodes_created: Cell::new(0),
                }
            }

            pub(super) const fn unlimited() -> Self {
                Self::new(Cost {
                    work: usize::MAX,
                    bytes: usize::MAX,
                })
            }
        }

        impl<'db> CloneBindingsEffects<'db> for BudgetCloneEffects {
            type Error = Refusal;

            async fn local<T>(
                &self,
                work: Option<usize>,
                bytes: Option<usize>,
                operation: impl FnOnce() -> T,
            ) -> Result<T, Refusal> {
                self.attempted.set(self.attempted.get() + 1);
                let used = self.used.get();
                let work = work
                    .and_then(|work| work.checked_add(2))
                    .and_then(|work| used.work.checked_add(work))
                    .ok_or(Refusal::Overflow)?;
                let bytes = bytes
                    .and_then(|bytes| bytes.checked_add(size_of_val(&operation)))
                    .and_then(|bytes| bytes.checked_add(size_of::<T>()))
                    .and_then(|bytes| bytes.checked_add(size_of::<Result<T, Refusal>>()))
                    .and_then(|bytes| used.bytes.checked_add(bytes))
                    .ok_or(Refusal::Overflow)?;
                if work > self.limit.work {
                    return Err(Refusal::Work);
                }
                if bytes > self.limit.bytes {
                    return Err(Refusal::Bytes);
                }
                let cost = Cost { work, bytes };
                self.used.set(cost);
                self.prefix.borrow_mut().push(cost);
                let result = operation();
                self.completed.set(self.completed.get() + 1);
                Ok(result)
            }

            async fn signature_clone(
                &self,
                source: &Signature<'db>,
            ) -> Result<Signature<'db>, Refusal> {
                let (work, bytes) = self
                    .local(Some(3), Some(0), || {
                        (
                            source
                                .retirement_work()
                                .and_then(|work| work.checked_add(8)),
                            source.clone_requested_bytes(),
                        )
                    })
                    .await?;
                self.local(work, bytes, || {
                    self.signatures.set(self.signatures.get() + 1);
                    source.clone()
                })
                .await
            }

            fn clone_before(&self) {
                self.clone_before.set(self.clone_before.get() + 1);
            }

            fn clone_after(&self) {
                self.clone_after.set(self.clone_after.get() + 1);
            }

            fn clone_node_created(&self) {
                self.nodes_created.set(self.nodes_created.get() + 1);
            }
        }

        /// Executes the immediately ready controlled traversal while preserving its refusal result.
        pub(super) fn run_controlled_clone<'db>(
            source: &Bindings<'db>,
            effects: &BudgetCloneEffects,
        ) -> Result<Bindings<'db>, Refusal> {
            legacy_inline(async { Ok(clone_bindings_with(source, effects).await) })
        }

        /// Supplies every owned leaf plus one constructor, getter, and setter child for refusal sweeps.
        fn refusal_fixture<'db>(inputs: &LeafInputs<'db>) -> Bindings<'db> {
            let mut callable = owned_callable(inputs, 3, ResultShape::ExpandedSpilled);
            callable.overloads[0]
                .errors
                .push(BindingError::PropertyGetterCallError(
                    PropertyAccessorCallError {
                        bindings: Box::new(plain_node(71)),
                        argument_index_offset: 5,
                    },
                ));
            callable.overloads[2]
                .errors
                .push(BindingError::PropertySetterCallError(
                    PropertyAccessorCallError {
                        bindings: Box::new(plain_node(73)),
                        argument_index_offset: 9,
                    },
                ));
            Bindings {
                callable_type: Type::Never,
                implicit_dunder_new_is_possibly_unbound: true,
                implicit_dunder_init_is_possibly_unbound: false,
                elements: smallvec![BindingsElement {
                    callable_type: Type::unknown(),
                    items: smallvec![CallableItem::Constructor(ConstructorBinding {
                        entry: callable,
                        constructor_context: ConstructorContext::new(
                            Type::any(),
                            ConstructorCallableKind::New
                        ),
                        downstream_constructor: Some(Box::new(plain_node(79))),
                    })],
                }],
                enclosing_binding_contexts: Some(Box::from([inputs.context])),
                cleanup_next: None,
            }
        }

        /// Selects the resource whose numeric admission boundaries are exercised.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Dimension {
            Work,
            Bytes,
        }

        impl Dimension {
            const fn value(self, cost: Cost) -> usize {
                match self {
                    Self::Work => cost.work,
                    Self::Bytes => cost.bytes,
                }
            }

            const fn expected_refusal(self) -> Refusal {
                match self {
                    Self::Work => Refusal::Work,
                    Self::Bytes => Refusal::Bytes,
                }
            }

            const fn limit(self, limit: usize) -> Cost {
                match self {
                    Self::Work => Cost {
                        work: limit,
                        bytes: usize::MAX,
                    },
                    Self::Bytes => Cost {
                        work: usize::MAX,
                        bytes: limit,
                    },
                }
            }
        }

        /// Verifies a budget one unit below each positive cumulative admission boundary refuses before
        /// its closure and retires every partial output owner.
        #[test_case::test_case(Dimension::Work; "work")]
        #[test_case::test_case(Dimension::Bytes; "bytes")]
        fn every_numeric_boundary_refuses_before_mutation(
            dimension: Dimension,
        ) -> anyhow::Result<()> {
            let db = setup_db();
            let inputs = LeafInputs::new(&db);
            let source = refusal_fixture(&inputs);
            let expected = flat_snapshot(&source);
            let unlimited = BudgetCloneEffects::unlimited();
            let successful = run_controlled_clone(&source, &unlimited)
                .map_err(|error| anyhow::anyhow!("unlimited run failed: {error:?}"))?;
            assert_eq!(flat_snapshot(&successful), expected);
            assert_eq!(unlimited.attempted.get(), unlimited.completed.get());
            assert_eq!(unlimited.clone_before.get(), 1);
            assert_eq!(unlimited.clone_after.get(), 1);
            assert_eq!(unlimited.signatures.get(), 6);
            assert_eq!(unlimited.nodes_created.get(), 4);
            drop(successful);
            let prefix = unlimited.prefix.into_inner();
            let mut previous = 0;
            let mut exercised = 0;
            // The ordered prefix comes from this fixture's actual allocations and traversal.
            // Each iteration depends on the previous cumulative cost, so keep this a single sweep.
            for (index, cost) in prefix.iter().copied().enumerate() {
                let boundary = dimension.value(cost);
                if boundary == previous {
                    continue;
                }
                previous = boundary;
                exercised += 1;
                let effects = BudgetCloneEffects::new(dimension.limit(boundary - 1));
                let probe = RetirementProbe::start();
                let result = run_controlled_clone(&source, &effects);
                assert_eq!(
                    result.err(),
                    Some(dimension.expected_refusal()),
                    "operation {index}"
                );
                assert_eq!(effects.attempted.get(), index + 1, "operation {index}");
                assert_eq!(
                    effects.completed.get(),
                    index,
                    "rejected closure {index} ran"
                );
                assert_eq!(
                    *effects.prefix.borrow(),
                    prefix[..index],
                    "operation {index}"
                );
                assert_eq!(
                    flat_snapshot(&source),
                    expected,
                    "source changed at operation {index}"
                );
                let counts = probe.counts();
                assert_eq!(
                    counts.completed, counts.begun,
                    "operation {index}: {counts:?}"
                );
                assert_eq!(
                    counts.completed,
                    effects.nodes_created.get(),
                    "operation {index}: created owner was not retired"
                );
                assert_eq!(counts.active, 0, "operation {index}: {counts:?}");
                assert!(
                    counts.maximum_active <= 2,
                    "recursive cleanup at operation {index}: {counts:?}"
                );
            }
            assert!(exercised > 0);
            let exact = BudgetCloneEffects::new(unlimited.used.get());
            let completed = run_controlled_clone(&source, &exact)
                .map_err(|error| anyhow::anyhow!("exact complete budget failed: {error:?}"))?;
            assert_eq!(flat_snapshot(&completed), expected);
            Ok(())
        }
    }
}
