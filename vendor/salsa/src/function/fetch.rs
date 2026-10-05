#[cfg(test)]
use crate::DatabaseKeyIndex;
use crate::Id;
#[cfg(feature = "accumulator")]
use crate::accumulator::accumulated_map::InputAccumulatedValues;
use crate::attempt_probe::AttemptSupport;
use crate::attempt_probe::MemoReuse;
use crate::function::eviction::EvictionPolicy;
use crate::function::execute::execution_run::explicit_reads::assert_ordinary_execution_allowed;
use crate::function::fetch::selection::Refresh;
use crate::function::maybe_changed_after::ShallowUpdate;
use crate::function::memo::{Memo, MemoOutputCheck, MemoVerification, SelectedMemo};
use crate::function::{Configuration, IngredientImpl};
use crate::zalsa::{MemoIngredientIndex, Zalsa};
use crate::zalsa_local::ZalsaLocal;
use crate::zalsa_local::dependency_read::DependencyRead;

pub(super) mod selection;

pub(super) struct MemoDependencyRead<'db> {
    pub(super) read: DependencyRead<'db>,
    support: Option<&'db AttemptSupport>,
    incomplete: bool,
}

pub(super) enum FetchProbe<'db, C: Configuration> {
    Miss,
    Ready(SelectedMemo<'db, C>),
    Verify(EagerFetchVerification<'db, C>),
}

pub(super) struct EagerFetchVerification<'db, C: Configuration> {
    selected: SelectedMemo<'db, C>,
    verification: MemoVerification<'db>,
}

impl<'db, C: Configuration> EagerFetchVerification<'db, C> {
    #[cfg(test)]
    pub(super) fn key(&self) -> DatabaseKeyIndex {
        self.verification.key()
    }

    pub(super) fn output_check_work(&self, check: MemoOutputCheck) -> usize {
        self.verification.output_check_work(check)
    }

    pub(super) fn outputs_are_empty(&self) -> bool {
        self.verification.outputs_are_empty()
    }

    pub(super) fn controlled_outputs_are_empty(&self) -> bool {
        self.verification.controlled_outputs_are_empty()
    }

    pub(super) fn event(&self) {
        self.verification.event();
    }

