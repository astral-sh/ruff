use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{
    C3Effects, C3Occurrences, C3Work, SynchronousC3Effects, c3_merge_with, capture_c3_sync,
    comparison_label_bytes, sealed,
};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::mro::Mro;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::tuple::TupleType;
use crate::types::{
    ClassLiteral, ClassType, DivergentType, DynamicType, GenericAlias, KnownClass,
    MaterializationKind, Type, TypingModule, todo_type,
};

fn database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/c3.py",
        r#"
from enum import Enum
from typing import NamedTuple, TypedDict

class Base[T]:
    def get(self) -> T: ...
    def set(self, value: T) -> None: ...

Dynamic = type("Dynamic", (), {})
Named = NamedTuple("Named", [("value", int)])
Typed = TypedDict("Typed", {"value": int})
Enumeration = Enum("Enumeration", {"VALUE": 1})
"#,
    )?;
    Ok(db)
}

fn literal<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(db, system_path_to_file(db, "/src/c3.py")?, env.program(db));
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("expected a class literal for {name}"))
}

#[derive(Default)]
struct RecordingEffects {
    work: RefCell<Vec<C3Work>>,
    fail_at: Option<usize>,
    identities: Cell<usize>,
    fail_identity_at: Option<usize>,
    publications: Cell<usize>,
    capture: bool,
    coordinates: RefCell<Option<Vec<(usize, usize)>>>,
}

impl RecordingEffects {
    fn identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, usize> {
        let index = self.identities.get();
        self.identities.set(index + 1);
        if self.fail_identity_at == Some(index) {
            return Err(index);
        }
        Ok(fields.mro_identity(base))
    }

    fn record(&self, work: C3Work) -> Result<(), usize> {
        let index = self.work.borrow().len();
        self.work.borrow_mut().push(work);
        if self.fail_at == Some(index) {
            return Err(index);
        }
        if work == C3Work::Publish {
            self.publications.set(self.publications.get() + 1);
        }
        Ok(())
    }
}

impl sealed::Sealed for RecordingEffects {}

impl<'db> C3Effects<'db> for RecordingEffects {
    type Error = usize;

    async fn mro_identity(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.identity(fields, base)
    }

    async fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error> {
        self.record(work)
    }

    async fn start_occurrences(
        &self,
        sequence_count: usize,
    ) -> Result<Option<C3Occurrences>, Self::Error> {
        Ok(self.capture.then(|| C3Occurrences::new(sequence_count)))
    }

    async fn publish_occurrences(
        &self,
        occurrences: Option<C3Occurrences>,
    ) -> Result<(), Self::Error> {
        *self.coordinates.borrow_mut() = occurrences.map(|occurrences| occurrences.selected);
        Ok(())
    }
}

impl SynchronousC3Effects for RecordingEffects {
    type Error = usize;

    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.identity(fields, base)
    }

    fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error> {
        self.record(work)
    }
}

fn assert_merge<'db>(
    db: &'db TestDb,
    sequences: Vec<VecDeque<ClassBase<'db>>>,
    expected: Option<Mro<'db>>,
) -> Vec<C3Work> {
    let effects = RecordingEffects::default();
    assert_eq!(super::super::c3_merge(db, sequences.clone()), expected);
    assert_eq!(
        try_poll_immediate(c3_merge_with(
            crate::types::mro::field_reads::MroFieldReads::new(db),
            sequences,
            &effects
        )),
        Poll::Ready(Ok(expected)),
    );
    assert_eq!(effects.publications.get(), 1);
    effects.work.into_inner()
}

#[test]
fn empty_inputs_keep_allocation_boxing_and_publication_order() -> anyhow::Result<()> {
    let db = database()?;
    for sequences in [
        Vec::new(),
        Vec::from([VecDeque::new(), VecDeque::with_capacity(5)]),
    ] {
        let len = sequences.len();
        assert_eq!(
            assert_merge(&db, sequences, Some(Mro::from([]))),
            [
                C3Work::OutputCapacity { entries: 8 },
                C3Work::RetainSequences { len },
                C3Work::BoxOutput { len: 0, capacity: 8 },
                C3Work::Publish,
            ],
        );
    }
    Ok(())
}

