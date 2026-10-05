use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{SynthesizedMemberEffects, SynthesizedMemberWork, own_synthesized_member_with, sealed};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::own_member::OwnMemberLookupRequest;
use crate::types::class::{CodeGeneratorKind, FrozenDataclassMethod};
use crate::types::generics::Specialization;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{ClassLiteral, GenericContext, StaticClassLiteral, Type};

fn database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/synthesis.py",
        r#"
from dataclasses import dataclass
from functools import total_ordering
from typing import NamedTuple, TypedDict

class Plain: ...
class Context[T]: ...

@total_ordering
class Ordered[T]:
    def __lt__(self, other: object) -> bool:
        return False

@dataclass
class Data:
    value: int

class Named(NamedTuple):
    value: int

class Typed(TypedDict):
    value: int
"#,
    )?;
    Ok(db)
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/synthesis.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("expected a static class for {name}"))
}

fn request<'a, 'db>(
    class: StaticClassLiteral<'db>,
    name: &'a str,
) -> OwnMemberLookupRequest<'a, 'db> {
    OwnMemberLookupRequest {
        class,
        name,
        inherited_generic_context: None,
        specialization: None,
    }
}

#[derive(Debug, Eq, PartialEq)]
struct RecordedRequest<'db> {
    class: StaticClassLiteral<'db>,
    name: String,
    inherited_generic_context: Option<GenericContext<'db>>,
    specialization: Option<Specialization<'db>>,
}

impl<'db> From<OwnMemberLookupRequest<'_, 'db>> for RecordedRequest<'db> {
    fn from(request: OwnMemberLookupRequest<'_, 'db>) -> Self {
        Self {
            class: request.class,
            name: request.name.to_owned(),
            inherited_generic_context: request.inherited_generic_context,
            specialization: request.specialization,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Event<'db> {
    Checkpoint(SynthesizedMemberWork),
    Ordering,
    Frozen(&'static str),
    CodeGenerator(StaticClassLiteral<'db>),
    Generated(CodeGeneratorKind<'db>),
}

struct RecordingEffects<'db> {
    db: &'db TestDb,
    request: RecordedRequest<'db>,
    events: RefCell<Vec<Event<'db>>>,
    ordering: Result<Option<Type<'db>>, &'static str>,
    frozen: Result<Option<Type<'db>>, &'static str>,
    generator: Result<Option<CodeGeneratorKind<'db>>, &'static str>,
    generated: Result<Option<Type<'db>>, &'static str>,
    rejected_work: Option<SynthesizedMemberWork>,
}

impl<'db> RecordingEffects<'db> {
    fn new(db: &'db TestDb, request: OwnMemberLookupRequest<'_, 'db>) -> Self {
        Self {
            db,
            request: request.into(),
            events: RefCell::new(Vec::new()),
            ordering: Err("ordering member"),
            frozen: Err("frozen member"),
            generator: Ok(None),
            generated: Err("generated member"),
            rejected_work: None,
        }
    }

    fn record_request(&self, request: OwnMemberLookupRequest<'_, 'db>, event: Event<'db>) {
        assert_eq!(RecordedRequest::from(request), self.request);
        self.events.borrow_mut().push(event);
    }
}

impl sealed::Sealed for RecordingEffects<'_> {}

impl<'db> SynthesizedMemberEffects<'db> for RecordingEffects<'db> {
    async fn total_ordering(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.total_ordering(self.db))
    }
    type Error = &'static str;

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        assert_eq!(class, self.request.class);
        self.events.borrow_mut().push(Event::CodeGenerator(class));
        self.generator
    }
    async fn checkpoint(&self, work: SynthesizedMemberWork) -> Result<(), Self::Error> {
        self.events.borrow_mut().push(Event::Checkpoint(work));
        if self.rejected_work == Some(work) {
            Err("checkpoint")
        } else {
            Ok(())
        }
    }

    async fn total_ordering_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record_request(request, Event::Ordering);
        self.ordering
    }

    async fn frozen_subclass_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        method: FrozenDataclassMethod,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let name = match method {
            FrozenDataclassMethod::SetAttr => "__setattr__",
            FrozenDataclassMethod::DelAttr => "__delattr__",
        };
        self.record_request(request, Event::Frozen(name));
        self.frozen
    }

    async fn generated_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        generator: CodeGeneratorKind<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record_request(request, Event::Generated(generator));
        self.generated
    }
}

