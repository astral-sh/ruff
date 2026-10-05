use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::fmt::Debug;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{
    BaseMroEffects, BaseMroFacts, BaseMroStart, BaseMroWork, ClassMroStart, InlineBaseMroEffects,
    SynchronousBaseMroEffects, base_mro_start_with, class_mro_start_with, collect_base_mro_with,
    collect_single_base_mro_with, sealed,
};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::mro::Mro;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::tuple::TupleType;
use crate::types::{
    ClassLiteral, ClassType, DivergentType, DynamicType, GenericAlias, KnownClass,
    StaticClassLiteral, Type, TypingModule,
};
use crate::{Db, ProgramEnvironment};

fn database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/base_mro.py",
        r#"
from enum import Enum
from typing import NamedTuple, TypedDict

class Plain: ...
class Base[T]: ...
class Child[U](Base[U]): ...

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
        system_path_to_file(db, "/src/base_mro.py")?,
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

fn context<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<GenericContext<'db>> {
    class(db, name)?
        .generic_context(db)
        .ok_or_else(|| anyhow::anyhow!("expected a generic context for {name}"))
}

fn original_alias(db: &TestDb) -> anyhow::Result<GenericAlias<'_>> {
    let [Type::GenericAlias(alias)] = class(db, "Child")?.explicit_bases(db) else {
        anyhow::bail!("Child must declare one generic alias base");
    };
    Ok(*alias)
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Checkpoint(BaseMroWork),
    Object,
    Compose(Specialization<'db>, Specialization<'db>),
    Collect(BaseMroStart<'db>),
    CollectWithRoot(ClassType<'db>, BaseMroStart<'db>),
}

struct RecordingEffects<'db> {
    inline: InlineBaseMroEffects<'db>,
    events: RefCell<Vec<Event<'db>>>,
    rejected_operation: Option<&'static str>,
    rejected_checkpoint: Option<usize>,
}

impl<'db> RecordingEffects<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self {
            inline: InlineBaseMroEffects::new(db),
            events: RefCell::new(Vec::new()),
            rejected_operation: None,
            rejected_checkpoint: None,
        }
    }

    fn record(&self, event: Event<'db>, operation: &'static str) -> Result<(), &'static str> {
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
            .copied()
            .filter(|event| !matches!(event, Event::Checkpoint(_)))
            .collect()
    }
}

impl sealed::Sealed for RecordingEffects<'_> {}

impl<'db> BaseMroFacts<'db> for RecordingEffects<'db> {
    type Error = &'static str;
}

impl<'db> BaseMroEffects<'db> for RecordingEffects<'db> {
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(infallible(self.inline.alias_origin(alias)))
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(infallible(self.inline.alias_specialization(alias)))
    }

    async fn object_base(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        self.record(Event::Object, "object")?;
        Ok(infallible(self.inline.object_base(env)))
    }

    async fn checkpoint(&self, work: BaseMroWork) -> Result<(), Self::Error> {
        let index = self.events.borrow().len();
        self.events.borrow_mut().push(Event::Checkpoint(work));
        if self.rejected_checkpoint == Some(index) {
            Err("checkpoint")
        } else {
            Ok(())
        }
    }

    async fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.record(Event::Compose(base, additional), "composition")?;
        Ok(infallible(
            self.inline.compose_specialization(base, additional),
        ))
    }

    async fn collect_start(
        &self,
        start: BaseMroStart<'db>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error> {
        self.record(Event::Collect(start), "collection")?;
        Ok(infallible(self.inline.collect_start(start)))
    }

    async fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> Result<Mro<'db>, Self::Error> {
        self.record(Event::CollectWithRoot(root, start), "single collection")?;
        Ok(infallible(self.inline.collect_start_with_root(root, start)))
    }
}

#[test]
fn nongeneric_class_starts_ignore_additional_specialization() -> anyhow::Result<()> {
    let db = database()?;
    let supplied = context(&db, "Child")?.specialize(&db, &[Type::int_literal(7)]);
    for name in ["Plain", "Dynamic", "Named", "Typed", "Enumeration"] {
        let literal = literal(&db, name)?;
        for additional in [None, Some(supplied)] {
            let effects = RecordingEffects::new(&db);
            assert_eq!(
                try_poll_immediate(class_mro_start_with(
                    crate::types::mro::field_reads::MroFieldReads::new(&db),
                    ClassType::NonGeneric(literal),
                    additional,
                    &effects
                )),
                Poll::Ready(Ok(ClassMroStart {
                    class: literal,
                    specialization: None
                })),
            );
            assert_eq!(
                *effects.events.borrow(),
                [
                    Event::Checkpoint(BaseMroWork::ClassDispatch),
                    Event::Checkpoint(BaseMroWork::Publish)
                ]
            );
        }
    }
    Ok(())
}

