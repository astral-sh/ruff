use std::any::TypeId;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ptr::NonNull;

#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Event as TransferEvent, Kind, MemoSnapshot, Slots,
};
use crate::attempt_probe::{self, MemoReuse, QueryPolicy};
use crate::cycle::{
    CycleHeads, CycleHeadsIterator, IterationStamp, ProvisionalStatus, empty_cycle_heads,
};
use crate::database::AsDynDatabase;
#[cfg(test)]
use crate::function::CopyMemoProfile;
use crate::function::delete::PreparedRetirement;
use crate::function::{ClaimResult, Configuration, IngredientImpl, PassiveMemoProfile, Reentrancy};
use crate::ingredient::Ingredient;
use crate::key::DatabaseKeyIndex;
use crate::memo_ingredient_indices::MemoIngredientMap;
use crate::prepared_source_probe::Stamp;
use crate::revision::AtomicRevision;
use crate::sync::atomic::Ordering;
use crate::table::memo::{DummyMemo, MemoSlot, MemoTableWithTypesMut, PreparedMemoSlot, ToDynMemo};
use crate::zalsa::{MemoIngredientIndex, Zalsa, ZalsaDatabase};
use crate::zalsa_local::{QueryOriginRef, QueryRevisions};
use crate::{Cancelled, Event, EventKind, Id, Revision};

mod prepared_source;
pub use prepared_source::{PreparedSourceError, PreparedSourceMemo};

#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub enum KeyRetirementError {
    Mapping,
    Outputs,
    WorkOverflow,
    Quote(crate::quote::QuoteError),
    #[cfg(feature = "accumulator")]
    DirectAccumulators,
}

impl From<crate::quote::QuoteError> for KeyRetirementError {
    fn from(error: crate::quote::QuoteError) -> Self {
        Self::Quote(error)
    }
}

impl KeyRetirementError {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Mapping => "fixed query key memo mapping is unsupported",
            Self::Outputs => "fixed query key memo has tracked outputs",
            Self::Quote(crate::quote::QuoteError::Exhausted) => {
                "retirement quotation needs more prepaid work"
            }
            Self::Quote(crate::quote::QuoteError::Overflow) => "retirement quotation work overflow",
            Self::Quote(crate::quote::QuoteError::Unsupported) => {
                "retirement quotation is unsupported"
            }
            Self::WorkOverflow => "fixed query key memo retirement work overflow",
            #[cfg(feature = "accumulator")]
            Self::DirectAccumulators => "fixed query key memo has direct accumulators",
        }
    }

    pub(super) fn value_message(self) -> &'static str {
        match self {
            Self::Mapping => "finite interned value memo mapping is unsupported",
            Self::Outputs => "finite interned value memo has tracked outputs",
            Self::Quote(crate::quote::QuoteError::Exhausted) => {
                "retirement quotation needs more prepaid work"
            }
            Self::Quote(crate::quote::QuoteError::Overflow) => "retirement quotation work overflow",
            Self::Quote(crate::quote::QuoteError::Unsupported) => {
                "retirement quotation is unsupported"
            }
            Self::WorkOverflow => "finite interned value memo retirement work overflow",
            #[cfg(feature = "accumulator")]
            Self::DirectAccumulators => "finite interned value memo has direct accumulators",
        }
    }
}

impl<C: Configuration> IngredientImpl<C> {
    pub(super) fn passive_memo_index(
        &self,
        zalsa: &Zalsa,
        argument: &dyn Ingredient,
    ) -> Result<MemoIngredientIndex, KeyRetirementError> {
        let index = self
            .memo_ingredient_indices
            .get_checked(argument.ingredient_index())
            .ok_or(KeyRetirementError::Mapping)?;
        if !argument.memo_table_types().has_memo_type::<Memo<C>>(index)
            || zalsa.ingredient_index_for_memo(argument.ingredient_index(), index) != self.index
        {
            return Err(KeyRetirementError::Mapping);
        }
        Ok(index)
    }

    pub(super) fn query_key_memo_index(
        &self,
        zalsa: &Zalsa,
        argument: &dyn Ingredient,
    ) -> Result<MemoIngredientIndex, KeyRetirementError> {
        let index = self.passive_memo_index(zalsa, argument)?;
        if index.as_usize() != 0
            || !argument
                .memo_table_types()
                .has_single_memo_type::<Memo<C>>()
        {
            return Err(KeyRetirementError::Mapping);
        }
        Ok(index)
    }
}

#[cfg(test)]
pub(super) fn check_passive_key_memo<C: Configuration>(
    table: MemoTableWithTypesMut<'_>,
    index: MemoIngredientIndex,
) -> Result<(), KeyRetirementError>
where
    for<'db> C::Output<'db>: Copy,
{
    inspect_passive_singleton::<C, CopyMemoProfile>(table, index).map(|_| ())
}

pub(super) fn inspect_passive_singleton<C, P>(
    mut table: MemoTableWithTypesMut<'_>,
    index: MemoIngredientIndex,
) -> Result<MemoRetirementQuote, KeyRetirementError>
where
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    if index.as_usize() != 0 || !table.has_single_memo_type::<Memo<C>>() {
        return Err(KeyRetirementError::Mapping);
    }
    inspect_passive_memo::<C, P>(&mut table, index)
}

pub(super) fn inspect_passive_singleton_bounded<C, P>(
    mut table: MemoTableWithTypesMut<'_>,
    index: MemoIngredientIndex,
    fuel: &mut crate::quote::QuoteFuel,
) -> Result<MemoRetirementQuote, KeyRetirementError>
where
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    fuel.consume(1)?;
    if index.as_usize() != 0 || !table.has_single_memo_type::<Memo<C>>() {
        return Err(KeyRetirementError::Mapping);
    }
    inspect_passive_memo_bounded::<C, P>(&mut table, index, fuel)
}