#[test]
fn ordering_success_returns_before_generator_lookup() -> anyhow::Result<()> {
    let db = database()?;
    let ordered = class(&db, "Ordered")?;
    assert!(ordered.total_ordering(&db));
    for name in ["__lt__", "__le__", "__gt__", "__ge__"] {
        let request = request(ordered, name);
        let mut effects = RecordingEffects::new(&db, request);
        effects.ordering = Ok(Some(Type::int_literal(11)));
        effects.generator = Err("missing generator");
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Ok(Some(Type::int_literal(11)))),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission {
                    name_bytes: name.len(),
                }),
                Event::Checkpoint(SynthesizedMemberWork::OrderingRequest),
                Event::Ordering,
                Event::Checkpoint(SynthesizedMemberWork::Publish),
            ],
        );
    }
    Ok(())
}

#[test]
fn ordering_absence_continues_with_the_original_request() -> anyhow::Result<()> {
    let db = database()?;
    let ordered = class(&db, "Ordered")?;
    let context = ordered
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Ordered must have a generic context"))?;
    let inherited = class(&db, "Context")?.generic_context(&db);
    let specialization = context.specialize(&db, [Type::int_literal(7)].as_slice());
    for supplied in [None, Some(specialization)] {
        let request = OwnMemberLookupRequest {
            class: ordered,
            name: "__le__",
            inherited_generic_context: inherited,
            specialization: supplied,
        };
        let mut effects = RecordingEffects::new(&db, request);
        effects.ordering = Ok(None);
        effects.generator = Ok(Some(CodeGeneratorKind::DataclassLike(None)));
        effects.generated = Ok(Some(Type::int_literal(29)));
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Ok(Some(Type::int_literal(29)))),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission { name_bytes: 6 }),
                Event::Checkpoint(SynthesizedMemberWork::OrderingRequest),
                Event::Ordering,
                Event::Checkpoint(SynthesizedMemberWork::CodeGenerator),
                Event::CodeGenerator(ordered),
                Event::Checkpoint(SynthesizedMemberWork::GeneratedRequest),
                Event::Generated(CodeGeneratorKind::DataclassLike(None)),
                Event::Checkpoint(SynthesizedMemberWork::Publish),
            ],
        );
    }
    Ok(())
}

#[test]
fn frozen_success_returns_before_generator_lookup() -> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db, "Plain")?;
    for name in ["__setattr__", "__delattr__"] {
        let request = request(plain, name);
        let mut effects = RecordingEffects::new(&db, request);
        effects.frozen = Ok(Some(Type::int_literal(13)));
        effects.generator = Err("missing generator");
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Ok(Some(Type::int_literal(13)))),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission {
                    name_bytes: name.len(),
                }),
                Event::Checkpoint(SynthesizedMemberWork::FrozenRequest),
                Event::Frozen(name),
                Event::Checkpoint(SynthesizedMemberWork::Publish),
            ],
        );
    }
    Ok(())
}

#[test]
fn frozen_absence_continues_to_the_generator() -> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db, "Plain")?;
    for name in ["__setattr__", "__delattr__"] {
        let request = request(plain, name);
        let mut effects = RecordingEffects::new(&db, request);
        effects.frozen = Ok(None);
        effects.generator = Ok(Some(CodeGeneratorKind::DataclassLike(None)));
        effects.generated = Ok(None);
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Ok(None)),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission {
                    name_bytes: name.len(),
                }),
                Event::Checkpoint(SynthesizedMemberWork::FrozenRequest),
                Event::Frozen(name),
                Event::Checkpoint(SynthesizedMemberWork::CodeGenerator),
                Event::CodeGenerator(plain),
                Event::Checkpoint(SynthesizedMemberWork::GeneratedRequest),
                Event::Generated(CodeGeneratorKind::DataclassLike(None)),
                Event::Checkpoint(SynthesizedMemberWork::Publish),
            ],
        );
    }
    Ok(())
}

#[test]
fn ordering_requires_both_the_decorator_and_an_ordering_name() -> anyhow::Result<()> {
    let db = database()?;
    for (class_name, name) in [("Ordered", "__init__"), ("Plain", "__lt__")] {
        let class = class(&db, class_name)?;
        let request = request(class, name);
        let effects = RecordingEffects::new(&db, request);
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Ok(None)),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission {
                    name_bytes: name.len(),
                }),
                Event::Checkpoint(SynthesizedMemberWork::CodeGenerator),
                Event::CodeGenerator(class),
                Event::Checkpoint(SynthesizedMemberWork::Publish),
            ],
        );
    }
    Ok(())
}

