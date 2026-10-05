use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::panic::AssertUnwindSafe;

use salsa::Database as _;

use super::*;
use crate::types::callable::scheduled_probe::member_lookup::LookupFailure;
use crate::types::callable::scheduled_probe::mro::{
    PreparedMroWork, PreparedStaticMroEffects, c3_work_units, merge_mro_sequences,
};
use crate::types::mro::Mro;
use crate::types::mro::c3::{C3Work, InlineC3Effects, SynchronousC3Effects, c3_merge_sync, sealed};
use crate::types::mro::construction::{StaticMroEffects, StaticMroWork};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::tuple::TupleType;
use crate::types::{DynamicType, GenericAlias, MaterializationKind, TypingModule};

type Answer<'db> = Result<Option<Mro<'db>>, LookupFailure<'db>>;

fn run_merge<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    sequences: &[VecDeque<ClassBase<'db>>],
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Answer<'db>> {
    let router = Router::default();
    let before = sequences.to_vec();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            merge_mro_sequences(db, router, sequences)
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "source reads: {:?}",
        observed.reads
    );
    assert_eq!(sequences, before);
    assert!(!router.consumer_active.get());
    assert!(observed.value.graph.pending.is_empty());
    assert!(observed.value.graph.mapping_pending.is_empty());
    observed.value
}

#[test]
fn prepared_c3_returns_complete_success_or_inconsistency() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let [a, b, root] = [ClassBase::Any, ClassBase::Protocol, ClassBase::Generic];
    for (sequences, expected) in [
        (vec![], Some(Mro::from([]))),
        (vec![VecDeque::new(), VecDeque::new()], Some(Mro::from([]))),
        (
            vec![
                VecDeque::from([a, root]),
                VecDeque::new(),
                VecDeque::from([b, root]),
                VecDeque::from([a, b]),
            ],
            Some(Mro::from([a, b, root])),
        ),
        (
            vec![VecDeque::from([root, a, b]), VecDeque::from([root, b, a])],
            None,
        ),
    ] {
        let full = run_merge(&db, &env, &sequences, 100_000, (false, false));
        assert_eq!(full.consumer, Some(Ok(expected)));
        assert_eq!(
            full.consumer,
            Some(Ok(c3_merge_sync(&db, sequences, &InlineC3Effects).unwrap()))
        );
    }
    Ok(())
}

#[test]
fn prepared_c3_preserves_winning_payloads_and_distinct_any_identities() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/c3.py", "class Root[T]:\n    value: T\n")
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/c3.py")?,
        env.program(&db),
    );
    let root = explicit_global_symbol(&db, file, "Root")
        .place
        .expect_type()
        .as_class_literal()
        .unwrap()
        .as_static()
        .unwrap();
    let context = root.generic_context(&db).unwrap();
    let first = context
        .specialize(&db, &[Type::any()])
        .with_materialization_kind(&db, Some(MaterializationKind::Top));
    let second = context.specialize(&db, &[KnownClass::Int.to_instance(&db, &env)]);
    let first = ClassBase::Class(ClassType::Generic(GenericAlias::new(&db, root, first)));
    let second = ClassBase::Class(ClassType::Generic(GenericAlias::new(&db, root, second)));
    let tuple = ClassBase::Class(
        TupleType::heterogeneous(&db, &env, [Type::any(), Type::unknown()]).to_class_type(&db),
    );
    let other_tuple = ClassBase::Class(
        TupleType::heterogeneous(&db, &env, [KnownClass::Int.to_instance(&db, &env)])
            .to_class_type(&db),
    );
    let object = ClassBase::object(&db, &env);

    for (first, second) in [
        (first, second),
        (tuple, other_tuple),
        (
            ClassBase::TypedDict(TypingModule::Typing),
            ClassBase::TypedDict(TypingModule::TypingExtensions),
        ),
    ] {
        for (winner, other) in [(first, second), (second, first)] {
            let sequences = [
                VecDeque::from([winner, object]),
                VecDeque::from([other, object]),
            ];
            let full = run_merge(&db, &env, &sequences, 100_000, (false, false));
            assert_eq!(full.consumer, Some(Ok(Some(Mro::from([winner, object])))));
            assert_eq!(
                full.consumer,
                Some(Ok(
                    c3_merge_sync(&db, sequences.to_vec(), &InlineC3Effects).unwrap()
                ))
            );
        }
    }
    let sequences = [VecDeque::from([
        ClassBase::Any,
        ClassBase::Dynamic(DynamicType::Any),
    ])];
    let full = run_merge(&db, &env, &sequences, 100_000, (false, false));
    assert_eq!(
        full.consumer,
        Some(Ok(Some(Mro::from(
            sequences[0].iter().copied().collect::<Vec<_>>()
        ))))
    );
    Ok(())
}