pub(super) fn inspect_passive_memo_bounded<C, P>(
    table: &mut MemoTableWithTypesMut<'_>,
    index: MemoIngredientIndex,
    fuel: &mut crate::quote::QuoteFuel,
) -> Result<MemoRetirementQuote, KeyRetirementError>
where
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    inspect_passive_memo_with::<C, P>(table, index, Some(fuel))
}

pub(super) struct MemoRetirementQuote {
    pub(super) present: bool,
    pub(super) output_units: usize,
}

pub(super) fn inspect_passive_memo<C, P>(
    table: &mut MemoTableWithTypesMut<'_>,
    index: MemoIngredientIndex,
) -> Result<MemoRetirementQuote, KeyRetirementError>
where
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    inspect_passive_memo_with::<C, P>(table, index, None)
}

fn inspect_passive_memo_with<C, P>(
    table: &mut MemoTableWithTypesMut<'_>,
    index: MemoIngredientIndex,
    mut fuel: Option<&mut crate::quote::QuoteFuel>,
) -> Result<MemoRetirementQuote, KeyRetirementError>
where
    C: Configuration,
    P: PassiveMemoProfile<C>,
{
    if let Some(fuel) = fuel.as_deref_mut() {
        fuel.consume(1)?;
    }
    if !table.has_memo_type::<Memo<C>>(index) {
        return Err(KeyRetirementError::Mapping);
    }
    let mut result = Ok(MemoRetirementQuote {
        present: false,
        output_units: 0,
    });
    table.reborrow().map_memo::<Memo<C>>(index, |memo| {
        if let Some(fuel) = fuel.as_deref_mut()
            && let Err(error) =
                fuel.consume(memo.header.output_check_work(MemoOutputCheck::OutputsOnly))
        {
            result = Err(error.into());
            return;
        }
        if !memo.header.outputs_are_empty() {
            result = Err(KeyRetirementError::Outputs);
            return;
        }
        #[cfg(feature = "accumulator")]
        if memo.header.revisions.accumulated().is_some() {
            result = Err(KeyRetirementError::DirectAccumulators);
            return;
        }
        let units = match memo.value.as_ref() {
            None => Ok(0),
            Some(output) => match fuel.as_deref_mut() {
                Some(fuel) => P::retired_output_work_bounded(output, fuel).map_err(Into::into),
                None => P::retired_output_work(output).ok_or(KeyRetirementError::WorkOverflow),
            },
        };
        result = units.map(|output_units| MemoRetirementQuote {
            present: true,
            output_units,
        });
    });
    result
}

impl<C: Configuration> IngredientImpl<C> {
    pub(super) fn memo_preparation_bytes(&self, zalsa: &Zalsa, id: Id) -> Option<usize> {
        size_of::<Memo<C>>()
            .checked_add(super::delete::DeletedEntries::<C>::entry_size())?
            .checked_add(
                zalsa
                    .memo_table_for::<C::SalsaStruct<'_>>(id)
                    .preparation_bytes()?,
            )
    }

    pub(super) fn prepare_memo_slot<'db>(
        &self,
        zalsa: &'db Zalsa,
        id: Id,
        index: MemoIngredientIndex,
    ) -> Option<PreparedMemoSlot<'db, Memo<C>>> {
        zalsa
            .memo_table_for::<C::SalsaStruct<'_>>(id)
            .prepare_slot(index)
    }

    pub(super) fn install_prepared_memo<'db>(
        &'db self,
        slot: PreparedMemoSlot<'db, Memo<C>>,
        memo: PreparedMemo<'db, C>,
        retirement: Option<PreparedRetirement<'db, C>>,
    ) -> &'db Memo<C> {
        self.install_memo_allocation(slot, memo.allocation, retirement)
    }

    /// Loads the current memo for `key_index`. This does not hold any sort of
    /// lock on the `memo_map` once it returns, so this memo could immediately
    /// become outdated if other threads store into the `memo_map`.
    pub(super) fn get_memo_from_table_for<'db>(
        &self,
        zalsa: &'db Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Option<&'db Memo<C>> {
        let memo = zalsa
            .memo_table_for::<C::SalsaStruct<'_>>(id)
            .get(memo_ingredient_index)?;
        // SAFETY: The memo table owns this allocation for at least `'db`.
        Some(unsafe { memo.as_ref() })
    }

    pub(super) fn memo_slot<'db>(
        &self,
        zalsa: &'db Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> MemoSlot<'db> {
        // SAFETY: The table stores 'static memos (to support `Any`), but the memos remain valid
        // for `'db` because dropping them is delayed until the end of the revision.
        unsafe {
            MemoSlot::new(
                zalsa.memo_table_for::<C::SalsaStruct<'_>>(id),
                memo_ingredient_index,
            )
        }
    }

    /// Evicts the existing memo for the given key, replacing it
    /// with an equivalent memo that has no value. If the memo is untracked
    /// or has values assigned as output of another query, this has no effect.
    pub(super) fn evict_value_from_memo_for(
        table: MemoTableWithTypesMut<'_>,
        memo_ingredient_index: MemoIngredientIndex,
    ) {
        let map = |memo: &mut Memo<C>| {
            if memo.header.can_evict_value() {
                // Set the memo value to `None`.
                memo.value = None;
            }
        };

        table.map_memo(memo_ingredient_index, map)
    }
}

/// The final memo allocation, still owned by an unpublished query result.
pub(super) struct PreparedMemo<'db, C: Configuration> {
    allocation: Box<Memo<C>>,
    database: PhantomData<&'db ()>,
}

impl<'db, C: Configuration> PreparedMemo<'db, C> {
    pub(super) fn new(
        value: C::Output<'db>,
        revision: Revision,
        revisions: QueryRevisions,
    ) -> Self {
        Self {
            allocation: prepare_memo_allocation(Memo::new(Some(value), revision, revisions)),
            database: PhantomData,
        }
    }
}

pub(super) fn prepare_memo_allocation<C: Configuration>(mut memo: Memo<C>) -> Box<Memo<C>> {
    if let Some(ids) = memo.header.revisions.tracked_struct_ids_mut() {
        ids.shrink_to_fit();
    }
    Box::new(memo)
}