#[test]
fn captured_sync_and_async_merges_keep_identical_occurrences() -> anyhow::Result<()> {
    let db = database()?;
    let object = ClassBase::object(&db, &db.program_environment());
    let sequences = vec![
        VecDeque::new(),
        VecDeque::from([ClassBase::Generic, object]),
        VecDeque::from([ClassBase::Generic, object]),
        VecDeque::from([object]),
    ];
    let synchronous = RecordingEffects::default();
    let Ok((mro, coordinates)) = capture_c3_sync(&db, sequences.clone(), &synchronous) else {
        anyhow::bail!("a merge with infallible checkpoints must finish");
    };
    let asynchronous = RecordingEffects {
        capture: true,
        ..RecordingEffects::default()
    };
    assert_eq!(
        try_poll_immediate(c3_merge_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            sequences,
            &asynchronous
        )),
        Poll::Ready(Ok(mro)),
    );
    assert_eq!(coordinates, [(1, 0), (1, 1)]);
    assert_eq!(asynchronous.coordinates.into_inner(), Some(coordinates));
    assert_eq!(
        asynchronous.work.into_inner(),
        synchronous.work.into_inner()
    );
    Ok(())
}

#[test]
fn captured_merges_discard_partial_coordinates_on_failure_or_interruption() -> anyhow::Result<()> {
    let db = database()?;
    for sequences in [
        vec![VecDeque::from([ClassBase::Any, ClassBase::Generic])],
        vec![
            VecDeque::from([ClassBase::Any]),
            VecDeque::from([ClassBase::Generic, ClassBase::Protocol]),
            VecDeque::from([ClassBase::Protocol, ClassBase::Generic]),
        ],
    ] {
        let complete = RecordingEffects {
            capture: true,
            ..RecordingEffects::default()
        };
        let Poll::Ready(Ok(result)) = try_poll_immediate(c3_merge_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            sequences.clone(),
            &complete,
        )) else {
            anyhow::bail!("a merge with infallible checkpoints must finish");
        };
        let work = complete.work.into_inner();
        assert!(work.contains(&C3Work::SelectedIdentity));
        if result.is_none() {
            assert!(complete.coordinates.into_inner().is_none());
            let Ok((result, coordinates)) =
                capture_c3_sync(&db, sequences.clone(), &RecordingEffects::default())
            else {
                anyhow::bail!("a merge with infallible checkpoints must finish");
            };
            assert!(result.is_none());
            assert!(coordinates.is_empty());
        }

        for fail_at in 0..work.len() {
            let effects = RecordingEffects {
                fail_at: Some(fail_at),
                capture: true,
                ..RecordingEffects::default()
            };
            assert_eq!(
                try_poll_immediate(c3_merge_with(
                    crate::types::mro::field_reads::MroFieldReads::new(&db),
                    sequences.clone(),
                    &effects
                )),
                Poll::Ready(Err(fail_at)),
            );
            assert!(effects.coordinates.into_inner().is_none());
            assert_eq!(effects.publications.get(), 0);

            let effects = RecordingEffects {
                fail_at: Some(fail_at),
                ..RecordingEffects::default()
            };
            assert_eq!(
                capture_c3_sync(&db, sequences.clone(), &effects),
                Err(fail_at),
            );
            assert_eq!(effects.publications.get(), 0);
        }
    }
    Ok(())
}

#[test]
fn singleton_trace_charges_terminal_advances_and_appends_before_removal() -> anyhow::Result<()> {
    let db = database()?;
    let capacity = Vec::<ClassBase<'_>>::with_capacity(8).capacity();
    assert_eq!(
        assert_merge(
            &db,
            Vec::from([VecDeque::from([ClassBase::Any])]),
            Some(Mro::from([ClassBase::Any])),
        ),
        [
            C3Work::OutputCapacity { entries: 8 },
            C3Work::RetainSequences { len: 1 },
            C3Work::CandidateAdvance,
            C3Work::TailSequenceAdvance,
            C3Work::TailEntryAdvance,
            C3Work::TailSequenceAdvance,
            C3Work::OutputAppend {
                prefix_len: 0,
                capacity
            },
            C3Work::SelectedIdentity,
            C3Work::RemovalSequenceAdvance,
            C3Work::RemoveHead { todo_bytes: 0 },
            C3Work::RemovalSequenceAdvance,
            C3Work::RetainSequences { len: 1 },
            C3Work::BoxOutput { len: 1, capacity },
            C3Work::Publish,
        ],
    );
    Ok(())
}

#[test]
fn typed_dict_identity_keeps_the_first_modules_payload() -> anyhow::Result<()> {
    let db = database()?;
    let typing = ClassBase::TypedDict(TypingModule::Typing);
    let extensions = ClassBase::TypedDict(TypingModule::TypingExtensions);
    for (first, second) in [(typing, extensions), (extensions, typing)] {
        let work = assert_merge(
            &db,
            Vec::from([
                VecDeque::new(),
                VecDeque::from([first]),
                VecDeque::new(),
                VecDeque::from([second]),
                VecDeque::new(),
            ]),
            Some(Mro::from([first])),
        );
        assert_eq!(
            work.iter()
                .filter(|work| matches!(work, C3Work::RetainSequences { .. }))
                .copied()
                .collect::<Vec<_>>(),
            [
                C3Work::RetainSequences { len: 5 },
                C3Work::RetainSequences { len: 2 },
            ],
        );
    }
    Ok(())
}

