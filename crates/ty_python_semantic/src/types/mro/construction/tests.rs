use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;
use ty_python_core::scope::ScopeId;

use super::{
    InlineStaticMroEffects, StaticMroEffects, StaticMroFacts, StaticMroWork,
    SynchronousStaticMroEffects, base_has_cyclic_mro_with, maybe_add_generic_with, sealed,
    static_mro_with,
};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::mro::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{
    ClassLiteral, ClassType, DivergentType, DynamicType, GenericAlias, KnownInstanceType,
    SpecialFormType, StaticClassLiteral, Type, TypingModule,
};
use crate::{Db, ProgramEnvironment};

fn database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/construction.py",
        r#"
from enum import Enum
from typing import NamedTuple, TypedDict
from missing import unknown

class Plain: ...
class A: ...
class B: ...
class Single(A): ...
class Multiple(A, B): ...
class Base[T]: ...
class Alias(Base[int]): ...
class Forward[T](Base[T]): ...
class Invalid(42, A, False): ...
class InvalidSingle(42): ...
class Duplicate(A, A): ...
class DuplicateDynamic(unknown, unknown): ...

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
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/construction.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("expected a class literal for {name}"))
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    literal(db, name)?
        .as_static()
        .ok_or_else(|| anyhow::anyhow!("expected a static class for {name}"))
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ErrorKind<'db> {
    Invalid(Vec<(usize, Type<'db>)>),
    Cycle,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event<'db> {
    Checkpoint(StaticMroWork),
    Root(StaticClassLiteral<'db>, Option<Specialization<'db>>),
    ExplicitBases(StaticClassLiteral<'db>),
    Pep695(StaticClassLiteral<'db>),
    Convert(usize, Type<'db>),
    Object,
    Cycle(StaticClassLiteral<'db>, Option<Specialization<'db>>),
    CollectSingle(ClassType<'db>, ClassBase<'db>, Option<Specialization<'db>>),
    Collect(ClassBase<'db>, Option<Specialization<'db>>),
    Specialize(ClassBase<'db>, Option<Specialization<'db>>),
    C3(Vec<Vec<ClassBase<'db>>>),
    Error(ErrorKind<'db>),
    FailedC3(StaticClassLiteral<'db>, Vec<Type<'db>>, Vec<ClassBase<'db>>),
}

impl Event<'_> {
    fn operation(&self) -> Option<&'static str> {
        Some(match self {
            Self::Checkpoint(_) => return None,
            Self::Root(..) => "root",
            Self::ExplicitBases(_) => "explicit bases",
            Self::Pep695(_) => "classification",
            Self::Convert(..) => "conversion",
            Self::Object => "object",
            Self::Cycle(..) => "cycle",
            Self::CollectSingle(..) => "single collection",
            Self::Collect(..) => "collection",
            Self::Specialize(..) => "specialization",
            Self::C3(_) => "C3",
            Self::Error(_) => "error",
            Self::FailedC3(..) => "error details",
        })
    }
}

struct RecordingEffects<'db> {
    db: &'db dyn Db,
    inline: InlineStaticMroEffects<'db>,
    events: RefCell<Vec<Event<'db>>>,
    rejected_operation: Option<&'static str>,
    rejected_work: Option<StaticMroWork>,
    cycle: Option<bool>,
}

impl<'db> RecordingEffects<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            inline: InlineStaticMroEffects::new(db),
            events: RefCell::new(Vec::new()),
            rejected_operation: None,
            rejected_work: None,
            cycle: None,
        }
    }

    fn record(&self, event: Event<'db>, operation: &'static str) -> Result<(), &'static str> {
        assert_eq!(event.operation(), Some(operation));
        self.events.borrow_mut().push(event);
        if self.rejected_operation == Some(operation) {
            Err(operation)
        } else {
            Ok(())
        }
    }

    fn calls(&self) -> Vec<Event<'db>> {
        self.events
            .borrow()
            .iter()
            .filter(|event| !matches!(event, Event::Checkpoint(_)))
            .cloned()
            .collect()
    }

    fn work(&self) -> Vec<StaticMroWork> {
        self.events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Checkpoint(work) => Some(*work),
                _ => None,
            })
            .collect()
    }
}

impl sealed::Sealed for RecordingEffects<'_> {}