/// A memoized query result.
///
/// # Layout
///
/// [`ErasedMemo`] retains a pointer to the base address of the `Memo` allocation, with spatial
/// provenance covering the entire allocation, so that it can recover the typed memo. Placing
/// `header` at offset zero also lets it access [`MemoHeader`] with a direct pointer cast. The C
/// representation makes that offset stable.
#[repr(C)]
#[derive(Debug)]
pub struct Memo<C: Configuration> {
    /// Configuration-independent state used to validate and manage this memo.
    ///
    /// Must be at offset zero for [`ErasedMemo::header`].
    pub(super) header: MemoHeader,

    /// The result of the query, if we decide to memoize it.
    pub(super) value: Option<C::Output<'static>>,
}

/// A shared, type-erased handle to a [`Memo`].
///
/// `data` points to the base address of the `Memo<C>` allocation with spatial provenance
/// covering the entire allocation, which remains valid for shared access for `'db`, even after
/// replacement. `to_dyn_fn` and `type_id` describe the same `C`.
#[derive(Clone, Copy)]
pub(crate) struct ErasedMemo<'db> {
    /// A pointer to the base address of the [`Memo`] allocation, with spatial provenance covering
    /// the entire allocation.
    data: NonNull<DummyMemo>,

    /// Coerces `data` to a trait object using the vtable for its concrete memo type.
    to_dyn_fn: ToDynMemo,

    /// The concrete memo type, used to assert that downcasts match the registered memo type.
    type_id: TypeId,

    /// Binds shared access to the allocation lifetime.
    _lifetime: PhantomData<&'db ()>,
}

impl<'memo> ErasedMemo<'memo> {
    #[cfg(all(test, not(feature = "shuttle")))]
    pub(in crate::function) fn transfer_test_snapshot(self) -> MemoSnapshot {
        self.header().transfer_test_snapshot(self.has_value())
    }

    /// Constructs an erased handle from a memo allocation pointer and its type metadata.
    ///
    /// # Safety
    ///
    /// `data` must point to the base address of a live, aligned `Memo<C>` allocation, have
    /// spatial provenance covering the entire allocation, and remain valid for shared access for
    /// `'memo`. `to_dyn_fn` must be the trait-object coercion for `Memo<C>`, and `type_id` must be
    /// `TypeId::of::<Memo<C>>()` for that same `C`.
    #[inline]
    pub(crate) unsafe fn from_raw_parts(
        data: NonNull<DummyMemo>,
        to_dyn_fn: ToDynMemo,
        type_id: TypeId,
    ) -> Self {
        Self {
            data,
            to_dyn_fn,
            type_id,
            _lifetime: PhantomData,
        }
    }

    /// Returns the configuration-independent header without a table lookup.
    #[inline(always)]
    pub(super) fn header(self) -> &'memo MemoHeader {
        // SAFETY: `data` points to the base address of a `Memo` allocation valid for `'memo`, with
        // spatial provenance covering the allocation, and `Memo::header` has offset zero.
        unsafe { self.data.cast::<MemoHeader>().as_ref() }
    }

    /// Returns whether the memo currently contains a value.
    #[inline]
    pub(super) fn has_value(self) -> bool {
        // SAFETY: `to_dyn_fn` matches the concrete memo allocation, which is valid for shared
        // access for `'memo`.
        unsafe { (self.to_dyn_fn)(self.data).as_ref() }.has_value()
    }

    /// Returns the concrete memo after asserting that it uses configuration `C`.
    ///
    /// # Panics
    ///
    /// Panics if the memo was created for a different configuration, matching
    /// [`MemoTableWithTypes::get`](crate::table::memo::MemoTableWithTypes::get).
    #[inline]
    pub(super) fn downcast<C: Configuration>(self) -> &'memo Memo<C> {
        assert_eq!(
            self.type_id,
            TypeId::of::<Memo<C>>(),
            "ErasedMemo downcast with the wrong configuration",
        );

        // SAFETY: The type check proves that `data` points to `Memo<C>`; the handle guarantees
        // that the allocation is valid for shared access for `'memo`.
        unsafe { self.data.cast::<Memo<C>>().as_ref() }
    }
}

#[derive(Clone, Copy)]
pub(super) enum MemoOutputCheck {
    OutputsOnly,
    Controlled,
}

impl MemoOutputCheck {
    pub(super) fn work(self, revisions: &QueryRevisions) -> usize {
        #[cfg(feature = "accumulator")]
        if matches!(self, Self::Controlled) && revisions.accumulated().is_some() {
            return 0;
        }
        if revisions.tracked_struct_ids().is_empty() {
            revisions.origin().output_scan_work()
        } else {
            0
        }
    }
}

#[derive(Debug)]
pub(super) struct MemoHeader {
    /// Last revision when this memo was verified; this begins
    /// as the current revision.
    pub(super) verified_at: AtomicRevision,

    /// Revision information
    pub(super) revisions: QueryRevisions,
}

impl MemoHeader {
    pub(super) fn output_check_work(&self, check: MemoOutputCheck) -> usize {
        check.work(&self.revisions)
    }

    #[cfg(all(test, not(feature = "shuttle")))]
    pub(in crate::function) fn transfer_test_snapshot(&self, has_value: bool) -> MemoSnapshot {
        let mut heads = Slots::default();
        for head in self.revisions.cycle_heads().iter() {
            heads.push((head.database_key_index, head.iteration.load()));
        }
        MemoSnapshot {
            identity: std::ptr::from_ref(self).addr(),
            has_value,
            verified_at: self.verified_at.load(),
            execution_revision: self.revisions.execution_revision(),
            iteration: self.revisions.iteration(),
            heads,
            converged: self.revisions.cycle_converged(),
            final_: self.revisions.verified_final.load(Ordering::Acquire),
            changed_at: self.revisions.changed_at,
            durability: self.revisions.durability,
            support: self
                .revisions
                .attempt_support()
                .map(transfer_trace::support_snapshot),
        }
    }

