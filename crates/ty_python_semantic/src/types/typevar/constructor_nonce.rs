//! Shared nonce decisions for freshening generic callable and constructor occurrences.

use std::alloc::Layout;
use std::cell::RefCell;
use std::convert::Infallible;

use super::{
    BindingContext, BoundTypeVarInstance, TypeVarNonce, TypeVarNonceGenerator,
    TypeVarNonceGeneratorInner,
};
use crate::Db;
use crate::types::GenericContext;
use crate::types::constraints::control::hash_slots;
use crate::types::generics::context_construction::ContextVariables;

/// Identifies an admission boundary that can change the nonce generator.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ConstructorNonceOperation {
    Create,
    EnclosingInsert,
    SeenInsert,
    Increment,
}

/// Supplies admitted storage operations and canonical fields for nonce decisions.
pub(in crate::types) trait ConstructorNonceEffects<'db> {
    type Error;

    /// Admits local work and storage, including the fixed result and error transfers.
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Reads a generic context's variables in their stored encounter order.
    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error>;

    /// Reads the binding context from a bound variable's canonical identity.
    async fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BindingContext<'db>, Self::Error>;

    /// Observes an attempted mutation immediately before its admission.
    #[cfg(test)]
    fn before_nonce(&self, _operation: ConstructorNonceOperation) {}

    /// Observes a completed mutation after its admission succeeds.
    #[cfg(test)]
    fn after_nonce(&self, _operation: ConstructorNonceOperation) {}
}

/// Runs the shared nonce decisions with ordinary, synchronous field access.
#[derive(Debug)]
pub(in crate::types) struct InlineConstructorNonceEffects;

impl<'db> ConstructorNonceEffects<'db> for InlineConstructorNonceEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(db)))
    }

    async fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BindingContext<'db>, Self::Error> {
        Ok(variable.binding_context(db))
    }
}

/// Selects the set and scalar identity used by one insertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NonceEntry<'db> {
    Enclosing(BindingContext<'db>),
    Seen(GenericContext<'db>),
}

impl NonceEntry<'_> {
    #[cfg(test)]
    const fn operation(self) -> ConstructorNonceOperation {
        match self {
            Self::Enclosing(_) => ConstructorNonceOperation::EnclosingInsert,
            Self::Seen(_) => ConstructorNonceOperation::SeenInsert,
        }
    }
}

/// Metadata obtained by the admitted lookup preceding a set insertion.
#[derive(Clone, Copy, Debug)]
struct SetMetadata {
    len: usize,
    capacity: usize,
    present: bool,
}

/// Storage and work for a single insertion, including any replacement table's retirement.
#[derive(Clone, Copy, Debug)]
struct SetInsertionQuote {
    work: usize,
    bytes: usize,
    additional: usize,
}

/// Quotes one bounded-key lookup and the short borrow that contains it.
fn probe_work(capacity: usize) -> Option<usize> {
    hash_slots::<Infallible>(capacity)
        .ok()?
        .checked_mul(4)?
        .checked_add(12)
}

/// Quotes growth for an insert-only set of generic or binding-context identities.
fn insertion_quote<T>(metadata: SetMetadata) -> Option<SetInsertionQuote> {
    let required = metadata.len.checked_add(usize::from(!metadata.present))?;
    let old_slots = hash_slots::<Infallible>(metadata.capacity).ok()?;
    if required <= metadata.capacity {
        return Some(SetInsertionQuote {
            // Probe, scalar insertion, and eventual retirement of the inserted identity.
            work: old_slots.checked_mul(4)?.checked_add(20)?,
            bytes: 0,
            additional: 0,
        });
    }

    let requested_capacity = metadata.capacity.checked_mul(2)?.max(required).max(4);
    // The pinned hashbrown backend stays within twice this logical capacity. The slot bound
    // includes control bytes and the smallest table; neither set removes entries.
    let new_slots = hash_slots::<Infallible>(requested_capacity.checked_mul(2)?).ok()?;
    let bytes = new_slots.checked_mul(size_of::<T>().checked_add(1)?)?;
    Layout::from_size_align(bytes, align_of::<T>()).ok()?;

    // Rehashing can probe the replacement table for every retained key. Also fund scans and
    // relocation, old-table retirement, and cleanup of the new table after interruption.
    let work = metadata
        .len
        .checked_add(1)?
        .checked_mul(new_slots.checked_add(1)?)?
        .checked_mul(4)?
        .checked_add(old_slots.checked_mul(2)?)?
        .checked_add(new_slots.checked_mul(2)?)?
        .checked_add(metadata.len.checked_mul(4)?)?
        .checked_add(32)?;
    Some(SetInsertionQuote {
        work,
        bytes,
        additional: required.checked_sub(metadata.len)?,
    })
}

/// Quotes the reference-counted generator allocation, including the two reference counters.
fn generator_bytes<'db>() -> Option<usize> {
    let (layout, _) = Layout::new::<[usize; 2]>()
        .extend(Layout::new::<RefCell<TypeVarNonceGeneratorInner<'db>>>())
        .ok()?;
    Some(layout.pad_to_align().size())
}