#[test]
fn dependency_failures_do_not_become_absence() -> anyhow::Result<()> {
    let db = database()?;
    for (class_name, name, work, event, error) in [
        (
            "Ordered",
            "__le__",
            SynthesizedMemberWork::OrderingRequest,
            Event::Ordering,
            "ordering member",
        ),
        (
            "Plain",
            "__setattr__",
            SynthesizedMemberWork::FrozenRequest,
            Event::Frozen("__setattr__"),
            "frozen member",
        ),
        (
            "Plain",
            "__delattr__",
            SynthesizedMemberWork::FrozenRequest,
            Event::Frozen("__delattr__"),
            "frozen member",
        ),
    ] {
        let class = class(&db, class_name)?;
        let request = request(class, name);
        let effects = RecordingEffects::new(&db, request);
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Err(error)),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission {
                    name_bytes: name.len(),
                }),
                Event::Checkpoint(work),
                event,
            ],
        );
    }
    Ok(())
}

#[test]
fn missing_generator_is_reported_after_successful_earlier_guards() -> anyhow::Result<()> {
    let db = database()?;
    let ordered = class(&db, "Ordered")?;
    let request = request(ordered, "__le__");
    let mut effects = RecordingEffects::new(&db, request);
    effects.ordering = Ok(None);
    effects.generator = Err("missing generator");
    assert_eq!(
        try_poll_immediate(own_synthesized_member_with(
            request,
            &effects
        )),
        Poll::Ready(Err("missing generator")),
    );
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Checkpoint(SynthesizedMemberWork::Admission { name_bytes: 6 }),
            Event::Checkpoint(SynthesizedMemberWork::OrderingRequest),
            Event::Ordering,
            Event::Checkpoint(SynthesizedMemberWork::CodeGenerator),
            Event::CodeGenerator(ordered),
        ],
    );
    Ok(())
}

#[test]
fn selected_generators_reach_the_dependency_even_for_an_absent_name() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    for (class_name, generator) in [
        ("Data", CodeGeneratorKind::DataclassLike(None)),
        ("Named", CodeGeneratorKind::NamedTuple),
        ("Typed", CodeGeneratorKind::TypedDict),
    ] {
        let class = class(&db, class_name)?;
        let name = "absent_member";
        assert_eq!(
            CodeGeneratorKind::from_class(&db, class.into()),
            Some(generator),
        );
        assert_eq!(
            class.own_synthesized_member(&db, &env, None, None, name),
            None,
        );
        let request = request(class, name);
        let mut effects = RecordingEffects::new(&db, request);
        effects.generator = Ok(Some(generator));
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Err("generated member")),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission {
                    name_bytes: name.len(),
                }),
                Event::Checkpoint(SynthesizedMemberWork::CodeGenerator),
                Event::CodeGenerator(class),
                Event::Checkpoint(SynthesizedMemberWork::GeneratedRequest),
                Event::Generated(generator),
            ],
        );
    }
    Ok(())
}

#[test]
fn failed_admission_does_not_request_dependencies() -> anyhow::Result<()> {
    let db = database()?;
    let request = request(class(&db, "Ordered")?, "__le__");
    let mut effects = RecordingEffects::new(&db, request);
    effects.rejected_work = Some(SynthesizedMemberWork::Admission { name_bytes: 6 });
    assert_eq!(
        try_poll_immediate(own_synthesized_member_with(
            request,
            &effects
        )),
        Poll::Ready(Err("checkpoint")),
    );
    assert_eq!(
        *effects.events.borrow(),
        [Event::Checkpoint(SynthesizedMemberWork::Admission {
            name_bytes: 6,
        })],
    );
    Ok(())
}

#[test]
fn publication_failure_rejects_both_presence_and_absence() -> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db, "Plain")?;
    for member in [None, Some(Type::int_literal(41))] {
        let request = request(plain, "member");
        let mut effects = RecordingEffects::new(&db, request);
        effects.generator = Ok(Some(CodeGeneratorKind::DataclassLike(None)));
        effects.generated = Ok(member);
        effects.rejected_work = Some(SynthesizedMemberWork::Publish);
        assert_eq!(
            try_poll_immediate(own_synthesized_member_with(
                request,
                &effects
            )),
            Poll::Ready(Err("checkpoint")),
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint(SynthesizedMemberWork::Admission { name_bytes: 6 }),
                Event::Checkpoint(SynthesizedMemberWork::CodeGenerator),
                Event::CodeGenerator(plain),
                Event::Checkpoint(SynthesizedMemberWork::GeneratedRequest),
                Event::Generated(CodeGeneratorKind::DataclassLike(None)),
                Event::Checkpoint(SynthesizedMemberWork::Publish),
            ],
        );
    }
    Ok(())
}