#[test]
fn generic_identity_preserves_selected_arguments_tuple_shape_and_materialization()
-> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let base = literal(&db, "Base")?
        .as_static()
        .ok_or_else(|| anyhow::anyhow!("Base must be static"))?;
    let context = base
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Base must be generic"))?;
    let top = context
        .specialize(&db, &[Type::unknown()])
        .with_materialization_kind(&db, Some(MaterializationKind::Top));
    let bottom = top.with_materialization_kind(&db, Some(MaterializationKind::Bottom));
    let tuple = KnownClass::Tuple
        .try_to_class_literal(&db, &env)
        .ok_or_else(|| anyhow::anyhow!("tuple must be available"))?;
    let tuple_context = tuple
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("tuple must be generic"))?;
    let first_tuple = tuple_context.specialize_tuple(
        &db,
        Type::unknown(),
        TupleType::homogeneous(&db, &env, Type::int_literal(1)),
    );
    let second_tuple = tuple_context.specialize_tuple(
        &db,
        Type::unknown(),
        TupleType::homogeneous(&db, &env, Type::int_literal(2)),
    );
    for (origin, first, second) in [
        (
            base,
            context.specialize(&db, &[Type::int_literal(1)]),
            context.specialize(&db, &[Type::int_literal(2)]),
        ),
        (base, top, bottom),
        (tuple, first_tuple, second_tuple),
    ] {
        assert_ne!(first, second);
        for (first, second) in [(first, second), (second, first)] {
            let first = ClassBase::Class(ClassType::Generic(GenericAlias::new(&db, origin, first)));
            let second =
                ClassBase::Class(ClassType::Generic(GenericAlias::new(&db, origin, second)));
            // The earlier alias cannot be selected until Generic leaves the other sequence's head.
            assert_merge(
                &db,
                Vec::from([
                    VecDeque::from([first]),
                    VecDeque::from([ClassBase::Generic, second]),
                ]),
                Some(Mro::from([ClassBase::Generic, first])),
            );
        }
    }
    Ok(())
}

#[test]
fn refused_identity_reads_discard_selected_prefixes_and_allow_retry() -> anyhow::Result<()> {
    let db = database()?;
    let base = literal(&db, "Base")?
        .as_static()
        .ok_or_else(|| anyhow::anyhow!("Base must be static"))?;
    let context = base
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Base must be generic"))?;
    let alias = ClassBase::Class(ClassType::Generic(GenericAlias::new(
        &db,
        base,
        context.specialize(&db, &[Type::int_literal(1)]),
    )));
    let sequences = vec![
        VecDeque::from([alias]),
        VecDeque::from([ClassBase::Generic, alias]),
    ];
    let expected = Some(Mro::from([ClassBase::Generic, alias]));
    let complete = RecordingEffects::default();
    assert_eq!(
        try_poll_immediate(c3_merge_with(
            MroFieldReads::new(&db),
            sequences.clone(),
            &complete
        )),
        Poll::Ready(Ok(expected.clone())),
    );
    let mut rejected_after_append = false;
    for index in 0..complete.identities.get() {
        let effects = RecordingEffects {
            fail_identity_at: Some(index),
            ..RecordingEffects::default()
        };
        assert_eq!(
            try_poll_immediate(c3_merge_with(
                MroFieldReads::new(&db),
                sequences.clone(),
                &effects
            )),
            Poll::Ready(Err(index)),
        );
        assert_eq!(effects.identities.get(), index + 1);
        assert_eq!(effects.publications.get(), 0);
        rejected_after_append |= effects
            .work
            .borrow()
            .iter()
            .any(|work| matches!(work, C3Work::OutputAppend { .. }));

        let retry = RecordingEffects::default();
        assert_eq!(
            capture_c3_sync(&db, sequences.clone(), &retry).map(|(mro, _)| mro),
            Ok(expected.clone()),
        );
        assert_eq!(retry.identities.get(), complete.identities.get());
        assert_eq!(retry.publications.get(), 1);
    }
    assert!(rejected_after_append);
    Ok(())
}

