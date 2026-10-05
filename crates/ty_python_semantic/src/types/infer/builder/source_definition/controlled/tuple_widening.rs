use std::convert::Infallible;
use std::slice;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::constraints::control::sequence_growth;
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::set_theoretic::widening::{
    AlternativeCursor, TupleElementCursor, TupleWideningEffects, TupleWideningFacts,
    widen_growing_tuples_with,
};
use crate::types::tuple::{TupleLength, TupleSpec, TupleType};
use crate::types::{RecursivelyDefined, Type, UnionBuilder, UnionType};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn widen_growing_tuples(
        &self,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| {
            widen_growing_tuples_with(previous, current, env, TupleWideningFacts, self)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleWideningEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn exact_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        TupleSpecEffects::exact_spec(self, tuple).await
    }

    async fn next_type(&self, types: &mut AlternativeCursor<'db>) -> RunResult<Option<Type<'db>>> {
        self.local(size_of::<Type<'db>>() + 3, 0, || types.next())
            .await
    }

    async fn push_length(
        &self,
        lengths: &mut Vec<TupleLength>,
        length: TupleLength,
    ) -> RunResult<()> {
        if lengths.len() == lengths.capacity() {
            let required = Self::checked(lengths.len().checked_add(1))?;
            let growth = sequence_growth::<TupleLength, Infallible>(lengths.capacity(), required)
                .map_err(|_| RunError::Contract("tuple length buffer growth overflow"))?;
            // Capacity work covers eventual disposal; tuple lengths have no owned children.
            let work = Self::checked(
                growth
                    .requested_payload_bytes
                    .checked_mul(2)
                    .and_then(|work| {
                        lengths
                            .len()
                            .checked_mul(size_of::<TupleLength>())?
                            .checked_add(work)
                    })
                    .and_then(|work| work.checked_add(4)),
            )?;
            #[cfg(test)]
            self.local(1, 0, || {
                observations::before_growth(growth.requested_payload_bytes);
            })
            .await?;
            self.local(work, growth.requested_payload_bytes, || {
                lengths.reserve_exact(growth.requested_capacity - lengths.len());
                #[cfg(test)]
                observations::after_growth();
            })
            .await?;
        }
        self.local(size_of::<TupleLength>() + 2, 0, || {
            lengths.push(length);
            #[cfg(test)]
            observations::length_pushed(self.db(), self.access.endpoint())?;
            Ok(())
        })
        .await?
    }

    async fn next_length(
        &self,
        lengths: &mut slice::Iter<'_, TupleLength>,
    ) -> RunResult<Option<TupleLength>> {
        self.local(size_of::<TupleLength>() + 2, 0, || lengths.next().copied())
            .await
    }

    async fn tuple_elements(
        &self,
        spec: &'db TupleSpec<'db>,
    ) -> RunResult<TupleElementCursor<'db>> {
        self.local(size_of::<TupleElementCursor<'db>>() * 2 + 1, 0, || {
            TupleElementCursor::new(self.db(), spec)
        })
        .await
    }

    async fn next_tuple_element(
        &self,
        elements: &mut TupleElementCursor<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(size_of::<Type<'db>>() + 4, 0, || elements.next())
            .await
    }

    async fn new_recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        self.local(size_of::<UnionBuilder<'db>>() * 2 + 1, 0, || {
            UnionBuilder::new(self.db(), env).cycle_recovery(true)
        })
        .await
    }

    async fn union_recursion(&self, union: UnionType<'db>) -> RunResult<RecursivelyDefined> {
        self.union_recursion_source(union).await
    }

    async fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> RunResult<()> {
        self.local(2, 0, || builder.merge_recursively_defined(recursion))
            .await
    }

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        PairUnionEffects::union_add(self, builder, ty).await
    }

    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        #[cfg(test)]
        self.local(1, 0, || observations::union_build(self.db()))
            .await?;
        PairUnionEffects::union_build(self, builder).await
    }

    async fn homogeneous_tuple(
        &self,
        env: &ProgramEnvironment<'db>,
        element: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        let spec = self
            .local(size_of::<TupleSpec<'db>>() * 2 + 1, 0, || {
                TupleSpec::homogeneous(element)
            })
            .await?;
        Ok(Type::tuple(self.access.intern_tuple(program, spec).await?))
    }
}

#[cfg(test)]
pub(in crate::types::infer) mod observations {
    use std::cell::Cell;

    use salsa::execution_probe::{ExecutionWork, RunResult, TaskEndpoint};

    use crate::Db;

    #[derive(Clone, Copy, Debug, Default)]
    pub(in crate::types::infer) struct Snapshot {
        pub(in crate::types::infer) growth_attempts: usize,
        pub(in crate::types::infer) growth_allocations: usize,
        pub(in crate::types::infer) first_growth_requested_bytes: Option<usize>,
        pub(in crate::types::infer) lengths_pushed: usize,
        pub(in crate::types::infer) first_length_remaining: Option<usize>,
        pub(in crate::types::infer) union_builds: usize,
        pub(in crate::types::infer) union_build_remaining: [Option<usize>; 2],
    }

    thread_local! {
        static SNAPSHOT: Cell<Snapshot> = const { Cell::new(Snapshot {
            growth_attempts: 0,
            growth_allocations: 0,
            first_growth_requested_bytes: None,
            lengths_pushed: 0,
            first_length_remaining: None,
            union_builds: 0,
            union_build_remaining: [None; 2],
        }) };
        static REFUSE_BYTES: Cell<bool> = const { Cell::new(false) };
    }

    pub(in crate::types::infer) fn reset(refuse_bytes: bool) {
        SNAPSHOT.set(Snapshot::default());
        REFUSE_BYTES.set(refuse_bytes);
    }

    pub(in crate::types::infer) fn snapshot() -> Snapshot {
        SNAPSHOT.get()
    }

    pub(super) fn before_growth(requested_bytes: usize) {
        let mut snapshot = SNAPSHOT.get();
        snapshot.growth_attempts += 1;
        if snapshot.growth_attempts == 1 {
            snapshot.first_growth_requested_bytes = Some(requested_bytes);
        }
        SNAPSHOT.set(snapshot);
    }

    pub(super) fn after_growth() {
        let mut snapshot = SNAPSHOT.get();
        snapshot.growth_allocations += 1;
        SNAPSHOT.set(snapshot);
    }

    pub(super) fn length_pushed(db: &dyn Db, endpoint: &TaskEndpoint<'_, '_>) -> RunResult<()> {
        let mut snapshot = SNAPSHOT.get();
        snapshot.lengths_pushed += 1;
        if snapshot.lengths_pushed == 1 {
            snapshot.first_length_remaining =
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        SNAPSHOT.set(snapshot);
        if snapshot.lengths_pushed == 1 && REFUSE_BYTES.get() {
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: usize::MAX,
            })?;
        }
        Ok(())
    }

    pub(super) fn union_build(db: &dyn Db) {
        let mut snapshot = SNAPSHOT.get();
        if let Some(slot) = snapshot
            .union_build_remaining
            .get_mut(snapshot.union_builds)
        {
            *slot = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        snapshot.union_builds += 1;
        SNAPSHOT.set(snapshot);
    }
}