impl<'db> TypeVarNonceGenerator<'db> {
    /// Creates one matching generator and records the supplied enclosing binding contexts.
    pub(in crate::types) async fn new_for_matching_with<E: ConstructorNonceEffects<'db>>(
        enclosing: &[BindingContext<'db>],
        effects: &E,
    ) -> Result<Self, E::Error> {
        #[cfg(test)]
        effects.before_nonce(ConstructorNonceOperation::Create);
        let generator = effects
            .local(Some(32), generator_bytes(), || {
                let generator = Self::default();
                #[cfg(test)]
                effects.after_nonce(ConstructorNonceOperation::Create);
                generator
            })
            .await?;
        let mut index = 0;
        loop {
            let context = effects
                .local(Some(6), Some(0), || {
                    let context = enclosing.get(index).copied();
                    if context.is_some() {
                        index += 1;
                    }
                    context
                })
                .await?;
            let Some(context) = context else {
                break;
            };
            generator
                .insert_with(NonceEntry::Enclosing(context), effects)
                .await?;
        }
        effects
            .local(Some(2), Some(size_of::<Self>()), || ())
            .await?;
        Ok(generator)
    }

    /// Determines whether a context repeats an enclosing or previously seen occurrence.
    ///
    /// A context whose variables all belong to one enclosing binding context is recursive and
    /// needs freshening immediately. That case leaves `seen` unchanged. Empty contexts, contexts
    /// outside the enclosing set, and mixed contexts use the first-versus-repeated insertion.
    pub(in crate::types) async fn should_freshen_with<E: ConstructorNonceEffects<'db>>(
        &self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let variables = effects.variables(db, generic_context).await?;
        let first = effects
            .local(Some(4), Some(0), || {
                GenericContext::variable_at_in(variables, 0)
            })
            .await?;
        if let Some(first) = first {
            let binding_context = effects.binding_context(db, first).await?;
            let capacity = effects
                .local(Some(8), Some(0), || {
                    self.inner.borrow().enclosing.capacity()
                })
                .await?;
            // A context inherited from an enclosing definition can be merged with another context.
            // Only the unmerged context represents a recursive occurrence that needs freshening.
            let enclosing = effects
                .local(probe_work(capacity), Some(0), || {
                    self.inner.borrow().enclosing.contains(&binding_context)
                })
                .await?;
            if enclosing {
                let mut index = 1;
                loop {
                    let variable = effects
                        .local(Some(6), Some(0), || {
                            let variable = GenericContext::variable_at_in(variables, index);
                            if variable.is_some() {
                                index += 1;
                            }
                            variable
                        })
                        .await?;
                    let Some(variable) = variable else {
                        return effects.local(Some(2), Some(0), || true).await;
                    };
                    let other = effects.binding_context(db, variable).await?;
                    let same = effects
                        .local(Some(4), Some(0), || other == binding_context)
                        .await?;
                    if !same {
                        break;
                    }
                }
            }
        }
        let inserted = self
            .insert_with(NonceEntry::Seen(generic_context), effects)
            .await?;
        effects.local(Some(2), Some(0), || !inserted).await
    }

    /// Returns the current nonce and advances the generator exactly once after admission.
    pub(in crate::types) async fn next_with<E: ConstructorNonceEffects<'db>>(
        &self,
        effects: &E,
    ) -> Result<TypeVarNonce, E::Error> {
        #[cfg(test)]
        effects.before_nonce(ConstructorNonceOperation::Increment);
        effects
            .local(Some(16), Some(0), || {
                #[cfg(test)]
                super::OWNERSHIP_PROBE_NONCES.with(|counts| {
                    let (starts, allocations) = counts.get();
                    counts.set((starts, allocations + 1));
                });
                let nonce = {
                    let mut inner = self.inner.borrow_mut();
                    let nonce = inner.next;
                    inner.next = nonce.increment();
                    nonce
                };
                #[cfg(test)]
                effects.after_nonce(ConstructorNonceOperation::Increment);
                nonce
            })
            .await
    }

    /// Inserts one identity after admitting probes, any table growth, and eventual cleanup.
    async fn insert_with<E: ConstructorNonceEffects<'db>>(
        &self,
        entry: NonceEntry<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let capacity = effects
            .local(Some(10), Some(0), || {
                let inner = self.inner.borrow();
                match entry {
                    NonceEntry::Enclosing(_) => inner.enclosing.capacity(),
                    NonceEntry::Seen(_) => inner.seen.capacity(),
                }
            })
            .await?;
        let metadata = effects
            .local(probe_work(capacity), Some(0), || {
                let inner = self.inner.borrow();
                match entry {
                    NonceEntry::Enclosing(context) => SetMetadata {
                        len: inner.enclosing.len(),
                        capacity: inner.enclosing.capacity(),
                        present: inner.enclosing.contains(&context),
                    },
                    NonceEntry::Seen(context) => SetMetadata {
                        len: inner.seen.len(),
                        capacity: inner.seen.capacity(),
                        present: inner.seen.contains(&context),
                    },
                }
            })
            .await?;
        let quote = effects
            .local(Some(64), Some(0), || match entry {
                NonceEntry::Enclosing(_) => insertion_quote::<BindingContext<'db>>(metadata),
                NonceEntry::Seen(_) => insertion_quote::<GenericContext<'db>>(metadata),
            })
            .await?;
        #[cfg(test)]
        effects.before_nonce(entry.operation());
        effects
            .local(
                quote.map(|quote| quote.work),
                quote.map(|quote| quote.bytes),
                || {
                    let inserted = {
                        let mut inner = self.inner.borrow_mut();
                        match entry {
                            NonceEntry::Enclosing(context) => {
                                if let Some(quote) = quote
                                    && quote.additional != 0
                                {
                                    inner.enclosing.reserve(quote.additional);
                                }
                                inner.enclosing.insert(context)
                            }
                            NonceEntry::Seen(context) => {
                                if let Some(quote) = quote
                                    && quote.additional != 0
                                {
                                    inner.seen.reserve(quote.additional);
                                }
                                inner.seen.insert(context)
                            }
                        }
                    };
                    #[cfg(test)]
                    effects.after_nonce(entry.operation());
                    inserted
                },
            )
            .await
    }
}