impl<'db> StaticMroFacts<'db> for RecordingEffects<'db> {
    type Error = &'static str;
}

impl<'db> StaticMroEffects<'db> for RecordingEffects<'db> {
    async fn body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(infallible(self.inline.body_scope(class)))
    }

    async fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(infallible(self.inline.is_object(class)))
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        Ok(infallible(self.inline.static_class_literal(class)))
    }

    async fn explicit_bases<'call>(
        &'call self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'call [Type<'db>], Self::Error>
    where
        'db: 'call,
    {
        self.record(Event::ExplicitBases(class), "explicit bases")?;
        Ok(infallible(self.inline.explicit_bases(class)))
    }

    async fn has_pep_695_type_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.record(Event::Pep695(class), "classification")?;
        Ok(infallible(self.inline.has_pep_695_type_params(class)))
    }

    async fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        assert_eq!(class.explicit_bases(self.db)[index], ty);
        assert_eq!(
            env.program(self.db),
            ProgramEnvironment::from_scope(class.body_scope(self.db)).program(self.db),
        );
        self.record(Event::Convert(index, ty), "conversion")?;
        Ok(infallible(
            self.inline.converted_explicit_base(env, class, index, ty),
        ))
    }

    async fn object_base(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        self.record(Event::Object, "object")?;
        Ok(infallible(self.inline.object_base(env)))
    }

    async fn checkpoint(&self, work: StaticMroWork) -> Result<(), Self::Error> {
        self.events.borrow_mut().push(Event::Checkpoint(work));
        if self.rejected_work == Some(work) {
            Err("checkpoint")
        } else {
            Ok(())
        }
    }

    async fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.record(Event::Root(class, specialization), "root")?;
        Ok(infallible(self.inline.root_class(class, specialization)))
    }

    async fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Self::Error> {
        self.record(Event::Cycle(class, specialization), "cycle")?;
        Ok(self
            .cycle
            .unwrap_or_else(|| infallible(self.inline.static_mro_is_cycle(class, specialization))))
    }

    async fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Self::Error> {
        self.record(
            Event::CollectSingle(root, base, additional),
            "single collection",
        )?;
        Ok(infallible(
            self.inline
                .collect_single_base_mro(env, root, base, additional),
        ))
    }

    async fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error> {
        self.record(Event::Collect(base, additional), "collection")?;
        Ok(infallible(
            self.inline.collect_base_mro(env, base, additional),
        ))
    }

    async fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        self.record(Event::Specialize(base, specialization), "specialization")?;
        Ok(infallible(
            self.inline.specialize_base(base, specialization),
        ))
    }

    async fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Self::Error> {
        self.record(
            Event::C3(
                sequences
                    .iter()
                    .map(|sequence| sequence.iter().copied().collect())
                    .collect(),
            ),
            "C3",
        )?;
        Ok(infallible(self.inline.c3_merge(sequences)))
    }

    async fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Self::Error> {
        let recorded = match &kind {
            StaticMroErrorKind::InvalidBases(bases) => ErrorKind::Invalid(bases.to_vec()),
            StaticMroErrorKind::InheritanceCycle => ErrorKind::Cycle,
            _ => ErrorKind::Other,
        };
        self.record(Event::Error(recorded), "error")?;
        Ok(infallible(self.inline.make_error(env, class, kind)))
    }

    async fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Self::Error> {
        self.record(
            Event::FailedC3(
                class_literal,
                original_bases.to_vec(),
                resolved_bases.to_vec(),
            ),
            "error details",
        )?;
        Ok(infallible(self.inline.failed_c3(
            env,
            class_literal,
            class,
            original_bases,
            resolved_bases,
        )))
    }
}