#[test]
fn prepared_c3_copies_wrapped_inputs_in_logical_order_and_grows_output() -> anyhow::Result<()> {
    let initial_capacity = Vec::<ClassBase<'_>>::with_capacity(8).capacity();
    let count = initial_capacity + 4;
    let source = (0..count)
        .map(|index| format!("class C{index}: ...\n"))
        .collect::<String>();
    let db = TestDbBuilder::new()
        .with_file("/src/c3.py", &source)
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/c3.py")?,
        env.program(&db),
    );
    let mut sequence = VecDeque::with_capacity(2 * count);
    for index in 0..count {
        let literal = explicit_global_symbol(&db, file, &format!("C{index}"))
            .place
            .expect_type()
            .as_class_literal()
            .unwrap();
        sequence.push_back(ClassBase::Class(ClassType::NonGeneric(literal)));
    }
    for _ in 0..sequence.capacity() - 1 {
        let head = sequence.pop_front().unwrap();
        sequence.push_back(head);
    }
    assert!(sequence.capacity() > sequence.len());
    assert!(!sequence.as_slices().1.is_empty());
    let expected = Mro::from(sequence.iter().copied().collect::<Vec<_>>());
    let sequences = [sequence];
    let full = run_merge(&db, &env, &sequences, 100_000, (false, false));
    assert_eq!(full.consumer, Some(Ok(Some(expected))));
    let trace = Trace::default();
    assert_eq!(
        full.consumer,
        Some(Ok(c3_merge_sync(&db, sequences.to_vec(), &trace).unwrap()))
    );
    let mut spare_capacity_appends = 0;
    let mut growth_appends = 0;
    for work in trace.0.into_inner() {
        if let C3Work::OutputAppend {
            prefix_len,
            capacity,
        } = work
        {
            if prefix_len < capacity {
                assert_eq!(c3_work_units(work), Ok(8));
                spare_capacity_appends += 1;
            } else {
                assert_eq!(prefix_len, capacity);
                assert_eq!(c3_work_units(work), Ok(16 * (prefix_len + 1) + 8));
                growth_appends += 1;
            }
        }
    }
    assert!(spare_capacity_appends >= initial_capacity);
    assert!(growth_appends > 0);
    Ok(())
}

#[derive(Default)]
struct Trace(RefCell<Vec<C3Work>>);

impl sealed::Sealed for Trace {}

impl SynchronousC3Effects for Trace {
    type Error = Infallible;

    fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error> {
        self.0.borrow_mut().push(work);
        Ok(())
    }

    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(fields.mro_identity(base))
    }
}

fn interruption_inputs<'db>() -> [VecDeque<ClassBase<'db>>; 2] {
    [
        VecDeque::from([ClassBase::Any, ClassBase::Protocol]),
        VecDeque::from([ClassBase::Any, ClassBase::Generic]),
    ]
}