#[test]
fn distinct_dynamic_literal_and_divergent_identities_keep_their_values() -> anyhow::Result<()> {
    let db = database()?;
    let marker = DivergentType::new(salsa::plumbing::Id::from_bits(1));
    let base = literal(&db, "Base")?
        .as_static()
        .ok_or_else(|| anyhow::anyhow!("Base must be static"))?;
    let context = base
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Base must be generic"))?;
    let mut entries = Vec::from([
        ClassBase::Any,
        ClassBase::Dynamic(DynamicType::Any),
        ClassBase::Dynamic(DynamicType::Unknown),
        ClassBase::Dynamic(DynamicType::UnknownGeneric(context)),
        ClassBase::Dynamic(DynamicType::UnspecializedTypeVar),
        ClassBase::Dynamic(DynamicType::UnknownLambdaParameter),
        ClassBase::Dynamic(DynamicType::InvalidConcatenateUnknown),
        ClassBase::Dynamic(DynamicType::AmbiguousOverload),
        ClassBase::Divergent(marker),
        ClassBase::Divergent(marker.materialized(MaterializationKind::Top)),
        ClassBase::Divergent(marker.materialized(MaterializationKind::Bottom)),
    ]);
    for name in ["Dynamic", "Named", "Typed", "Enumeration"] {
        entries.push(ClassBase::Class(ClassType::NonGeneric(literal(&db, name)?)));
    }
    assert_merge(
        &db,
        entries.iter().map(|base| VecDeque::from([*base])).collect(),
        Some(Mro::from(entries)),
    );
    Ok(())
}

#[test]
fn wrapped_sequences_keep_logical_order_across_output_growth() -> anyhow::Result<()> {
    let db = database()?;
    let mut sequence = VecDeque::with_capacity(16);
    for id in 1..=16 {
        sequence.push_back(ClassBase::Divergent(DivergentType::new(
            salsa::plumbing::Id::from_bits(id),
        )));
    }
    for _ in 0..11 {
        sequence.pop_front();
    }
    for id in 17..=25 {
        sequence.push_back(ClassBase::Divergent(DivergentType::new(
            salsa::plumbing::Id::from_bits(id),
        )));
    }
    assert!(!sequence.as_slices().1.is_empty());
    let expected = sequence.iter().copied().collect::<Vec<_>>();
    assert_eq!(expected.len(), 14);
    let mut output = Vec::with_capacity(8);
    let expected_appends = expected
        .iter()
        .map(|entry| {
            let work = C3Work::OutputAppend {
                prefix_len: output.len(),
                capacity: output.capacity(),
            };
            output.push(*entry);
            work
        })
        .collect::<Vec<_>>();
    let work = assert_merge(
        &db,
        Vec::from([VecDeque::new(), sequence, VecDeque::new()]),
        Some(Mro::from(expected)),
    );
    assert_eq!(
        work.iter()
            .filter(|work| matches!(work, C3Work::OutputAppend { .. }))
            .copied()
            .collect::<Vec<_>>(),
        expected_appends,
    );
    assert_eq!(
        &work[work.len() - 2..],
        [C3Work::BoxOutput { len: 14, capacity: output.capacity() }, C3Work::Publish],
    );
    Ok(())
}

#[test]
fn rejected_candidates_stop_at_the_first_equal_tail_identity() -> anyhow::Result<()> {
    let db = database()?;
    let a = ClassBase::Any;
    let b = ClassBase::Generic;
    let c = ClassBase::Protocol;
    let d = ClassBase::unknown();
    for early in [true, false] {
        let blocking = if early { [b, a, c] } else { [b, c, a] };
        let expected = if early { [b, d, a, c] } else { [b, c, d, a] };
        let work = assert_merge(
            &db,
            Vec::from([
                VecDeque::from([a]),
                VecDeque::from(blocking),
                VecDeque::from([d, a]),
            ]),
            Some(Mro::from(expected)),
        );
        let mut prefix = Vec::from([
            C3Work::OutputCapacity { entries: 8 },
            C3Work::RetainSequences { len: 3 },
            C3Work::CandidateAdvance,
            C3Work::TailSequenceAdvance,
            C3Work::TailEntryAdvance,
            C3Work::TailSequenceAdvance,
            C3Work::TailEntryAdvance,
            C3Work::IdentityComparison { todo_bytes: 0 },
        ]);
        if !early {
            prefix.extend([
                C3Work::TailEntryAdvance,
                C3Work::IdentityComparison { todo_bytes: 0 },
            ]);
        }
        prefix.push(C3Work::CandidateAdvance);
        assert_eq!(&work[..prefix.len()], prefix);
    }
    Ok(())
}