#[test]
fn single_base_uses_its_cycle_probe_before_the_specialized_iterator() -> anyhow::Result<()> {
    let db = database()?;
    let root = class(&db, "Single")?;
    let base = class(&db, "A")?;
    let base_ty = ClassBase::Class(ClassType::NonGeneric(base.into()));
    let root_ty = ClassType::NonGeneric(root.into());
    let effects = RecordingEffects::new(&db);
    assert_eq!(
        try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            None,
            &effects
        )),
        Poll::Ready(Ok(Mro::of_static_class(&db, root, None))),
    );
    assert_eq!(
        effects.calls(),
        [
            Event::Root(root, None),
            Event::ExplicitBases(root),
            Event::Pep695(root),
            Event::Convert(0, Type::ClassLiteral(base.into())),
            Event::Cycle(base, None),
            Event::CollectSingle(root_ty, base_ty, None),
        ],
    );
    assert_eq!(
        effects.work(),
        [
            StaticMroWork::Begin,
            StaticMroWork::RootRequest,
            StaticMroWork::ExplicitBases,
            StaticMroWork::Pep695Classification,
            StaticMroWork::ConvertBase { index: 0 },
            StaticMroWork::BaseCycleDispatch,
            StaticMroWork::StaticCycleRequest,
            StaticMroWork::SingleBaseCollectionRequest,
            StaticMroWork::Publish,
        ],
    );
    Ok(())
}

#[test]
fn multiple_bases_finish_collection_before_direct_specialization() -> anyhow::Result<()> {
    let db = database()?;
    let root = class(&db, "Multiple")?;
    let a = class(&db, "A")?;
    let b = class(&db, "B")?;
    let a_base = ClassBase::Class(ClassType::NonGeneric(a.into()));
    let b_base = ClassBase::Class(ClassType::NonGeneric(b.into()));
    let root_base = ClassBase::Class(ClassType::NonGeneric(root.into()));
    let object = ClassBase::object(&db, &db.program_environment());
    let effects = RecordingEffects::new(&db);
    assert_eq!(
        try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            None,
            &effects
        )),
        Poll::Ready(Ok(Ok(Mro::from([root_base, a_base, b_base, object])))),
    );
    assert_eq!(
        effects.calls(),
        [
            Event::Root(root, None),
            Event::ExplicitBases(root),
            Event::Convert(0, Type::ClassLiteral(a.into())),
            Event::Convert(1, Type::ClassLiteral(b.into())),
            Event::Pep695(root),
            Event::Cycle(a, None),
            Event::Collect(a_base, None),
            Event::Cycle(b, None),
            Event::Collect(b_base, None),
            Event::Specialize(a_base, None),
            Event::Specialize(b_base, None),
            Event::C3(vec![
                vec![root_base],
                vec![a_base, object],
                vec![b_base, object],
                vec![a_base, b_base]
            ]),
        ],
    );
    for advance in [
        StaticMroWork::RawBaseAdvance,
        StaticMroWork::ResolvedBaseAdvance,
        StaticMroWork::DirectBaseAdvance,
    ] {
        assert_eq!(
            effects
                .work()
                .iter()
                .filter(|work| **work == advance)
                .count(),
            3
        );
    }
    assert_eq!(effects.work().last(), Some(&StaticMroWork::Publish));
    Ok(())
}

#[test]
fn a_single_alias_reads_classification_before_and_after_conversion() -> anyhow::Result<()> {
    let db = database()?;
    let root = class(&db, "Alias")?;
    let raw = root.explicit_bases(&db)[0];
    assert!(matches!(raw, Type::GenericAlias(_)));
    let mut effects = RecordingEffects::new(&db);
    effects.rejected_operation = Some("cycle");
    assert_eq!(
        try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            None,
            &effects
        )),
        Poll::Ready(Err("cycle")),
    );
    let calls = effects.calls();
    assert_eq!(
        &calls[..5],
        [
            Event::Root(root, None),
            Event::ExplicitBases(root),
            Event::Pep695(root),
            Event::Convert(0, raw),
            Event::Pep695(root)
        ]
    );
    assert!(matches!(calls.last(), Some(Event::Cycle(_, Some(_)))));
    assert!(!effects.work().contains(&StaticMroWork::Publish));
    Ok(())
}

#[test]
fn invalid_bases_keep_all_original_indices_before_error_construction() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["InvalidSingle", "Invalid"] {
        let root = class(&db, name)?;
        let raw = root.explicit_bases(&db);
        let invalid = if raw.len() == 1 {
            vec![(0, raw[0])]
        } else {
            vec![(0, raw[0]), (2, raw[2])]
        };
        let effects = RecordingEffects::new(&db);
        let Poll::Ready(Ok(Err(error))) = try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            None,
            &effects,
        )) else {
            anyhow::bail!("invalid bases must produce a completed semantic error");
        };
        assert_eq!(
            error.reason(),
            &StaticMroErrorKind::InvalidBases(invalid.clone().into_boxed_slice())
        );
        let mut expected = vec![Event::Root(root, None), Event::ExplicitBases(root)];
        if raw.len() == 1 {
            expected.push(Event::Pep695(root));
        }
        expected.extend(
            raw.iter()
                .enumerate()
                .map(|(index, ty)| Event::Convert(index, *ty)),
        );
        expected.push(Event::Error(ErrorKind::Invalid(invalid)));
        assert_eq!(effects.calls(), expected);
        assert_eq!(effects.work().last(), Some(&StaticMroWork::Publish));
    }
    Ok(())
}