    pub(super) fn attempt_reuse(&self, zalsa: &Zalsa) -> MemoReuse {
        let mut result = self
            .revisions
            .attempt_support()
            .map_or(MemoReuse::Ordinary, |support| {
                support.reuse(zalsa, self.may_be_provisional())
            });
        // A provisional read and its next iteration must belong to the same evaluation.
        // Token completion alone cannot make a foreign predecessor safe to consume:
        // seeding, recovery and convergence must all retain the same history.
        if result == MemoReuse::Ordinary
            && self.may_be_provisional()
            && !self.revisions.attempt_support().map_or_else(
                || crate::attempt_probe::current_cycle_support(zalsa).is_none(),
                |support| support.is_current(zalsa),
            )
        {
            result = MemoReuse::Stale;
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = TransferEvent::new(Kind::Reuse);
            event.identity = std::ptr::from_ref(self).addr();
            event.support = self
                .revisions
                .attempt_support()
                .map(transfer_trace::support_snapshot);
            event.reuse = Some(result);
            transfer_trace::record(event);
        }
        result
    }

    pub(super) fn has_incomplete_attempt(&self) -> bool {
        self.revisions.has_incomplete_attempt()
    }

    /// Routes a retained participant to claimed finality verification. This does not permit
    /// reading or seeding from its approximation: the exact heads must first prove completion.
    pub(super) fn is_finality_candidate(&self) -> bool {
        self.may_be_provisional()
            && self.revisions.execution_revision().is_some()
            && !self.revisions.cycle_heads().is_empty()
            && !self
                .revisions
                .attempt_support()
                .is_some_and(|support| support.incomplete(false))
    }

    pub(super) fn same_attempt_owner(&self, other: &Self) -> bool {
        match (
            self.revisions.attempt_support(),
            other.revisions.attempt_support(),
        ) {
            (None, None) => true,
            (Some(left), Some(right)) => left.same_owner(right),
            _ => false,
        }
    }

    pub(super) fn can_seed_attempt(&self, zalsa: &Zalsa) -> bool {
        let result = self.attempt_reuse(zalsa) == MemoReuse::Ordinary;
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = TransferEvent::new(Kind::SeedAllowed).decision(result);
            event.identity = std::ptr::from_ref(self).addr();
            event.support = self
                .revisions
                .attempt_support()
                .map(transfer_trace::support_snapshot);
            transfer_trace::record(event);
        }
        result
    }

    fn new(revision_now: Revision, mut revisions: QueryRevisions) -> Self {
        debug_assert!(
            !revisions.verified_final.load(Ordering::Relaxed) || revisions.cycle_heads().is_empty(),
            "Memo must be finalized if it has no cycle heads"
        );
        revisions.record_execution_revision(revision_now);
        Self {
            verified_at: AtomicRevision::from(revision_now),
            revisions,
        }
    }

    #[inline]
    pub(super) fn origin(&self) -> QueryOriginRef<'_> {
        self.revisions.origin()
    }

    fn can_evict_value(&self) -> bool {
        // Careful: Cannot evict memos whose values were
        // assigned as output of another query
        // or those with untracked inputs
        // as their values cannot be reconstructed.
        matches!(self.origin(), QueryOriginRef::Derived(_))
    }

    /// True if this may be a provisional cycle-iteration result.
    #[inline]
    pub(super) fn may_be_provisional(&self) -> bool {
        // A nested final flag release-publishes its cycle's accepted root. This acquire also
        // carries that publication through participants which subsequently validate as final.
        !self.revisions.verified_final.load(Ordering::Acquire)
    }

    /// Cycle heads that should be propagated to dependent queries.
    #[inline(always)]
    pub(super) fn cycle_heads(&self) -> &CycleHeads {
        if self.may_be_provisional() {
            self.revisions.cycle_heads()
        } else {
            empty_cycle_heads()
        }
    }

    /// Returns `true` if this memo was part of a cycle in it's last iteration.
    #[inline(always)]
    pub(super) fn was_cycle_participant(&self) -> bool {
        !self.revisions.cycle_heads().is_empty()
    }

    /// Mark memo as having been verified in the `revision_now`, which should
    /// be the current revision.
    /// The caller is responsible to update the memo's `accumulated` state if their accumulated
    /// values have changed since.
    #[inline]
    pub(super) fn mark_as_verified(&self, zalsa: &Zalsa, database_key_index: DatabaseKeyIndex) {
        let verification =
            MemoVerification::new(self, zalsa, database_key_index, zalsa.current_revision());
        verification.event();
        verification.publish();
    }

    pub(super) fn outputs_are_empty(&self) -> bool {
        self.revisions.tracked_struct_ids().is_empty()
            && self.revisions.origin().outputs().next().is_none()
    }

    pub(super) fn controlled_outputs_are_empty(&self) -> bool {
        #[cfg(feature = "accumulator")]
        if self.revisions.accumulated().is_some() {
            return false;
        }
        self.outputs_are_empty()
    }

    pub(super) fn mark_outputs_as_verified(
        &self,
        zalsa: &Zalsa,
        database_key_index: DatabaseKeyIndex,
    ) {
        for output in self.revisions.origin().outputs() {
            output.mark_validated_output(zalsa, database_key_index);
        }
    }

    pub(super) fn tracing_debug(&self, has_value: bool) -> impl std::fmt::Debug + use<'_> {
        struct TracingDebug<'memo> {
            header: &'memo MemoHeader,
            has_value: bool,
        }

        impl std::fmt::Debug for TracingDebug<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("Memo")
                    .field(
                        "value",
                        if self.has_value {
                            &"Some(<value>)"
                        } else {
                            &"None"
                        },
                    )
                    .field("verified_at", &self.header.verified_at)
                    .field("revisions", &self.header.revisions)
                    .finish()
            }
        }

        TracingDebug {
            header: self,
            has_value,
        }
    }

    pub(super) fn remove_outputs(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex) {
        for stale_output in self.revisions.origin().outputs() {
            stale_output.remove_stale_output(zalsa, executor);
        }

        for (identity, id) in self.revisions.tracked_struct_ids() {
            let key = DatabaseKeyIndex::new(identity.ingredient_index(), *id);
            key.remove_stale_output(zalsa, executor);
        }
    }
}

