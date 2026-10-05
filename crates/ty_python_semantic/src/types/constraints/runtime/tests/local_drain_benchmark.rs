use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::fmt;
use std::hint::black_box;
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::time::Instant;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RegistryBuilder, RunResult};

use super::{RuntimeStructural, assert_same, inputs};
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::control::{TddControl, TddWork};
use crate::types::constraints::{
    ConstraintCombination, ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::constructor::expansion_probe;
use crate::types::{BoundTypeVarInstance, TypeVarVariance};

#[derive(Clone, Copy, Debug)]
enum Operation {
    Combine,
    Fold,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    task_count: usize,
    task_bytes: usize,
    resource_count: usize,
    resource_bytes: usize,
    work_count: usize,
    work_units: usize,
    polls: usize,
}

impl Counts {
    fn record(&mut self, work: ExecutionWork) {
        match work {
            ExecutionWork::Task { requested_bytes } => {
                self.task_count += 1;
                self.task_bytes += requested_bytes;
            }
            ExecutionWork::Resource { requested_bytes } => {
                self.resource_count += 1;
                self.resource_bytes += requested_bytes;
            }
            ExecutionWork::Work { units } => {
                self.work_count += 1;
                self.work_units += units;
            }
            ExecutionWork::Poll => self.polls += 1,
        }
    }
}

struct Admission {
    counts: Cell<Counts>,
    trace: Option<RefCell<Vec<ExecutionWork>>>,
}

impl Admission {
    fn new(trace: bool) -> Self {
        Self {
            counts: Cell::new(Counts::default()),
            trace: trace.then(RefCell::default),
        }
    }
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let mut counts = self.counts.get();
        counts.record(work);
        self.counts.set(counts);
        if let Some(trace) = &self.trace {
            trace.borrow_mut().push(work);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Trace {
    expected: Vec<ExecutionWork>,
    semantic: Counts,
    advances: usize,
    cursors: usize,
    outer_continues: usize,
    support_batches: usize,
    max_support_batch: usize,
}

impl TddControl for Trace {
    type Error = Infallible;

    fn admit(&mut self, work: TddWork) -> Result<(), Infallible> {
        if let TddWork::SupportWords { words } = work {
            self.support_batches += 1;
            self.max_support_batch = self.max_support_batch.max(words);
        }
        let weighted = ExecutionWork::Work {
            units: work.work_units(),
        };
        self.semantic.record(weighted);
        self.expected.push(weighted);
        if work.requested_payload_bytes() != 0 {
            let resource = ExecutionWork::Resource {
                requested_bytes: work.requested_payload_bytes(),
            };
            self.semantic.record(resource);
            self.expected.push(resource);
        }
        Ok(())
    }
}

fn drain<T>(trace: &mut Trace, mut advance: impl FnMut(&mut Trace) -> ControlFlow<T>) -> T {
    trace.cursors += 1;
    loop {
        trace.advances += 1;
        match advance(trace) {
            ControlFlow::Break(result) => return result,
            ControlFlow::Continue(()) => {
                trace.outer_continues += 1;
            }
        }
    }
}

fn direct<'db, 'c>(
    builder: &'c ConstraintSetBuilder<'db>,
    values: [ConstraintSet<'db, 'c>; 7],
    kind: ConstraintFoldKind,
    operation: Operation,
    trace: &mut Trace,
) -> ConstraintSet<'db, 'c> {
    match operation {
        Operation::Combine => {
            let mut cursor = ConstraintCombination::new(builder, kind, values[0], values[1]);
            drain(trace, |trace| {
                cursor.advance_with(trace).expect("finite combination")
            })
        }
        Operation::Fold => {
            let mut fold = ConstraintFold::new(builder, kind);
            for next in values {
                let mut cursor = fold.begin_push(next);
                let result = drain(trace, |trace| {
                    cursor.advance_with(trace).expect("finite push")
                });
                assert!(
                    result.is_continue(),
                    "distinct variables do not absorb the fold"
                );
            }
            let mut cursor = fold.begin_finish();
            drain(trace, |trace| {
                cursor.advance_with(trace).expect("finite finish")
            })
        }
    }
}

fn runtime<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    values: [ConstraintSet<'db, 'c>; 7],
    kind: ConstraintFoldKind,
    operation: Operation,
    admission: &Admission,
) -> ConstraintSet<'db, 'c> {
    expansion_probe::run(db, usize::MAX, || {
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(|endpoint| async move {
                let operations = RuntimeStructural::new(db, endpoint);
                match operation {
                    Operation::Combine => {
                        operations
                            .combine(builder, kind, values[0], values[1])
                            .await
                    }
                    Operation::Fold => {
                        let mut fold = ConstraintFold::new(builder, kind);
                        for next in values {
                            assert!(operations.push(&mut fold, next).await?.is_continue());
                        }
                        operations.finish(&mut fold).await
                    }
                }
            })
    })
    .0
    .expect("complete attempt")
    .expect("complete runtime operation")
}

fn ordinary<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    values: [ConstraintSet<'db, 'c>; 7],
    kind: ConstraintFoldKind,
    operation: Operation,
) -> ConstraintSet<'db, 'c> {
    match operation {
        Operation::Combine => {
            let mut left = values[0];
            match kind {
                ConstraintFoldKind::All => left.intersect(db, builder, values[1]),
                ConstraintFoldKind::Any => left.union(db, builder, values[1]),
            }
        }
        Operation::Fold => {
            let mut fold = ConstraintFold::new(builder, kind);
            for next in values {
                assert!(fold.push(next).is_continue());
            }
            fold.finish()
        }
    }
}

fn seeded<'db>(db: &'db TestDb, prefix: &[BoundTypeVarInstance<'db>]) -> ConstraintSetBuilder<'db> {
    let builder = ConstraintSetBuilder::new();
    for variable in prefix {
        builder.storage.borrow_mut().intern_typevar(db, *variable);
    }
    builder
}

fn capacities(builder: &ConstraintSetBuilder<'_>) -> [usize; 10] {
    let mut storage = builder.storage.borrow_mut();
    [
        storage.nodes.raw.capacity(),
        storage.supports.raw.capacity(),
        storage.node_supports.raw.capacity(),
        storage.source_orders.raw.capacity(),
        storage.node_cache.capacity(),
        storage.source_order_cache.capacity(),
        storage.and_cache.capacity(),
        storage.or_cache.capacity(),
        storage
            .supports
            .raw
            .iter_mut()
            .map(|support| support.words_mut().capacity())
            .sum(),
        storage
            .supports
            .iter()
            .map(|support| support.words().len())
            .max()
            .unwrap_or(0),
    ]
}

fn assert_storage(actual: &ConstraintSetBuilder<'_>, expected: &ConstraintSetBuilder<'_>) {
    let actual = actual.storage.borrow();
    let expected = expected.storage.borrow();
    assert_eq!(actual.constraints.raw, expected.constraints.raw);
    assert_eq!(actual.typevars.raw, expected.typevars.raw);
    assert_eq!(actual.nodes.raw, expected.nodes.raw);
    assert_eq!(actual.supports.raw, expected.supports.raw);
    assert_eq!(
        actual.constraint_supports.raw,
        expected.constraint_supports.raw
    );
    assert_eq!(actual.node_supports.raw, expected.node_supports.raw);
    assert_eq!(actual.source_orders.raw, expected.source_orders.raw);
    assert_eq!(actual.constraint_cache, expected.constraint_cache);
    assert_eq!(actual.typevar_cache, expected.typevar_cache);
    assert_eq!(actual.node_cache, expected.node_cache);
    assert_eq!(
        actual.constraint_bound_depth_cache,
        expected.constraint_bound_depth_cache
    );
    assert_eq!(actual.source_order_cache, expected.source_order_cache);
    assert_eq!(actual.never_satisfied_cache, expected.never_satisfied_cache);
    assert_eq!(actual.negate_cache, expected.negate_cache);
    assert_eq!(actual.or_cache, expected.or_cache);
    assert_eq!(actual.and_cache, expected.and_cache);
    assert_eq!(actual.exists_cache, expected.exists_cache);
}

fn cancellation_count(reader: &mut TestDb) -> usize {
    reader
        .take_salsa_events()
        .into_iter()
        .filter(|event| matches!(event.kind, salsa::EventKind::WillCheckCancellation))
        .count()
}

#[derive(Clone, Copy)]
struct Case {
    kind: ConstraintFoldKind,
    operation: Operation,
    warm: bool,
}

impl fmt::Debug for Case {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Case")
            .field(
                "kind",
                &match self.kind {
                    ConstraintFoldKind::All => "all",
                    ConstraintFoldKind::Any => "any",
                },
            )
            .field("operation", &self.operation)
            .field("warm", &self.warm)
            .finish()
    }
}

fn measure<'db>(
    output: &mut impl Write,
    db: &'db TestDb,
    reader: &mut TestDb,
    prefix: &[BoundTypeVarInstance<'db>],
    case: Case,
    samples: usize,
) -> io::Result<()> {
    let Case {
        kind,
        operation,
        warm,
    } = case;
    let reference = seeded(db, prefix);
    let reference_values = inputs(db, &reference);
    if warm {
        direct(
            &reference,
            reference_values,
            kind,
            operation,
            &mut Trace::default(),
        );
    }
    let start_capacities = capacities(&reference);
    let mut trace = Trace::default();
    trace.expected.push(ExecutionWork::Poll);
    let expected = direct(&reference, reference_values, kind, operation, &mut trace);
    let final_capacities = capacities(&reference);
    if !prefix.is_empty() {
        assert!(
            final_capacities[9] > 64,
            "the fixture contains real large supports"
        );
        if !warm {
            assert!(trace.support_batches > 1);
            assert_eq!(trace.max_support_batch, 64);
        }
    }

    let observed = seeded(db, prefix);
    let values = inputs(db, &observed);
    if warm {
        runtime(
            db,
            &observed,
            values,
            kind,
            operation,
            &Admission::new(false),
        );
    }
    assert_eq!(capacities(&observed), start_capacities);
    let admission = Admission::new(true);
    reader.clear_salsa_events();
    let result = runtime(db, &observed, values, kind, operation, &admission);
    let cancellations = cancellation_count(reader);
    assert!(std::ptr::eq(result.builder, &observed));
    assert_eq!(
        (result.node, result.source_order),
        (expected.node, expected.source_order)
    );
    assert_storage(&observed, &reference);
    assert_eq!(capacities(&observed), final_capacities);
    assert_same(result, ordinary(db, &observed, values, kind, operation));
    let events = admission.trace.as_ref().expect("trace enabled").borrow();
    let first_poll = events
        .iter()
        .position(|event| *event == ExecutionWork::Poll)
        .expect("root poll");
    let mut setup = Counts::default();
    for event in &events[..first_poll] {
        assert!(matches!(
            event,
            ExecutionWork::Task { .. } | ExecutionWork::Resource { .. }
        ));
        setup.record(*event);
    }
    assert_eq!(&events[first_poll..], trace.expected.as_slice());
    let totals = admission.counts.get();
    assert_eq!(setup.task_count, 1);
    assert_eq!(totals.task_count, 1);
    assert_eq!(totals.task_bytes, setup.task_bytes);
    assert_eq!(totals.polls, 1);
    assert_eq!(totals.work_count, trace.semantic.work_count);
    assert_eq!(totals.work_units, trace.semantic.work_units);
    assert_eq!(
        totals.resource_count,
        setup.resource_count + trace.semantic.resource_count
    );
    assert_eq!(
        totals.resource_bytes,
        setup.resource_bytes + trace.semantic.resource_bytes
    );
    assert_eq!(trace.advances, trace.outer_continues + trace.cursors);
    assert_eq!(
        trace.cursors,
        match operation {
            Operation::Combine => 1,
            Operation::Fold => 8,
        }
    );

    for runtime_path in [true, false] {
        let mut times = Vec::with_capacity(samples);
        let mut callbacks = None;
        for _ in 0..samples {
            let builder = seeded(db, prefix);
            let values = inputs(db, &builder);
            if warm {
                if runtime_path {
                    runtime(
                        db,
                        &builder,
                        values,
                        kind,
                        operation,
                        &Admission::new(false),
                    );
                } else {
                    ordinary(db, &builder, values, kind, operation);
                }
            }
            if runtime_path {
                assert_eq!(capacities(&builder), start_capacities);
            }
            let admission = Admission::new(false);
            reader.clear_salsa_events();
            let start = Instant::now();
            let result = black_box(if runtime_path {
                runtime(db, black_box(&builder), values, kind, operation, &admission)
            } else {
                ordinary(db, black_box(&builder), values, kind, operation)
            });
            times.push(start.elapsed().as_nanos());
            let sample_callbacks = cancellation_count(reader);
            if runtime_path {
                assert_eq!(admission.counts.get(), totals);
                assert_eq!(sample_callbacks, cancellations);
                assert_storage(&builder, &reference);
                assert_eq!(capacities(&builder), final_capacities);
            }
            if let Some(previous) = callbacks {
                assert_eq!(sample_callbacks, previous);
            }
            callbacks = Some(sample_callbacks);
            assert!(std::ptr::eq(result.builder, &builder));
            assert_eq!(
                (result.node, result.source_order),
                (expected.node, expected.source_order)
            );
        }
        let elapsed: u128 = times.iter().sum();
        times.sort_unstable();
        writeln!(
            output,
            "elapsed case={case:?} prefix={} path={} samples={samples} timer_pairs={samples} elapsed_ns={elapsed} median_ns={} cancellation_callbacks_per_sample={}",
            prefix.len(),
            if runtime_path {
                "runtime_local"
            } else {
                "ordinary"
            },
            times[samples / 2],
            callbacks.unwrap_or(0)
        )?;
    }
    writeln!(
        output,
        "accounting case={case:?} prefix={} structural_advances={} outer_continues={} local_actions_expected={} support_batches={} max_support_batch={} semantic={:?} checkpoint_work_count=0 checkpoint_work_units=0 setup={setup:?} total={totals:?} cancellation_callbacks={cancellations} start_capacities={start_capacities:?} final_capacities={final_capacities:?}",
        prefix.len(),
        trace.advances,
        trace.outer_continues,
        trace.advances + 2 * trace.cursors,
        trace.support_batches,
        trace.max_support_batch,
        trace.semantic,
    )
}

// Event vectors are drained outside every timing window. The default TestDb mutex/Vec observer
// and scalar admission counting remain inside runtime timings; these are instrumented costs.
#[test]
#[ignore = "opt-in local-drain measurement; prints timing and exact admission accounting"]
fn local_drain_measurement() -> io::Result<()> {
    let samples = std::env::var("TY_LOCAL_DRAIN_BENCH_SAMPLES").map_or(10, |value| {
        value
            .parse::<usize>()
            .expect("TY_LOCAL_DRAIN_BENCH_SAMPLES is an integer")
            .max(1)
    });
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "metadata fixture=local_drain_v1 arch={} os={} debug_assertions={} profile_label={:?} compiled_dev_opt={:?} compiled_test_opt={:?} observer=TestDb_mutex_vec_fresh_per_window local_boundary_observer=absent admission=scalar_counts_timed_full_trace_untimed capacity_columns=nodes,supports,node_supports,source_orders,node_cache,source_cache,and_cache,or_cache,support_word_capacity_sum,max_support_words",
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(debug_assertions),
        std::env::var("TY_LOCAL_DRAIN_BENCH_PROFILE").ok(),
        option_env!("CARGO_PROFILE_DEV_OPT_LEVEL"),
        option_env!("CARGO_PROFILE_TEST_OPT_LEVEL")
    )?;
    let db = setup_db();
    let mut reader = db.clone();
    let env = db.program_environment();
    let prefix: Vec<_> = (0..64 * usize::BITS as usize)
        .map(|index| {
            BoundTypeVarInstance::synthetic(
                &db,
                &env,
                Name::new(format!("Support{index}")),
                TypeVarVariance::Invariant,
            )
        })
        .collect();
    for prefix in [&[][..], prefix.as_slice()] {
        for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
            for operation in [Operation::Combine, Operation::Fold] {
                for warm in [false, true] {
                    measure(
                        &mut output,
                        &db,
                        &mut reader,
                        prefix,
                        Case {
                            kind,
                            operation,
                            warm,
                        },
                        samples,
                    )?;
                }
            }
        }
    }
    Ok(())
}