#[test]
fn forwarding_probes_the_original_alias_before_composing_the_receiver() -> anyhow::Result<()> {
    let db = database()?;
    let root = class(&db, "Forward")?;
    let context = root
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Forward must be generic"))?;
    let supplied = context.specialize(&db, &[Type::int_literal(7)]);
    let Type::GenericAlias(alias) = root.explicit_bases(&db)[0] else {
        anyhow::bail!("Forward must declare a generic alias base");
    };
    let mut effects = RecordingEffects::new(&db);
    effects.rejected_operation = Some("cycle");
    assert_eq!(
        try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            Some(supplied),
            &effects
        )),
        Poll::Ready(Err("cycle")),
    );
    assert_eq!(
        effects.calls(),
        [
            Event::Root(root, Some(supplied)),
            Event::ExplicitBases(root),
            Event::Pep695(root),
            Event::Convert(0, Type::GenericAlias(alias)),
            Event::Pep695(root),
            Event::Cycle(alias.origin(&db), Some(alias.specialization(&db))),
        ],
    );
    assert_ne!(alias.specialization(&db), supplied);
    assert!(!effects.work().contains(&StaticMroWork::Publish));
    Ok(())
}

#[test]
fn collection_and_direct_bases_receive_the_same_additional_specialization() -> anyhow::Result<()> {
    let db = database()?;
    let root = class(&db, "Forward")?;
    let context = root
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Forward must be generic"))?;
    let supplied = context.specialize(&db, &[Type::int_literal(7)]);
    let Type::GenericAlias(original) = root.explicit_bases(&db)[0] else {
        anyhow::bail!("Forward must declare a generic alias base");
    };
    let base_context = original
        .origin(&db)
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Base must be generic"))?;
    let specialized_base = ClassBase::Class(ClassType::Generic(GenericAlias::new(
        &db,
        original.origin(&db),
        base_context.specialize(&db, &[Type::int_literal(7)]),
    )));
    let specialized_root =
        ClassBase::Class(ClassType::Generic(GenericAlias::new(&db, root, supplied)));
    let object = ClassBase::object(&db, &db.program_environment());
    let effects = RecordingEffects::new(&db);
    assert_eq!(
        try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            Some(supplied),
            &effects
        )),
        Poll::Ready(Ok(Ok(Mro::from([
            specialized_root,
            specialized_base,
            ClassBase::Generic,
            object
        ])))),
    );
    let original_base = ClassBase::Class(ClassType::Generic(original));
    let calls = effects.calls();
    let transformations: Vec<_> = calls
        .iter()
        .filter(|event| matches!(event, Event::Collect(..) | Event::Specialize(..)))
        .collect();
    assert_eq!(
        transformations,
        [
            &Event::Collect(original_base, Some(supplied)),
            &Event::Collect(ClassBase::Generic, Some(supplied)),
            &Event::Specialize(original_base, Some(supplied)),
            &Event::Specialize(ClassBase::Generic, Some(supplied)),
        ],
    );
    assert_eq!(
        calls.last(),
        Some(&Event::C3(vec![
            vec![specialized_root],
            vec![specialized_base, ClassBase::Generic, object],
            vec![ClassBase::Generic, object],
            vec![specialized_base, ClassBase::Generic],
        ])),
    );
    Ok(())
}