/// The exact memo and revision selected before a verification event runs.
pub(super) struct MemoVerification<'db> {
    header: &'db MemoHeader,
    zalsa: &'db Zalsa,
    key: DatabaseKeyIndex,
    revision: Revision,
}

impl<'db> MemoVerification<'db> {
    #[cfg(test)]
    pub(super) fn key(&self) -> DatabaseKeyIndex {
        self.key
    }

    pub(super) fn new(
        header: &'db MemoHeader,
        zalsa: &'db Zalsa,
        key: DatabaseKeyIndex,
        revision: Revision,
    ) -> Self {
        Self {
            header,
            zalsa,
            key,
            revision,
        }
    }

    pub(super) fn output_check_work(&self, check: MemoOutputCheck) -> usize {
        self.header.output_check_work(check)
    }

    pub(super) fn outputs_are_empty(&self) -> bool {
        self.header.outputs_are_empty()
    }

    pub(super) fn controlled_outputs_are_empty(&self) -> bool {
        self.header.controlled_outputs_are_empty()
    }

    pub(super) fn event(&self) {
        self.zalsa.event(&|| {
            Event::new(EventKind::DidValidateMemoizedValue {
                database_key: self.key,
            })
        });
    }

    pub(super) fn publish(self) {
        self.header.verified_at.store(self.revision);
    }

    pub(super) fn publish_shallow(self) {
        let Self {
            header, zalsa, key, ..
        } = self;
        self.publish();
        header.mark_outputs_as_verified(zalsa, key);
    }
}

/// A selected memo whose value remains borrowed for the entire read epilogue.
pub(super) struct SelectedMemo<'db, C: Configuration> {
    memo: &'db Memo<C>,
    value: &'db C::Output<'db>,
}

impl<'db, C: Configuration> SelectedMemo<'db, C> {
    pub(super) fn new(memo: &'db Memo<C>) -> Option<Self> {
        Some(Self {
            memo,
            value: memo.value()?,
        })
    }

    pub(super) fn memo(&self) -> &'db Memo<C> {
        self.memo
    }

    pub(super) fn value(&self) -> &'db C::Output<'db> {
        self.value
    }
}

/// A final memo selected after its ordinary query completed, before an execution attempt.
/// The database borrow retains the allocation; reads still recheck its table identity.
pub struct FinalSourceMemo<'db, C: Configuration> {
    db: &'db C::DbView,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
    memo_index: MemoIngredientIndex,
    stamp: Stamp,
    selected: SelectedMemo<'db, C>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalSourceError {
    ActiveAttempt,
    ActiveQuery,
    ActiveOperation,
    ForeignIngredient,
    UnsupportedPolicy,
    MissingMemo,
    MissingValue,
    StaleStamp,
    UnverifiedMemo,
    ProvisionalMemo,
    IncompleteMemo,
    OutputBearingMemo,
    ReplacedMemo,
}

impl FinalSourceError {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::ActiveAttempt => "final source certification has an active attempt",
            Self::ActiveQuery => "final source certification has an active or borrowed query stack",
            Self::ActiveOperation => "final source certification has an active operation",
            Self::ForeignIngredient => "final source has a foreign database or ingredient",
            Self::UnsupportedPolicy => "final source requires an explicitly classified query",
            Self::MissingMemo => "final source memo is missing",
            Self::MissingValue => "final source memo has no value",
            Self::StaleStamp => "final source database stamp changed",
            Self::UnverifiedMemo => "final source memo is not verified in the current revision",
            Self::ProvisionalMemo => "final source memo is provisional",
            Self::IncompleteMemo => "final source memo has incomplete attempt support",
            Self::OutputBearingMemo => "final source memo has tracked outputs",
            Self::ReplacedMemo => "final source memo was replaced",
        }
    }
}