#[test]
fn prepared_c3_interruptions_preserve_borrowed_input_and_allow_fresh_retries() -> anyhow::Result<()>
{
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let sequences = interruption_inputs();
    let full = run_merge(&db, &env, &sequences, 100_000, (false, false));
    let trace = Trace::default();
    assert_eq!(
        full.consumer,
        Some(Ok(c3_merge_sync(&db, sequences.to_vec(), &trace).unwrap()))
    );
    let trace = trace.0.into_inner();
    let intake_count = 3 * sequences.len() + 2;
    let first_removal = trace
        .iter()
        .position(|work| matches!(work, C3Work::RemoveHead { .. }))
        .unwrap();
    let publication = trace
        .iter()
        .position(|work| matches!(work, C3Work::Publish))
        .unwrap();
    let stages = [
        ("input copy", 2),
        ("input append", 3),
        ("output allocation", intake_count),
        ("head removal", intake_count + first_removal),
        ("publication", intake_count + publication),
    ];
    let mut cuts = [None; 5];
    for budget in std::iter::once(0).chain(
        full.graph
            .boundaries
            .iter()
            .copied()
            .filter(|budget| *budget < full.work()),
    ) {
        let short = run_merge(&db, &env, &sequences, budget, (false, false));
        assert!(short.consumer.is_none());
        assert!(short.graph.exhausted);
        for (cut, (_, sequence)) in cuts.iter_mut().zip(stages) {
            // Admission and the next consumer poll are separate scheduler rounds. Cancelling
            // here drops the admitted state before the operation resumes.
            if short.consumer_polls == sequence + 1
                && short.semantic_work_polls.len() == sequence + 1
            {
                *cut = Some(budget);
            }
        }
    }
    for order in [(false, false), (true, false), (false, true), (true, true)] {
        let retry = run_merge(&db, &env, &sequences, full.work(), order);
        assert_eq!(retry.consumer, full.consumer);
        assert_eq!(retry.work(), full.work());
    }
    for ((name, _), cut) in stages.into_iter().zip(cuts) {
        let cut = cut.unwrap_or_else(|| panic!("missing cancellation boundary: {name}"));
        let db = TestDbBuilder::new().build()?;
        let env = db.program_environment();
        let sequences = interruption_inputs();
        let router = Router::default();
        router.cancel_at(cut, db.cancellation_token());
        let published = Cell::new(false);
        let before = sequences.clone();
        let observed = probe::capture(&db, || {
            salsa::Cancelled::catch(AssertUnwindSafe(|| {
                run_with(&db, &env, &router, 100_000, false, false, |router| async {
                    let result = merge_mro_sequences(&db, router, &sequences).await;
                    published.set(true);
                    result
                })
            }))
        })
        .unwrap();
        assert!(
            observed.reads.is_empty(),
            "source reads: {:?}",
            observed.reads
        );
        assert!(
            matches!(observed.value, Err(salsa::Cancelled::Local)),
            "{name}"
        );
        assert!(!published.get(), "{name}");
        assert!(!router.consumer_active.get(), "{name}");
        assert_eq!(sequences, before);
    }
    Ok(())
}

#[test]
fn constructor_c3_provider_uses_already_admitted_queues() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let router = Router::default();
    let observed = probe::capture(&db, || {
        run_with(&db, &env, &router, 100_000, false, false, |router| async {
            let work = PreparedMroWork::consumer(router);
            let effects = PreparedStaticMroEffects::new(&db, &env, &work);
            effects
                .checkpoint(StaticMroWork::DirectSequenceCapacity { len: 2 })
                .await?;
            let sequence = VecDeque::from([ClassBase::Any, ClassBase::Protocol]);
            effects
                .checkpoint(StaticMroWork::SequenceAppend { prefix_len: 0, capacity: 0 })
                .await?;
            let sequences = Vec::from([sequence]);
            effects.checkpoint(StaticMroWork::C3Request).await?;
            effects.c3_merge(sequences).await
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "source reads: {:?}",
        observed.reads
    );
    assert_eq!(
        observed.value.consumer,
        Some(Ok(Some(Mro::from([ClassBase::Any, ClassBase::Protocol]))))
    );
    let trace = Trace::default();
    c3_merge_sync(
        &db,
        vec![VecDeque::from([ClassBase::Any, ClassBase::Protocol])],
        &trace,
    )
    .unwrap();
    assert_eq!(
        observed.value.semantic_work_polls.len(),
        trace.0.borrow().len() + 3
    );
    Ok(())
}

#[test]
fn prepared_c3_costs_reject_length_and_comparison_overflow() {
    for work in [
        C3Work::OutputCapacity {
            entries: usize::MAX,
        },
        C3Work::RetainSequences { len: usize::MAX },
        C3Work::OutputAppend {
            prefix_len: usize::MAX,
            capacity: usize::MAX,
        },
        C3Work::OutputAppend {
            prefix_len: usize::MAX / 16,
            capacity: usize::MAX / 16,
        },
        C3Work::BoxOutput {
            len: usize::MAX,
            capacity: usize::MAX,
        },
        C3Work::IdentityComparison {
            todo_bytes: usize::MAX,
        },
        C3Work::RemoveHead {
            todo_bytes: usize::MAX,
        },
    ] {
        assert_eq!(c3_work_units(work), Err(Boundary::CostOverflow));
    }
    assert_eq!(
        c3_work_units(C3Work::IdentityComparison { todo_bytes: 123 }),
        Ok(131)
    );
    assert_eq!(
        c3_work_units(C3Work::RemoveHead { todo_bytes: 123 }),
        Ok(131)
    );
    assert_eq!(
        c3_work_units(C3Work::OutputAppend {
            prefix_len: usize::MAX - 1,
            capacity: usize::MAX,
        }),
        Ok(8)
    );
}