#[test]
fn generic_starts_distinguish_absence_identity_and_composition() -> anyhow::Result<()> {
    let db = database()?;
    let original = original_alias(&db)?;
    let original_specialization = original.specialization(&db);
    let child_context = context(&db, "Child")?;
    let identity = child_context.identity_specialization(&db);
    let supplied = child_context.specialize(&db, &[Type::int_literal(7)]);
    let composed = context(&db, "Base")?.specialize(&db, &[Type::int_literal(7)]);
    for (additional, expected) in [
        (None, original_specialization),
        (Some(identity), original_specialization),
        (Some(supplied), composed),
    ] {
        let effects = RecordingEffects::new(&db);
        assert_eq!(
            try_poll_immediate(class_mro_start_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                ClassType::Generic(original),
                additional,
                &effects
            )),
            Poll::Ready(Ok(ClassMroStart {
                class: original.origin(&db).into(),
                specialization: Some(expected)
            })),
        );
        let mut expected_events = vec![Event::Checkpoint(BaseMroWork::ClassDispatch)];
        if let Some(additional) = additional {
            expected_events.extend([
                Event::Checkpoint(BaseMroWork::CompositionRequest),
                Event::Compose(original_specialization, additional),
            ]);
        }
        expected_events.push(Event::Checkpoint(BaseMroWork::Publish));
        assert_eq!(*effects.events.borrow(), expected_events);
    }
    assert_ne!(original_specialization, composed);
    Ok(())
}

#[test]
fn a_tuple_start_retains_the_original_shape() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let tuple = KnownClass::Tuple
        .try_to_class_literal(&db, &env)
        .ok_or_else(|| anyhow::anyhow!("tuple must be available"))?;
    let context = tuple
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("tuple must be generic"))?;
    let shaped = context.specialize_tuple(
        &db,
        Type::unknown(),
        TupleType::homogeneous(&db, &env, Type::int_literal(3)),
    );
    let effects = RecordingEffects::new(&db);
    assert_eq!(
        try_poll_immediate(class_mro_start_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            ClassType::Generic(GenericAlias::new(&db, tuple, shaped)),
            None,
            &effects
        )),
        Poll::Ready(Ok(ClassMroStart {
            class: tuple.into(),
            specialization: Some(shaped)
        })),
    );
    assert!(effects.calls().is_empty());
    Ok(())
}

#[test]
fn fixed_base_starts_preserve_every_base_payload_and_entry_order() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let object = ClassBase::object(&db, &env);
    let additional = context(&db, "Child")?.specialize(&db, &[Type::int_literal(7)]);
    for base in [
        ClassBase::Any,
        ClassBase::Dynamic(DynamicType::Any),
        ClassBase::unknown(),
        ClassBase::Divergent(DivergentType::new(salsa::plumbing::Id::from_bits(1))),
        ClassBase::Generic,
        ClassBase::TypedDict(TypingModule::Typing),
        ClassBase::TypedDict(TypingModule::TypingExtensions),
        ClassBase::Protocol,
    ] {
        let expected = if base == ClassBase::Protocol {
            BaseMroStart::Length3([ClassBase::Protocol, ClassBase::Generic, object])
        } else {
            BaseMroStart::Length2([base, object])
        };
        let effects = RecordingEffects::new(&db);
        assert_eq!(
            try_poll_immediate(base_mro_start_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &env,
                base,
                Some(additional),
                &effects
            )),
            Poll::Ready(Ok(expected))
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(BaseMroWork::BaseDispatch),
                Event::Checkpoint(BaseMroWork::ObjectBase),
                Event::Object,
                Event::Checkpoint(BaseMroWork::Publish),
            ],
        );
    }
    Ok(())
}

#[test]
fn class_bases_pass_the_composed_start_to_both_collection_operations() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let original = original_alias(&db)?;
    let additional = context(&db, "Child")?.specialize(&db, &[Type::int_literal(7)]);
    let composed = context(&db, "Base")?.specialize(&db, &[Type::int_literal(7)]);
    let start = BaseMroStart::Class(ClassMroStart {
        class: original.origin(&db).into(),
        specialization: Some(composed),
    });
    let root = ClassType::Generic(GenericAlias::new(&db, class(&db, "Child")?, additional));
    let base = ClassBase::Class(ClassType::Generic(original));
    let first = ClassBase::Class(ClassType::Generic(GenericAlias::new(
        &db,
        original.origin(&db),
        composed,
    )));
    let object = ClassBase::object(&db, &env);
    let effects = RecordingEffects::new(&db);
    assert_eq!(
        try_poll_immediate(collect_base_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            &env,
            base,
            Some(additional),
            &effects
        )),
        Poll::Ready(Ok(VecDeque::from([first, ClassBase::Generic, object]))),
    );
    assert_eq!(
        effects.calls(),
        [
            Event::Compose(original.specialization(&db), additional),
            Event::Collect(start)
        ]
    );
    let effects = RecordingEffects::new(&db);
    assert_eq!(
        try_poll_immediate(collect_single_base_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            &env,
            root,
            base,
            Some(additional),
            &effects
        )),
        Poll::Ready(Ok(Mro::from([
            ClassBase::Class(root),
            first,
            ClassBase::Generic,
            object
        ]))),
    );
    assert_eq!(
        effects.calls(),
        [
            Event::Compose(original.specialization(&db), additional),
            Event::CollectWithRoot(root, start)
        ]
    );
    Ok(())
}