impl<C: Configuration> Debug for FinalSourceMemo<'_, C> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinalSourceMemo")
            .field("key", &self.database_key())
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl<'db, C: Configuration> FinalSourceMemo<'db, C> {
    /// Certifies an existing final memo without fetching, executing, or verifying a query.
    /// Call the ordinary query first, using its real generated key.
    pub fn certify(
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
        id: Id,
    ) -> Result<Self, FinalSourceError> {
        if attempt_probe::current().is_some() {
            return Err(FinalSourceError::ActiveAttempt);
        }
        if db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.is_empty())
            != Some(true)
        {
            return Err(FinalSourceError::ActiveQuery);
        }
        if attempt_probe::stack_depths() != (0, 0) {
            return Err(FinalSourceError::ActiveOperation);
        }
        if !matches!(
            C::ATTEMPT_POLICY,
            QueryPolicy::CompleteOnly | QueryPolicy::ReturnOnly
        ) {
            return Err(FinalSourceError::UnsupportedPolicy);
        }
        if !db
            .zalsa()
            .ingredients()
            .nth(ingredient.index.as_u32() as usize)
            .is_some_and(|known| std::ptr::addr_eq(known, ingredient as &dyn Ingredient))
        {
            return Err(FinalSourceError::ForeignIngredient);
        }
        let stamp = Stamp::current(db.as_dyn_database());
        let memo_index = ingredient.memo_ingredient_index(db.zalsa(), id);
        let memo = ingredient
            .get_memo_from_table_for(db.zalsa(), id, memo_index)
            .ok_or(FinalSourceError::MissingMemo)?;
        Self::check_header(db, memo)?;
        let selected = SelectedMemo::new(memo).ok_or(FinalSourceError::MissingValue)?;
        // This check can scan outgoing edges. The immutable selected allocation preserves its
        // result, so live source reads only repeat the scalar identity and finality checks.
        if !memo.header.outputs_are_empty() {
            return Err(FinalSourceError::OutputBearingMemo);
        }
        let result = Self {
            db,
            ingredient,
            id,
            memo_index,
            stamp,
            selected,
        };
        result.check_current()?;
        if attempt_probe::current().is_some() {
            return Err(FinalSourceError::ActiveAttempt);
        }
        Ok(result)
    }

    pub fn database_key(&self) -> DatabaseKeyIndex {
        self.ingredient.database_key_index(self.id)
    }

    pub(super) fn id(&self) -> Id {
        self.id
    }

    pub(super) fn belongs_to(&self, db: &C::DbView, ingredient: &IngredientImpl<C>) -> bool {
        std::ptr::eq(self.db.zalsa(), db.zalsa())
            && std::ptr::eq(self.db.zalsa_local(), db.zalsa_local())
            && std::ptr::eq(self.ingredient, ingredient)
    }

    pub(super) fn copy_handle(&self) -> Self {
        Self {
            db: self.db,
            ingredient: self.ingredient,
            id: self.id,
            memo_index: self.memo_index,
            stamp: self.stamp,
            selected: SelectedMemo {
                memo: self.selected.memo,
                value: self.selected.value,
            },
        }
    }

    pub(super) fn selected(&self) -> &SelectedMemo<'db, C> {
        &self.selected
    }

    pub(super) fn check_current(&self) -> Result<(), FinalSourceError> {
        if !self.stamp.belongs_to(self.db.as_dyn_database()) {
            return Err(FinalSourceError::StaleStamp);
        }
        let current = self
            .ingredient
            .get_memo_from_table_for(self.db.zalsa(), self.id, self.memo_index)
            .ok_or(FinalSourceError::MissingMemo)?;
        if !std::ptr::eq(current, self.selected.memo()) {
            return Err(FinalSourceError::ReplacedMemo);
        }
        Self::check_header(self.db, current)?;
        if current.value.is_none() {
            return Err(FinalSourceError::MissingValue);
        }
        Ok(())
    }

    fn check_header(db: &C::DbView, memo: &Memo<C>) -> Result<(), FinalSourceError> {
        if memo.header.verified_at.load() != db.zalsa().current_revision() {
            return Err(FinalSourceError::UnverifiedMemo);
        }
        if memo.header.may_be_provisional() {
            return Err(FinalSourceError::ProvisionalMemo);
        }
        if memo.header.attempt_reuse(db.zalsa()) != MemoReuse::Ordinary {
            return Err(FinalSourceError::IncompleteMemo);
        }
        Ok(())
    }
}

impl<C: Configuration> Memo<C> {
    #[cfg(all(test, not(feature = "shuttle")))]
    pub(in crate::function) fn transfer_test_snapshot(&self) -> MemoSnapshot {
        self.header.transfer_test_snapshot(self.value.is_some())
    }

    pub(super) fn new(
        value: Option<C::Output<'_>>,
        revision_now: Revision,
        revisions: QueryRevisions,
    ) -> Self {
        Self {
            value: value.map(|value| {
                // SAFETY: Guaranteed by `Configuration` and retained only in this memo.
                unsafe { std::mem::transmute::<C::Output<'_>, C::Output<'static>>(value) }
            }),
            header: MemoHeader::new(revision_now, revisions),
        }
    }

    pub(super) fn value(&self) -> Option<&C::Output<'_>> {
        self.value.as_ref().map(|value| {
            // SAFETY: Guaranteed by `Configuration`; the restored lifetime is
            // bounded by the borrow of this memo.
            unsafe { std::mem::transmute::<&C::Output<'static>, &C::Output<'_>>(value) }
        })
    }

    /// Returns `true` if this memo should be serialized.
    pub(super) fn should_serialize(&self) -> bool {
        // TODO: Serialization is a good opportunity to prune old query results based on
        // the `verified_at` revision.
        self.value.is_some()
            && !self.header.may_be_provisional()
            && !self.header.has_incomplete_attempt()
    }

    pub(super) fn tracing_debug(&self) -> impl std::fmt::Debug + use<'_, C> {
        self.header.tracing_debug(self.value.is_some())
    }
}

impl<C: Configuration> crate::table::memo::Memo for Memo<C> {
    fn has_value(&self) -> bool {
        self.value.is_some()
    }

    fn remove_outputs(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex) {
        self.header.remove_outputs(zalsa, executor);
    }

    #[cfg(feature = "salsa_unstable")]
    fn memory_usage(&self) -> crate::database::MemoInfo {
        let size_of = std::mem::size_of::<Memo<C>>() + self.header.revisions.allocation_size();
        let heap_size = self.value().map_or(Some(0), C::heap_size);

        crate::database::MemoInfo {
            debug_name: C::DEBUG_NAME,
            output: crate::database::SlotInfo {
                size_of_metadata: size_of - std::mem::size_of::<C::Output<'static>>(),
                debug_name: std::any::type_name::<C::Output<'static>>(),
                size_of_fields: std::mem::size_of::<C::Output<'static>>(),
                heap_size_of_fields: heap_size,
                memos: Vec::new(),
            },
        }
    }
}

#[cfg(feature = "persistence")]
mod persistence {
    use crate::function::Configuration;
    use crate::function::memo::{Memo, MemoHeader};
    use crate::revision::AtomicRevision;
    use crate::zalsa_local::QueryRevisions;
    use crate::zalsa_local::persistence::{MappedQueryRevisions, PersistentQueryOrigin};

    use serde::Deserialize;
    use serde::ser::SerializeStruct;