#[test]
fn generic_insertion_distinguishes_bare_protocol_and_the_remaining_suffix() -> anyhow::Result<()> {
    let db = database()?;
    let base = class(&db, "Base")?;
    let context = base
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Base must be generic"))?;
    let alias = Type::GenericAlias(GenericAlias::new(
        &db,
        base,
        context.specialize(&db, &[Type::int_literal(3)]),
    ));
    let protocol = Type::SpecialForm(SpecialFormType::Protocol);
    let subscripted = Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(context));
    for (original, remaining, append, scan_suffix) in [
        (vec![protocol, alias], vec![alias], false, false),
        (vec![subscripted], vec![], true, true),
        (vec![alias], vec![alias], false, true),
        (vec![alias], vec![], true, true),
        (vec![], vec![], true, true),
    ] {
        let effects = RecordingEffects::new(&db);
        let mut resolved = vec![ClassBase::Any, ClassBase::Protocol];
        assert_eq!(
            try_poll_immediate(maybe_add_generic_with(
                &mut resolved,
                &original,
                &remaining,
                &effects
            )),
            Poll::Ready(Ok(()))
        );
        let mut expected = vec![ClassBase::Any, ClassBase::Protocol];
        if append {
            expected.push(ClassBase::Generic);
        }
        assert_eq!(resolved, expected);
        let mut work = vec![StaticMroWork::GenericProtocolScan {
            len: original.len(),
        }];
        if scan_suffix {
            work.push(StaticMroWork::GenericAliasScan {
                len: remaining.len(),
            });
        }
        if append {
            work.push(StaticMroWork::ResolvedBaseAppend { prefix_len: 2, capacity: 2 });
        }
        assert_eq!(effects.work(), work);
        assert!(effects.calls().is_empty());
    }
    Ok(())
}

#[test]
fn rejected_generic_scan_or_growth_does_not_append() -> anyhow::Result<()> {
    let db = database()?;
    for rejected in [
        StaticMroWork::GenericProtocolScan { len: 0 },
        StaticMroWork::GenericAliasScan { len: 0 },
        StaticMroWork::ResolvedBaseAppend { prefix_len: 1, capacity: 1 },
    ] {
        let mut effects = RecordingEffects::new(&db);
        effects.rejected_work = Some(rejected);
        let mut resolved = vec![ClassBase::Any];
        assert_eq!(
            try_poll_immediate(maybe_add_generic_with(&mut resolved, &[], &[], &effects)),
            Poll::Ready(Err("checkpoint"))
        );
        assert_eq!(resolved, [ClassBase::Any]);
        assert_eq!(effects.work().last(), Some(&rejected));
        assert!(effects.calls().is_empty());
    }
    Ok(())
}

#[test]
fn only_static_class_bases_request_cycle_queries() -> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db, "Plain")?;
    let base = class(&db, "Base")?;
    let context = base
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Base must be generic"))?;
    let specialization = context.specialize(&db, &[Type::int_literal(3)]);
    for (literal, specialization) in [(plain, None), (base, Some(specialization))] {
        let mut effects = RecordingEffects::new(&db);
        effects.cycle = Some(true);
        let class = infallible(effects.inline.root_class(literal, specialization));
        assert_eq!(
            try_poll_immediate(base_has_cyclic_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                ClassBase::Class(class),
                &effects
            )),
            Poll::Ready(Ok(true))
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(StaticMroWork::BaseCycleDispatch),
                Event::Checkpoint(StaticMroWork::StaticCycleRequest),
                Event::Cycle(literal, specialization)
            ]
        );
    }
    let mut nonstatic = vec![
        ClassBase::Any,
        ClassBase::Dynamic(DynamicType::Any),
        ClassBase::unknown(),
        ClassBase::Divergent(DivergentType::new(salsa::plumbing::Id::from_bits(1))),
        ClassBase::Protocol,
        ClassBase::Generic,
        ClassBase::TypedDict(TypingModule::Typing),
        ClassBase::TypedDict(TypingModule::TypingExtensions),
    ];
    for name in ["Dynamic", "Named", "Typed", "Enumeration"] {
        nonstatic.push(ClassBase::Class(ClassType::NonGeneric(literal(&db, name)?)));
    }
    for base in nonstatic {
        let mut effects = RecordingEffects::new(&db);
        effects.rejected_operation = Some("cycle");
        assert_eq!(
            try_poll_immediate(base_has_cyclic_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                base,
                &effects
            )),
            Poll::Ready(Ok(false))
        );
        assert_eq!(
            *effects.events.borrow(),
            [Event::Checkpoint(StaticMroWork::BaseCycleDispatch)]
        );
    }
    Ok(())
}