    pub(super) fn finish(self) -> SelectedMemo<'db, C> {
        self.verification.publish_shallow();
        self.selected
    }
}

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    #[inline]
    pub fn fetch<'db>(
        &'db self,
        db: &'db C::DbView,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        id: Id,
    ) -> &'db C::Output<'db> {
        assert_ordinary_execution_allowed((zalsa, zalsa_local).into(), "query fetch");
        let _operation = crate::attempt_probe::enter(zalsa, C::ATTEMPT_POLICY, C::DEBUG_NAME);
        zalsa.unwind_if_revision_cancelled(zalsa_local);

        #[cfg(feature = "detailed-trace")]
        let _span =
            crate::tracing::debug_span!("fetch", query = ?self.database_key_index(id)).entered();

        let selected = self.refresh_memo_inner(db, zalsa, zalsa_local, id);
        self.record_memo_read(zalsa, zalsa_local, id, &selected)
    }

    /// Records the exact selected memo while the caller's query operation is still installed.
    /// Selection/validation happens before this helper, including any claim-transfer refetch.
    #[inline]
    pub(super) fn record_memo_read<'db>(
        &self,
        zalsa: &'db Zalsa,
        zalsa_local: &ZalsaLocal,
        id: Id,
        selected: &SelectedMemo<'db, C>,
    ) -> &'db C::Output<'db> {
        self.record_memo_use(zalsa_local, id, selected);
        self.record_memo_dependencies(zalsa, zalsa_local, id, selected)
    }

    #[inline]
    pub(super) fn record_memo_use(
        &self,
        _zalsa_local: &ZalsaLocal,
        id: Id,
        _selected: &SelectedMemo<'_, C>,
    ) {
        self.eviction.record_use(id);
        #[cfg(test)]
        completion_observation::record(
            completion_observation::Stage::Eviction,
            _zalsa_local,
            self.database_key_index(id),
            std::ptr::from_ref(_selected.memo()).addr(),
        );
    }

    #[inline]
    pub(super) fn record_memo_dependencies<'db>(
        &self,
        zalsa: &'db Zalsa,
        zalsa_local: &ZalsaLocal,
        id: Id,
        selected: &SelectedMemo<'db, C>,
    ) -> &'db C::Output<'db> {
        self.record_memo_dependencies_with_read(zalsa, zalsa_local, id, selected, None)
    }

    #[inline]
    fn memo_attempt_read<'db>(
        zalsa: &Zalsa,
        selected: &SelectedMemo<'db, C>,
    ) -> (Option<&'db AttemptSupport>, bool) {
        let memo = selected.memo();
        let provisional = memo.header.may_be_provisional();
        let support = memo.header.revisions.attempt_support();
        let incomplete = support
            .is_some_and(|support| support.reuse(zalsa, provisional) == MemoReuse::Incomplete);
        (support.filter(|_| incomplete || provisional), incomplete)
    }

    pub(super) fn prepare_memo_dependency_read<'db>(
        &self,
        zalsa: &Zalsa,
        id: Id,
        selected: &SelectedMemo<'db, C>,
    ) -> MemoDependencyRead<'db> {
        let memo = selected.memo();
        let revisions = &memo.header.revisions;
        let (support, incomplete) = Self::memo_attempt_read(zalsa, selected);
        MemoDependencyRead {
            read: DependencyRead::Full {
                input: self.database_key_index(id),
                durability: revisions.durability,
                changed_at: revisions.changed_at,
                cycle_heads: if incomplete {
                    crate::cycle::empty_cycle_heads()
                } else {
                    memo.header.cycle_heads()
                },
                #[cfg(feature = "accumulator")]
                accumulated_inputs: if revisions.accumulated().is_some() {
                    InputAccumulatedValues::Any
                } else {
                    revisions.accumulated_inputs.load()
                },
            },
            support,
            incomplete,
        }
    }

    /// A supplied read has already reserved its recipient's storage. No callouts may intervene
    /// between that reservation and the canonical metadata updates below.
    #[inline]
    pub(super) fn record_memo_dependencies_with_read<'db>(
        &self,
        zalsa: &'db Zalsa,
        zalsa_local: &ZalsaLocal,
        id: Id,
        selected: &SelectedMemo<'db, C>,
        prepared: Option<MemoDependencyRead<'db>>,
    ) -> &'db C::Output<'db> {
        let database_key_index = self.database_key_index(id);
        let memo = selected.memo();
        let memo_value = selected.value();
        let revisions = &memo.header.revisions;
        let (support, incomplete) = prepared.as_ref().map_or_else(
            || Self::memo_attempt_read(zalsa, selected),
            |prepared| (prepared.support, prepared.incomplete),
        );
        if let Some(support) = support {
            zalsa_local.report_attempt_read(zalsa, support, incomplete);
            #[cfg(test)]
            completion_observation::record(
                completion_observation::Stage::Support,
                zalsa_local,
                database_key_index,
                std::ptr::from_ref(memo).addr(),
            );
        }
        if let Some(prepared) = prepared {
            zalsa_local.apply_dependency_read(&prepared.read);
        } else {
            zalsa_local.report_tracked_read(
                database_key_index,
                revisions.durability,
                revisions.changed_at,
                if incomplete {
                    crate::cycle::empty_cycle_heads()
                } else {
                    memo.header.cycle_heads()
                },
                #[cfg(feature = "accumulator")]
                revisions.accumulated().is_some(),
                #[cfg(feature = "accumulator")]
                &revisions.accumulated_inputs,
            );
        }

        #[cfg(test)]
        completion_observation::record(
            completion_observation::Stage::TrackedRead,
            zalsa_local,
            database_key_index,
            std::ptr::from_ref(memo).addr(),
        );

        crate::prepared_source_probe::observe(
            zalsa,
            zalsa_local,
            database_key_index,
            std::ptr::from_ref(memo).addr(),
            || {
                if incomplete {
                    crate::prepared_source_probe::Status::Incomplete
                } else if memo.header.may_be_provisional() {
                    crate::prepared_source_probe::Status::Provisional
                } else {
                    crate::prepared_source_probe::Status::Final
                }
            },
        );
        #[cfg(test)]
        completion_observation::record(
            completion_observation::Stage::PreparedSource,
            zalsa_local,
            database_key_index,
            std::ptr::from_ref(memo).addr(),
        );

        memo_value
    }

    #[inline(always)]
    pub(super) fn refresh_memo<'db>(
        &'db self,
        db: &'db C::DbView,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        id: Id,
    ) -> &'db Memo<C> {
        let _operation = crate::attempt_probe::enter(zalsa, C::ATTEMPT_POLICY, C::DEBUG_NAME);
        let memo_ingredient_index = self.memo_ingredient_index(zalsa, id);
        if let Some(selected) = self.fetch_hot(zalsa, id, memo_ingredient_index) {
            return selected.memo();
        }
        Refresh::execute_cold(
            self,
            db,
            zalsa,
            zalsa_local,
            id,
            memo_ingredient_index,
            true,
        )
        .memo()
    }

    #[inline(always)]
    fn refresh_memo_inner<'db>(
        &'db self,
        db: &'db C::DbView,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        id: Id,
    ) -> SelectedMemo<'db, C> {
        let memo_ingredient_index = self.memo_ingredient_index(zalsa, id);

        // Keep the hot and cold probes in distinct control-flow blocks. Using `or_else`
        // here can outline both into one function, making hot hits pay for the cold path's
        // stack frame.
        if let Some(selected) = self.fetch_hot(zalsa, id, memo_ingredient_index) {
            return selected;
        }

        Refresh::execute_cold(
            self,
            db,
            zalsa,
            zalsa_local,
            id,
            memo_ingredient_index,
            false,
        )
    }

    #[inline(always)]
    fn fetch_hot<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Option<SelectedMemo<'db, C>> {
        match self.fetch_probe(zalsa, id, memo_ingredient_index) {
            FetchProbe::Miss => None,
            FetchProbe::Ready(selected) => Some(selected),
            FetchProbe::Verify(verification) => {
                verification.event();
                Some(verification.finish())
            }
        }
    }

    #[inline(always)]
    fn fetch_probe<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> FetchProbe<'db, C> {
        let Some(memo) = self.get_memo_from_table_for(zalsa, id, memo_ingredient_index) else {
            return FetchProbe::Miss;
        };

        let Some(selected) = SelectedMemo::new(memo) else {
            return FetchProbe::Miss;
        };
        match memo.header.attempt_reuse(zalsa) {
            MemoReuse::Incomplete if memo.header.is_finality_candidate() => {
                return FetchProbe::Miss;
            }
            MemoReuse::Incomplete => return FetchProbe::Ready(selected),
            MemoReuse::Stale => return FetchProbe::Miss,
            MemoReuse::Ordinary => {}
        }

        let database_key_index = self.database_key_index(id);

        let can_shallow_update = memo.header.shallow_verify_memo(
            zalsa,
            database_key_index,
            #[cfg(feature = "detailed-trace")]
            true,
        );

        if can_shallow_update.yes() && !memo.header.may_be_provisional() {
            // The memo is present in memo_map and we have verified that it is
            // still valid for the current revision.
            match can_shallow_update {
                ShallowUpdate::HigherDurability => FetchProbe::Verify(EagerFetchVerification {
                    selected,
                    verification: MemoVerification::new(
                        &memo.header,
                        zalsa,
                        database_key_index,
                        zalsa.current_revision(),
                    ),
                }),
                ShallowUpdate::Verified => FetchProbe::Ready(selected),
                ShallowUpdate::No => FetchProbe::Miss,
            }
        } else {
            FetchProbe::Miss
        }
    }
}