    /// A reference to the fields of a [`Memo`], with its [`QueryRevisions`] transformed.
    pub(crate) struct MappedMemo<'memo, C: Configuration> {
        pub(crate) value: Option<&'memo C::Output<'memo>>,
        pub(crate) verified_at: AtomicRevision,
        pub(crate) revisions: MappedQueryRevisions<'memo>,
    }

    impl<C: Configuration> Memo<C> {
        pub(crate) fn with_origin(
            &self,
            serialized_origin: PersistentQueryOrigin,
        ) -> MappedMemo<'_, C> {
            let value = self.value();
            let Memo { ref header, .. } = *self;
            let MemoHeader {
                ref verified_at,
                ref revisions,
            } = *header;

            MappedMemo {
                value,
                verified_at: AtomicRevision::from(verified_at.load()),
                revisions: revisions.with_origin(serialized_origin),
            }
        }
    }

    impl<C> serde::Serialize for MappedMemo<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            struct SerializeValue<'me, 'db, C: Configuration>(&'me C::Output<'db>);

            impl<C> serde::Serialize for SerializeValue<'_, '_, C>
            where
                C: Configuration,
            {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    C::serialize(self.0, serializer)
                }
            }

            let MappedMemo {
                value,
                verified_at,
                revisions,
            } = self;

            let value = value.expect(
                "attempted to serialize memo where `Memo::should_serialize` returned `false`",
            );

            let mut s = serializer.serialize_struct("Memo", 3)?;
            s.serialize_field("value", &SerializeValue::<C>(value))?;
            s.serialize_field("verified_at", &verified_at)?;
            s.serialize_field("revisions", &revisions)?;
            s.end()
        }
    }

    impl<'de, C> serde::Deserialize<'de> for Memo<C>
    where
        C: Configuration,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(Deserialize)]
            #[serde(rename = "Memo")]
            pub struct DeserializeMemo<C: Configuration> {
                #[serde(bound = "C: Configuration")]
                value: DeserializeValue<C>,
                verified_at: AtomicRevision,
                revisions: QueryRevisions,
            }

            struct DeserializeValue<C: Configuration>(C::Output<'static>);

            impl<'de, C> serde::Deserialize<'de> for DeserializeValue<C>
            where
                C: Configuration,
            {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    C::deserialize(deserializer)
                        .map(DeserializeValue)
                        .map_err(serde::de::Error::custom)
                }
            }

            let memo = DeserializeMemo::<C>::deserialize(deserializer)?;

            // Restoring a final value is not a new execution. In particular, its validation
            // revision cannot supply evidence for a provisional component in this process.
            Ok(Memo {
                value: Some(memo.value.0),
                header: MemoHeader {
                    verified_at: memo.verified_at,
                    revisions: memo.revisions,
                },
            })
        }
    }

    #[cfg(all(test, not(feature = "shuttle")))]
    mod tests {
        use super::Memo;
        use crate::cycle::{CycleHeads, IterationStamp};
        use crate::function::{Configuration, IngredientImpl};
        use crate::sync::atomic::AtomicBool;
        use crate::zalsa::ZalsaDatabase;
        use crate::zalsa_local::persistence::PersistentQueryOrigin;
        use crate::zalsa_local::{
            OriginAndExtra, OutputOrder, QueryRevisions, QueryRevisionsExtra,
        };
        use crate::{Database, DatabaseImpl, Durability, Revision};

        #[crate::tracked(returns(copy), persist)]
        fn persisted_value(_db: &dyn Database) -> u32 {
            7
        }

        fn round_trip<C: Configuration>(
            _ingredient: &IngredientImpl<C>,
            value: C::Output<'static>,
        ) -> Memo<C> {
            // Existing metadata is essential here: calling the fresh constructor during
            // restoration would otherwise leave the absent stamp unchanged by accident.
            let extra = QueryRevisionsExtra::new(
                #[cfg(feature = "accumulator")]
                Default::default(),
                Default::default(),
                CycleHeads::default(),
                IterationStamp::default(),
                true,
                None,
                OutputOrder::Execution,
            );
            let revision = Revision::start();
            let revisions = QueryRevisions {
                changed_at: revision,
                durability: Durability::HIGH,
                origin_and_extra: OriginAndExtra::derived(std::iter::empty(), extra),
                #[cfg(feature = "accumulator")]
                accumulated_inputs: Default::default(),
                verified_final: AtomicBool::new(true),
            };
            let memo = Memo::<C>::new(Some(value), revision, revisions);
            assert_eq!(memo.header.revisions.execution_revision(), Some(revision));
            let serialized =
                serde_json::to_value(memo.with_origin(PersistentQueryOrigin::derived([]))).unwrap();
            assert!(serialized["revisions"]["extra"].is_object());
            assert!(
                serialized["revisions"]["extra"]
                    .get("execution_revision")
                    .is_none()
            );
            let restored: Memo<C> = serde_json::from_value(serialized).unwrap();
            assert_eq!(restored.header.revisions.execution_revision(), None);
            assert_eq!(restored.header.verified_at.load(), revision);
            assert_eq!(restored.header.revisions.changed_at, revision);
            assert_eq!(restored.header.revisions.durability, Durability::HIGH);
            assert!(!restored.header.may_be_provisional());
            assert!(!restored.header.is_finality_candidate());
            restored
        }

        #[test]
        fn restoring_a_final_memo_does_not_record_a_fresh_execution() {
            let db = DatabaseImpl::default();
            let restored = round_trip(persisted_value::fn_ingredient_(&db, db.zalsa()), 7);
            assert_eq!(restored.value(), Some(&7));
        }
    }
}

#[derive(Debug)]
pub(super) enum TryClaimHeadsResult<'a> {
    /// Claiming the cycle head results in a cycle.
    Cycle {
        head_iteration: IterationStamp,
        header: &'a MemoHeader,
    },

    /// The cycle head is not finalized, but it can be claimed.
    Available,

    /// The cycle head is currently executed on another thread.
    Running,
}

/// Iterator to try claiming the transitive cycle heads of a memo.
pub(super) struct TryClaimCycleHeadsIter<'a> {
    zalsa: &'a Zalsa,

    cycle_heads: CycleHeadsIterator<'a>,
}

impl<'a> TryClaimCycleHeadsIter<'a> {
    pub(super) fn new(zalsa: &'a Zalsa, cycle_heads: &'a CycleHeads) -> Self {
        Self {
            zalsa,

            cycle_heads: cycle_heads.iter(),
        }
    }
}