#[test]
fn a_cycle_error_is_published_without_collecting_the_base() -> anyhow::Result<()> {
    let db = database()?;
    let root = class(&db, "Single")?;
    let mut effects = RecordingEffects::new(&db);
    effects.cycle = Some(true);
    let Poll::Ready(Ok(Err(error))) = try_poll_immediate(static_mro_with(
        crate::types::mro::field_reads::MroFieldReads::new(&db),
        root,
        None,
        &effects,
    )) else {
        anyhow::bail!("a completed cycle probe must produce a semantic cycle error");
    };
    assert!(error.is_cycle());
    assert_eq!(
        effects.calls().last(),
        Some(&Event::Error(ErrorKind::Cycle))
    );
    assert!(
        !effects
            .work()
            .contains(&StaticMroWork::SingleBaseCollectionRequest)
    );
    assert_eq!(effects.work().last(), Some(&StaticMroWork::Publish));
    Ok(())
}

#[test]
fn failed_c3_preserves_semantic_success_and_error_results() -> anyhow::Result<()> {
    let db = database()?;
    for (name, succeeds) in [("DuplicateDynamic", true), ("Duplicate", false)] {
        let root = class(&db, name)?;
        let effects = RecordingEffects::new(&db);
        let Poll::Ready(Ok(result)) = try_poll_immediate(static_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            root,
            None,
            &effects,
        )) else {
            anyhow::bail!("ordinary dependencies must complete construction");
        };
        assert_eq!(result.is_ok(), succeeds);
        assert_eq!(result, Mro::of_static_class(&db, root, None));
        let calls = effects.calls();
        assert!(matches!(&calls[calls.len() - 2], Event::C3(_)));
        assert!(
            matches!(calls.last(), Some(Event::FailedC3(class, raw, resolved)) if *class == root && raw == root.explicit_bases(&db) && resolved.len() == 2)
        );
        if let Ok(mro) = result {
            assert_eq!(
                mro,
                Mro::from_error(
                    &db,
                    &db.program_environment(),
                    ClassType::NonGeneric(root.into())
                )
            );
        }
        assert_eq!(effects.work().last(), Some(&StaticMroWork::Publish));
    }
    Ok(())
}

#[test]
fn dependency_failures_stop_before_later_operations_or_publication() -> anyhow::Result<()> {
    let db = database()?;
    for (name, operation) in [
        ("Plain", "root"),
        ("Plain", "explicit bases"),
        ("Plain", "classification"),
        ("Plain", "object"),
        ("Single", "conversion"),
        ("Single", "cycle"),
        ("Single", "single collection"),
        ("Multiple", "collection"),
        ("Multiple", "specialization"),
        ("Multiple", "C3"),
        ("Invalid", "error"),
        ("DuplicateDynamic", "error details"),
    ] {
        let root = class(&db, name)?;
        let mut effects = RecordingEffects::new(&db);
        effects.rejected_operation = Some(operation);
        assert_eq!(
            try_poll_immediate(static_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                root,
                None,
                &effects
            )),
            Poll::Ready(Err(operation))
        );
        assert!(!effects.work().contains(&StaticMroWork::Publish));
        assert_eq!(
            effects.events.borrow().last().and_then(Event::operation),
            Some(operation)
        );
        if operation == "conversion" {
            assert_eq!(
                effects.calls().last(),
                Some(&Event::Convert(0, root.explicit_bases(&db)[0]))
            );
        }
    }
    Ok(())
}

#[test]
fn failed_admission_and_publication_do_not_return_partial_results() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["Plain", "InvalidSingle", "DuplicateDynamic", "Duplicate"] {
        let root = class(&db, name)?;
        for rejected in [StaticMroWork::Begin, StaticMroWork::Publish] {
            let mut effects = RecordingEffects::new(&db);
            effects.rejected_work = Some(rejected);
            assert_eq!(
                try_poll_immediate(static_mro_with(
                    crate::types::mro::field_reads::MroFieldReads::new(&db),
                    root,
                    None,
                    &effects
                )),
                Poll::Ready(Err("checkpoint"))
            );
            assert_eq!(effects.work().last(), Some(&rejected));
            if rejected == StaticMroWork::Begin {
                assert_eq!(
                    *effects.events.borrow(),
                    [Event::Checkpoint(StaticMroWork::Begin)]
                );
            }
        }
    }
    Ok(())
}