#[cfg(test)]
pub(in crate::function) mod completion_observation {
    use std::cell::RefCell;

    use crate::DatabaseKeyIndex;
    use crate::attempt_probe::{self, QueryPolicy};
    use crate::zalsa_local::{QueryEdgeKind, ZalsaLocal};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::function) enum Stage {
        Eviction,
        Support,
        TrackedRead,
        PreparedSource,
        Converted,
    }

    #[derive(Debug)]
    pub(in crate::function) struct Event {
        pub stage: Stage,
        pub key: DatabaseKeyIndex,
        pub memo_address: usize,
        pub caller: Option<DatabaseKeyIndex>,
        pub depths: (usize, usize),
        pub policy: QueryPolicy,
        pub inputs: Vec<DatabaseKeyIndex>,
        pub has_support: bool,
    }

    thread_local! {
        static EVENTS: RefCell<Option<Vec<Event>>> = const { RefCell::new(None) };
    }

    pub(in crate::function) fn record(
        stage: Stage,
        local: &ZalsaLocal,
        key: DatabaseKeyIndex,
        memo_address: usize,
    ) {
        if !EVENTS.with_borrow(|events| events.is_some()) {
            return;
        }
        let Some((caller, inputs, has_support)) = local.try_with_query_stack(|stack| {
            stack.last().map_or((None, Vec::new(), false), |frame| {
                let (edges, support) = frame.completion_state();
                (
                    Some(frame.database_key_index),
                    edges
                        .iter()
                        .filter_map(|edge| {
                            (edge.kind() == QueryEdgeKind::Input).then_some(edge.key())
                        })
                        .collect(),
                    support,
                )
            })
        }) else {
            return;
        };
        let event = Event {
            stage,
            key,
            memo_address,
            caller,
            depths: attempt_probe::stack_depths(),
            policy: attempt_probe::current_policy(),
            inputs,
            has_support,
        };
        EVENTS.with_borrow_mut(|events| {
            if let Some(events) = events {
                events.push(event);
            }
        });
    }

    pub(in crate::function) fn collect<T>(body: impl FnOnce() -> T) -> (T, Vec<Event>) {
        EVENTS.with_borrow_mut(|events| assert!(events.replace(Vec::new()).is_none()));
        let reset = Reset;
        let result = body();
        let events = EVENTS.with_borrow_mut(|events| events.take().unwrap());
        drop(reset);
        (result, events)
    }

    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            EVENTS.with_borrow_mut(|events| *events = None);
        }
    }
}