impl<'a> Iterator for TryClaimCycleHeadsIter<'a> {
    type Item = TryClaimHeadsResult<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let head = self.cycle_heads.next()?;
        let head_database_key = head.database_key_index;
        let head_key_index = head_database_key.key_index();
        let ingredient = self
            .zalsa
            .lookup_ingredient(head_database_key.ingredient_index());
        let function = ingredient
            .as_function()
            .expect("cycle heads must be function ingredients");

        match function
            .sync_table()
            .peek_claim(self.zalsa, head_key_index, Reentrancy::Deny)
        {
            ClaimResult::Cycle { .. } => {
                // We hit a cycle blocking on the cycle head; this means this query actively
                // participates in the cycle and some other query is blocked on this thread.
                crate::tracing::trace!("Waiting for {head_database_key:?} results in a cycle");

                let Some(memo) = function.memo(self.zalsa, head_key_index) else {
                    return Some(TryClaimHeadsResult::Available);
                };
                let header = memo.header();
                match header.provisional_status(memo.has_value()) {
                    ProvisionalStatus::Incomplete => return Some(TryClaimHeadsResult::Available),
                    ProvisionalStatus::Provisional | ProvisionalStatus::Final => {}
                    ProvisionalStatus::Poisoned {
                        iteration,
                        verified_at,
                    } => {
                        if verified_at == self.zalsa.current_revision()
                            && iteration.cancellation_count()
                                == self.zalsa.runtime().cancellation_count()
                        {
                            Cancelled::PropagatedPanic.throw();
                        }

                        return Some(TryClaimHeadsResult::Available);
                    }
                }

                Some(TryClaimHeadsResult::Cycle {
                    head_iteration: head.iteration.load(),
                    header,
                })
            }
            ClaimResult::Running(running) => {
                crate::tracing::trace!("Ingredient {head_database_key:?} is running: {running:?}");

                Some(TryClaimHeadsResult::Running)
            }
            ClaimResult::Claimed(()) => Some(TryClaimHeadsResult::Available),
        }
    }
}

#[cfg(all(not(feature = "shuttle"), target_pointer_width = "64"))]
mod _memory_usage {
    use crate::cycle::CycleRecoveryStrategy;
    use crate::ingredient::Location;
    use crate::plumbing::{self, IngredientIndices, MemoIngredientSingletonIndex, SalsaStructInDb};
    use crate::table::memo::MemoTableWithTypes;
    use crate::zalsa::Zalsa;
    use crate::{Database, Id, Revision};

    use std::any::TypeId;
    use std::num::NonZeroUsize;

    // Required by `ErasedMemo::header`.
    const _: () = assert!(std::mem::offset_of!(super::Memo<DummyConfiguration>, header) == 0);
    const _: () = assert!(
        std::mem::offset_of!(super::Memo<DummyConfiguration>, value)
            == std::mem::size_of::<super::MemoHeader>()
    );

    // Memos are stored a lot, make sure their size doesn't randomly increase.
    const _: [(); std::mem::size_of::<super::MemoHeader>()] =
        [(); std::mem::size_of::<[usize; 4]>()];
    const _: [(); std::mem::size_of::<super::Memo<DummyConfiguration>>()] =
        [(); std::mem::size_of::<[usize; 5]>()];
    const _: [(); std::mem::size_of::<super::ErasedMemo<'static>>()] =
        [(); std::mem::size_of::<[usize; 4]>()];

    struct DummyStruct;

    impl SalsaStructInDb for DummyStruct {
        type MemoIngredientMap = MemoIngredientSingletonIndex;
        const LEAF_TYPE_IDS: &'static [typeid::ConstTypeId] = &[];

        fn lookup_ingredient_index(_: &Zalsa) -> IngredientIndices {
            unimplemented!()
        }

        fn cast(_: Id, _: TypeId) -> Option<Self> {
            unimplemented!()
        }

        unsafe fn memo_table(_: &Zalsa, _: Id, _: Revision) -> MemoTableWithTypes<'_> {
            unimplemented!()
        }

        fn entries(_: &Zalsa) -> impl Iterator<Item = crate::DatabaseKeyIndex> + '_ {
            std::iter::empty()
        }
    }

    struct DummyConfiguration;

    // SAFETY: `NonZeroUsize` is `'static` and contains no database lifetime.
    unsafe impl super::Configuration for DummyConfiguration {
        const DEBUG_NAME: &'static str = "";
        const LOCATION: Location = Location { file: "", line: 0 };
        const PERSIST: bool = false;
        const CYCLE_STRATEGY: CycleRecoveryStrategy = CycleRecoveryStrategy::Panic;

        type DbView = dyn Database;
        type SalsaStruct<'db> = DummyStruct;
        type Input<'db> = ();
        type Output<'db> = NonZeroUsize;
        type Eviction = crate::function::eviction::NoopEviction;

        fn values_equal<'db>(_: &Self::Output<'db>, _: &Self::Output<'db>) -> bool {
            unimplemented!()
        }

        fn id_to_input(_: &Zalsa, _: Id) -> Self::Input<'_> {
            unimplemented!()
        }

        fn execute<'db>(_: &'db Self::DbView, _: Self::Input<'db>) -> Self::Output<'db> {
            unimplemented!()
        }

        fn cycle_initial<'db>(
            _: &'db Self::DbView,
            _: Id,
            _: Self::Input<'db>,
        ) -> Self::Output<'db> {
            unimplemented!()
        }

        fn recover_from_cycle<'db>(
            _: &'db Self::DbView,
            _: &crate::Cycle,
            _: &Self::Output<'db>,
            value: Self::Output<'db>,
            _: Self::Input<'db>,
        ) -> Self::Output<'db> {
            value
        }

        fn serialize<S>(_: &Self::Output<'_>, _: S) -> Result<S::Ok, S::Error>
        where
            S: plumbing::serde::Serializer,
        {
            unimplemented!()
        }

        fn deserialize<'de, D>(_: D) -> Result<Self::Output<'static>, D::Error>
        where
            D: plumbing::serde::Deserializer<'de>,
        {
            unimplemented!()
        }
    }
}