#[test]
fn fixed_starts_reach_the_corresponding_collection_operation_unchanged() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let object = ClassBase::object(&db, &env);
    let root = ClassType::NonGeneric(literal(&db, "Plain")?);
    for (base, start, entries) in [
        (
            ClassBase::Any,
            BaseMroStart::Length2([ClassBase::Any, object]),
            vec![ClassBase::Any, object],
        ),
        (
            ClassBase::Protocol,
            BaseMroStart::Length3([ClassBase::Protocol, ClassBase::Generic, object]),
            vec![ClassBase::Protocol, ClassBase::Generic, object],
        ),
    ] {
        let effects = RecordingEffects::new(&db);
        assert_eq!(
            try_poll_immediate(collect_base_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &env,
                base,
                None,
                &effects
            )),
            Poll::Ready(Ok(VecDeque::from(entries.clone())))
        );
        assert_eq!(effects.calls(), [Event::Object, Event::Collect(start)]);
        let effects = RecordingEffects::new(&db);
        let expected: Mro<'_> = std::iter::once(ClassBase::Class(root))
            .chain(entries)
            .collect();
        assert_eq!(
            try_poll_immediate(collect_single_base_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &env,
                root,
                base,
                None,
                &effects
            )),
            Poll::Ready(Ok(expected))
        );
        assert_eq!(
            effects.calls(),
            [Event::Object, Event::CollectWithRoot(root, start)]
        );
    }
    Ok(())
}

#[test]
fn rejected_dependencies_stop_before_collection_or_publication() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let original = original_alias(&db)?;
    let additional = context(&db, "Child")?.specialize(&db, &[Type::int_literal(7)]);
    let base = ClassBase::Class(ClassType::Generic(original));
    for operation in ["composition", "collection"] {
        let mut effects = RecordingEffects::new(&db);
        effects.rejected_operation = Some(operation);
        assert_eq!(
            try_poll_immediate(collect_base_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &env,
                base,
                Some(additional),
                &effects
            )),
            Poll::Ready(Err(operation))
        );
        assert_eq!(
            effects.calls()[0],
            Event::Compose(original.specialization(&db), additional)
        );
        if operation == "composition" {
            assert_eq!(effects.calls().len(), 1);
            assert!(matches!(
                effects.events.borrow().last(),
                Some(Event::Compose(..))
            ));
        } else {
            assert!(matches!(
                effects.events.borrow().last(),
                Some(Event::Collect(_))
            ));
        }
    }
    let mut effects = RecordingEffects::new(&db);
    effects.rejected_operation = Some("object");
    assert_eq!(
        try_poll_immediate(collect_base_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            &env,
            ClassBase::Protocol,
            None,
            &effects
        )),
        Poll::Ready(Err("object"))
    );
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Checkpoint(BaseMroWork::BaseDispatch),
            Event::Checkpoint(BaseMroWork::ObjectBase),
            Event::Object
        ]
    );
    let root = ClassType::NonGeneric(literal(&db, "Plain")?);
    let mut effects = RecordingEffects::new(&db);
    effects.rejected_operation = Some("single collection");
    assert_eq!(
        try_poll_immediate(collect_single_base_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            &env,
            root,
            ClassBase::Protocol,
            None,
            &effects
        )),
        Poll::Ready(Err("single collection"))
    );
    assert!(
        matches!(effects.events.borrow().last(), Some(Event::CollectWithRoot(recorded, _)) if *recorded == root)
    );
    Ok(())
}

fn assert_checkpoint_rejections<'db, T: Debug + PartialEq>(
    db: &'db dyn Db,
    run: impl Fn(&RecordingEffects<'db>) -> Poll<Result<T, &'static str>>,
) {
    let successful = RecordingEffects::new(db);
    assert!(matches!(run(&successful), Poll::Ready(Ok(_))));
    let expected = successful.events.into_inner();
    for (index, event) in expected.iter().enumerate() {
        if !matches!(event, Event::Checkpoint(_)) {
            continue;
        }
        let mut rejected = RecordingEffects::new(db);
        rejected.rejected_checkpoint = Some(index);
        assert_eq!(run(&rejected), Poll::Ready(Err("checkpoint")));
        assert_eq!(*rejected.events.borrow(), expected[..=index]);
    }
}

#[test]
fn checkpoint_rejection_stops_at_each_start_and_collection_boundary() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let additional = context(&db, "Child")?.specialize(&db, &[Type::int_literal(7)]);
    let generic = ClassBase::Class(ClassType::Generic(original_alias(&db)?));
    let root = ClassType::NonGeneric(literal(&db, "Plain")?);
    for base in [generic, ClassBase::Protocol, ClassBase::Any] {
        assert_checkpoint_rejections(&db, |effects| {
            try_poll_immediate(collect_base_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &env,
                base,
                Some(additional),
                effects,
            ))
        });
        assert_checkpoint_rejections(&db, |effects| {
            try_poll_immediate(collect_single_base_mro_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &env,
                root,
                base,
                Some(additional),
                effects,
            ))
        });
    }
    Ok(())
}