#[test]
fn inconsistency_discards_the_selected_prefix_and_charges_candidate_termination()
-> anyhow::Result<()> {
    let db = database()?;
    let capacity = Vec::<ClassBase<'_>>::with_capacity(8).capacity();
    let work = assert_merge(
        &db,
        Vec::from([
            VecDeque::from([ClassBase::unknown()]),
            VecDeque::from([ClassBase::Any, ClassBase::Generic]),
            VecDeque::from([ClassBase::Generic, ClassBase::Any]),
        ]),
        None,
    );
    assert_eq!(
        work.iter()
            .filter(|work| matches!(work, C3Work::OutputAppend { .. }))
            .copied()
            .collect::<Vec<_>>(),
        [C3Work::OutputAppend {
            prefix_len: 0,
            capacity
        }],
    );
    assert_eq!(
        &work[work.len() - 2..],
        [C3Work::CandidateAdvance, C3Work::Publish],
    );
    assert!(
        !work
            .iter()
            .any(|work| matches!(work, C3Work::BoxOutput { .. }))
    );
    Ok(())
}

#[test]
fn every_rejected_checkpoint_stops_without_publishing_a_partial_answer() -> anyhow::Result<()> {
    let db = database()?;
    for sequences in [
        Vec::from([
            VecDeque::from([ClassBase::unknown()]),
            VecDeque::from([ClassBase::Any, ClassBase::Generic]),
            VecDeque::from([ClassBase::Protocol, ClassBase::Generic]),
            VecDeque::from([ClassBase::Any, ClassBase::Protocol]),
        ]),
        Vec::from([
            VecDeque::from([ClassBase::unknown()]),
            VecDeque::from([ClassBase::Any, ClassBase::Generic]),
            VecDeque::from([ClassBase::Generic, ClassBase::Any]),
        ]),
    ] {
        let completed = RecordingEffects::default();
        assert!(matches!(
            try_poll_immediate(c3_merge_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                sequences.clone(),
                &completed
            )),
            Poll::Ready(Ok(_)),
        ));
        let work = completed.work.into_inner();
        for index in 0..work.len() {
            let effects = RecordingEffects {
                fail_at: Some(index),
                ..RecordingEffects::default()
            };
            assert_eq!(
                try_poll_immediate(c3_merge_with(
                    crate::types::mro::field_reads::MroFieldReads::new(&db),
                    sequences.clone(),
                    &effects
                )),
                Poll::Ready(Err(index)),
            );
            assert_eq!(*effects.work.borrow(), work[..=index]);
            assert_eq!(effects.publications.get(), 0);
        }
    }
    Ok(())
}

#[test]
fn todo_comparisons_admit_equal_length_bytes_before_tail_and_head_equality() -> anyhow::Result<()> {
    let db = database()?;
    let left = ClassBase::Dynamic(todo_type!("abc").expect_dynamic());
    for (right, right_len) in [
        (ClassBase::Dynamic(todo_type!("abc").expect_dynamic()), 3),
        (ClassBase::Dynamic(todo_type!("abd").expect_dynamic()), 3),
        (ClassBase::Dynamic(todo_type!("z").expect_dynamic()), 1),
    ] {
        let common_bytes = if cfg!(debug_assertions) && right_len == 3 {
            3
        } else {
            0
        };
        let left_bytes = if cfg!(debug_assertions) { 3 } else { 0 };
        assert_eq!(
            comparison_label_bytes(left.into(), right.into()),
            common_bytes,
        );
        assert_eq!(comparison_label_bytes(left.into(), Type::unknown()), 0);
        assert_eq!(comparison_label_bytes(Type::unknown(), right.into()), 0);

        let expected = if left == right {
            Mro::from([left])
        } else {
            Mro::from([left, right])
        };
        let work = assert_merge(
            &db,
            Vec::from([VecDeque::from([left]), VecDeque::from([right])]),
            Some(expected),
        );
        let removals = work
            .iter()
            .filter_map(|work| match work {
                C3Work::RemoveHead { todo_bytes } => Some(*todo_bytes),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(&removals[..2], [left_bytes, common_bytes]);

        let expected = if left == right {
            Mro::from([ClassBase::Generic, left])
        } else {
            Mro::from([left, ClassBase::Generic, right])
        };
        let work = assert_merge(
            &db,
            Vec::from([
                VecDeque::from([left]),
                VecDeque::from([ClassBase::Generic, right]),
            ]),
            Some(expected),
        );
        assert_eq!(
            work.iter().find_map(|work| match work {
                C3Work::IdentityComparison { todo_bytes } => Some(*todo_bytes),
                _ => None,
            }),
            Some(common_bytes),
        );
    }
    Ok(())
}
