use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::future::ready;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{
    MroCursor, MroDirection, MroIterationEffects, MroIterationWork, full_mro_with, mro_next_sync,
    mro_next_with,
};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::mro::root::{
    InlineMroRootEffects, MroRootEffects, MroRootFacts, MroRootWork, MroTailRequest,
    mro_first_sync, mro_tail_request_sync, sealed,
};
use crate::types::mro::{Mro, MroIterator};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::source_read::{SourceReadControl, UnrestrictedSourceRead};
use crate::types::{ClassLiteral, ClassType, KnownClass, StaticClassLiteral, Type};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/iteration.py",
            r#"
from enum import Enum
from typing import NamedTuple, TypedDict

class Owner: ...
class Child(Owner): ...
class Grandchild(Child): ...
class Generic[T]: ...
class GenericChild[U](Generic[U]): ...
Specialized = GenericChild[Owner]
class Invalid(1): ...
Dynamic = type("Dynamic", (Owner,), {})
Named = NamedTuple("Named", [("value", int)])
Typed = TypedDict("Typed", {"value": int})
Enumeration = Enum("Enumeration", {"VALUE": 1})
"#,
        )
        .build()
}

fn request<'db>(
    db: &'db TestDb,
    name: &str,
) -> anyhow::Result<(ClassLiteral<'db>, Option<Specialization<'db>>)> {
    let env = db.program_environment();
    let ty = if name == "object" {
        KnownClass::Object.to_class_literal(db, &env)
    } else {
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/iteration.py")?,
            env.program(db),
        );
        global_symbol(db, file, name).place.expect_type()
    };
    match ty {
        Type::ClassLiteral(class) => Ok((class, None)),
        Type::GenericAlias(alias) => Ok((alias.origin(db).into(), Some(alias.specialization(db)))),
        _ => anyhow::bail!("{name} is not a class or a specialized class"),
    }
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

// Retain the iterator decisions before cursor extraction. The root helpers are already
// independently covered; this oracle must not delegate advancement or source dispatch to the cursor.
#[derive(Clone)]
struct OriginalIterator<'db> {
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    first_element_yielded: bool,
    subsequent_elements: Option<std::slice::Iter<'db, ClassBase<'db>>>,
}

impl<'db> OriginalIterator<'db> {
    fn new(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        Self {
            db,
            class,
            specialization,
            first_element_yielded: false,
            subsequent_elements: None,
        }
    }

    fn first_element(&self) -> ClassBase<'db> {
        infallible(mro_first_sync(
            self.db,
            self.class,
            self.specialization,
            &InlineMroRootEffects::new(self.db),
        ))
    }

    fn full_mro_except_first_element(&mut self) -> &mut std::slice::Iter<'db, ClassBase<'db>> {
        let db = self.db;
        self.subsequent_elements.get_or_insert_with(|| {
            let request = infallible(mro_tail_request_sync(
                db,
                self.class,
                self.specialization,
                &InlineMroRootEffects::new(db),
            ));
            let mut full_mro_iter = match request {
                MroTailRequest::Static(literal, specialization) => {
                    match literal.try_mro(db, specialization) {
                        Ok(mro) => mro.iter(),
                        Err(error) => error.fallback_mro().iter(),
                    }
                }
                MroTailRequest::Dynamic(literal) => match literal.try_mro(db) {
                    Ok(mro) => mro.iter(),
                    Err(error) => error.fallback_mro().iter(),
                },
                MroTailRequest::DynamicNamedTuple(literal) => literal.mro(db).iter(),
                MroTailRequest::DynamicTypedDict(literal) => literal.mro(db).iter(),
                MroTailRequest::DynamicEnum(literal) => match literal.try_mro(db) {
                    Ok(mro) => mro.iter(),
                    Err(error) => error.fallback_mro().iter(),
                },
            };
            full_mro_iter.next();
            full_mro_iter
        })
    }
}

impl<'db> Iterator for OriginalIterator<'db> {
    type Item = ClassBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.first_element_yielded {
            self.first_element_yielded = true;
            return Some(self.first_element());
        }
        self.full_mro_except_first_element().next().copied()
    }
}

impl DoubleEndedIterator for OriginalIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.full_mro_except_first_element()
            .next_back()
            .copied()
            .or_else(|| {
                if self.first_element_yielded {
                    None
                } else {
                    self.first_element_yielded = true;
                    Some(self.first_element())
                }
            })
    }
}

fn advance<I: DoubleEndedIterator>(iterator: &mut I, direction: MroDirection) -> Option<I::Item> {
    match direction {
        MroDirection::Forward => iterator.next(),
        MroDirection::Reverse => iterator.next_back(),
    }
}

fn directions(pattern: usize) -> [MroDirection; 16] {
    std::array::from_fn(|index| match pattern {
        0 => MroDirection::Forward,
        1 => MroDirection::Reverse,
        _ if index % 2 == 0 => MroDirection::Forward,
        _ => MroDirection::Reverse,
    })
}

#[test]
fn ordinary_iterator_and_cursor_match_original_items_clones_and_exhaustion() -> anyhow::Result<()> {
    let db = database()?;
    for name in [
        "object",
        "Grandchild",
        "GenericChild",
        "Specialized",
        "Dynamic",
        "Invalid",
        "Named",
        "Typed",
        "Enumeration",
    ] {
        let (class, specialization) = request(&db, name)?;
        if name == "Specialized" {
            assert!(specialization.is_some());
        }
        if name == "Dynamic" {
            assert!(matches!(class, ClassLiteral::Dynamic(_)));
        }
        for pattern in 0..3 {
            let mut original = OriginalIterator::new(&db, class, specialization);
            let mut ordinary = MroIterator::new(&db, class, specialization);
            let mut cursor = MroCursor::new(class, specialization);
            let mut async_cursor = MroCursor::new(class, specialization);
            let async_effects = Recording::new(&db, None);
            for (index, direction) in directions(pattern).into_iter().enumerate() {
                if [0, 1, 2, 5, 15].contains(&index) {
                    let mut original_clone = original.clone();
                    let mut ordinary_clone = ordinary.clone();
                    let mut cursor_clone = cursor.clone();
                    for direction in directions((pattern + 1) % 3) {
                        let expected = advance(&mut original_clone, direction);
                        assert_eq!(
                            advance(&mut ordinary_clone, direction),
                            expected,
                            "{name}: clone"
                        );
                        assert_eq!(
                            infallible(mro_next_sync(
                                &db,
                                &mut cursor_clone,
                                direction,
                                &InlineMroRootEffects::new(&db)
                            )),
                            expected,
                            "{name}: cursor clone",
                        );
                    }
                }
                let expected = advance(&mut original, direction);
                assert_eq!(
                    advance(&mut ordinary, direction),
                    expected,
                    "{name}: {index}"
                );
                assert_eq!(
                    infallible(mro_next_sync(
                        &db,
                        &mut cursor,
                        direction,
                        &InlineMroRootEffects::new(&db)
                    )),
                    expected,
                    "{name}: cursor {index}",
                );
                assert_eq!(
                    try_poll_immediate(mro_next_with(
                        crate::types::mro::field_reads::MroFieldReads::new(&db),
                        &mut async_cursor,
                        direction,
                        &async_effects
                    )),
                    Poll::Ready(Ok(expected)),
                    "{name}: async cursor {index}",
                );
                if index == 15 {
                    assert_eq!(expected, None, "fixture exceeded the bounded walk: {name}");
                }
            }
        }
    }
    let (invalid, _) = request(&db, "Invalid")?;
    let expected = [
        ClassBase::Class(ClassType::NonGeneric(invalid)),
        ClassBase::unknown(),
        ClassBase::object(&db, &db.program_environment()),
    ];
    assert_eq!(
        MroIterator::new(&db, invalid, None).collect::<Vec<_>>(),
        expected
    );
    Ok(())
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            )
        })
        .collect()
}

fn cold_trace(name: &str, pattern: usize, original: bool) -> anyhow::Result<Vec<Vec<String>>> {
    let db = database()?;
    let (class, specialization) = request(&db, name)?;
    let mut old = OriginalIterator::new(&db, class, specialization);
    let mut new = MroIterator::new(&db, class, specialization);
    executions(&db);
    let mut trace = Vec::new();
    for direction in directions(pattern) {
        if original {
            advance(&mut old, direction);
        } else {
            advance(&mut new, direction);
        }
        trace.push(executions(&db));
    }
    Ok(trace)
}

#[test]
fn cold_source_order_is_independent_of_item_equality() -> anyhow::Result<()> {
    for name in [
        "object",
        "Grandchild",
        "GenericChild",
        "Specialized",
        "Dynamic",
        "Invalid",
    ] {
        for pattern in 0..3 {
            let expected = cold_trace(name, pattern, true)?;
            let actual = cold_trace(name, pattern, false)?;
            assert_eq!(actual, expected, "{name}: direction pattern {pattern}");
            if name == "Grandchild" {
                let first_mro = actual
                    .iter()
                    .position(|batch| {
                        batch
                            .iter()
                            .any(|name| name.contains("try_mro_unspecialized"))
                    })
                    .ok_or_else(|| anyhow::anyhow!("missing cold MRO query: {actual:?}"))?;
                assert_eq!(first_mro, usize::from(pattern != 1));
            }
            assert!(actual[15].is_empty(), "exhaustion reentered source: {name}");
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Iteration(MroIterationWork),
    Root(&'static str),
    Context(StaticClassLiteral<'db>),
    Default,
    Normalize,
    FullRequest(ClassLiteral<'db>, Option<Specialization<'db>>),
    SourceBefore,
    SourceAfter,
    FullReady,
    Item(Option<ClassBase<'db>>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

struct Recording<'db> {
    db: &'db dyn Db,
    events: RefCell<Vec<Event<'db>>>,
    refuse_at: Option<usize>,
}

impl<'db> Recording<'db> {
    fn new(db: &'db dyn Db, refuse_at: Option<usize>) -> Self {
        Self {
            db,
            events: RefCell::new(Vec::new()),
            refuse_at,
        }
    }

    fn record(&self, event: Event<'db>) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse_at == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }
}

impl sealed::Sealed for Recording<'_> {}

impl<'db> MroRootFacts<'db> for Recording<'db> {
    type Error = Refused;
}

impl<'db> MroRootEffects<'db> for Recording<'db> {
    async fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Refused> {
        self.record(Event::Context(class))?;
        Ok(class.generic_context(self.db))
    }

    async fn checkpoint(&self, work: MroRootWork) -> Result<(), Refused> {
        ready(self.record(Event::Root(match work {
            MroRootWork::GenericContext => "context",
            MroRootWork::DefaultSpecialization => "default",
            MroRootWork::TupleRuntimeSpecialization => "tuple",
            MroRootWork::GenericAlias => "alias",
        })))
        .await
    }

    async fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Refused> {
        self.record(Event::Default)?;
        ready(Ok(class.default_specialization(self.db))).await
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Refused> {
        self.record(Event::Normalize)?;
        ready(Ok(
            specialization.tuple_runtime_element_specialization(self.db)
        ))
        .await
    }
}

fn request_key(request: MroTailRequest<'_>) -> (ClassLiteral<'_>, Option<Specialization<'_>>) {
    match request {
        MroTailRequest::Static(class, specialization) => (class.into(), specialization),
        MroTailRequest::Dynamic(class) => (class.into(), None),
        MroTailRequest::DynamicNamedTuple(class) => (class.into(), None),
        MroTailRequest::DynamicTypedDict(class) => (class.into(), None),
        MroTailRequest::DynamicEnum(class) => (class.into(), None),
    }
}

struct Boundary<'a, 'db> {
    effects: &'a Recording<'db>,
    after: Cell<bool>,
}

impl SourceReadControl for Boundary<'_, '_> {
    type Error = Refused;

    fn check(&self) -> Result<(), Refused> {
        self.effects.record(if self.after.replace(true) {
            Event::SourceAfter
        } else {
            Event::SourceBefore
        })
    }
}

impl<'db> MroIterationEffects<'db> for Recording<'db> {
    async fn iteration_checkpoint(&self, work: MroIterationWork) -> Result<(), Refused> {
        ready(self.record(Event::Iteration(work))).await
    }

    async fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Refused> {
        let (class, specialization) = request_key(request);
        self.record(Event::FullRequest(class, specialization))?;
        let mro = full_mro_with(
            self.db,
            request,
            &Boundary {
                effects: self,
                after: Cell::new(false),
            },
        )?;
        self.record(Event::FullReady)?;
        ready(Ok(mro)).await
    }
}

fn recorded_walk<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    pattern: usize,
    effects: &Recording<'db>,
) -> Result<(), Refused> {
    let mut cursor = MroCursor::new(class, specialization);
    for direction in directions(pattern) {
        let Poll::Ready(result) = try_poll_immediate(mro_next_with(
            crate::types::mro::field_reads::MroFieldReads::new(db),
            &mut cursor,
            direction,
            effects,
        )) else {
            panic!("immediately ready iteration provider suspended");
        };
        effects.record(Event::Item(result?))?;
    }
    Ok(())
}

#[test]
fn each_iteration_and_source_checkpoint_stops_before_later_reads_or_items() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["object", "Grandchild", "Specialized", "Dynamic", "Invalid"] {
        let (class, specialization) = request(&db, name)?;
        for pattern in 0..3 {
            let baseline = Recording::new(&db, None);
            assert_eq!(
                recorded_walk(&db, class, specialization, pattern, &baseline),
                Ok(())
            );
            let expected = baseline.events.into_inner();
            assert!(
                expected
                    .iter()
                    .any(|event| matches!(event, Event::SourceAfter))
            );
            for (index, event) in expected.iter().enumerate() {
                if !matches!(
                    event,
                    Event::Iteration(_) | Event::SourceBefore | Event::SourceAfter
                ) {
                    continue;
                }
                let effects = Recording::new(&db, Some(index));
                assert_eq!(
                    recorded_walk(&db, class, specialization, pattern, &effects),
                    Err(Refused(index))
                );
                assert_eq!(
                    *effects.events.borrow(),
                    expected[..=index],
                    "{name}: {pattern}, {index}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn first_only_iteration_never_requests_a_tail_or_full_mro() -> anyhow::Result<()> {
    let db = database()?;
    for name in [
        "object",
        "Grandchild",
        "GenericChild",
        "Specialized",
        "Dynamic",
        "Invalid",
    ] {
        let (class, specialization) = request(&db, name)?;
        let mut original = OriginalIterator::new(&db, class, specialization);
        let mut cursor = MroCursor::new(class, specialization);
        let effects = Recording::new(&db, None);
        let expected = original.next();
        assert_eq!(
            try_poll_immediate(mro_next_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                &mut cursor,
                MroDirection::Forward,
                &effects
            )),
            Poll::Ready(Ok(expected))
        );
        assert!(cursor.first_element_yielded);
        assert!(cursor.subsequent_elements.is_none());
        assert!(!effects.events.borrow().iter().any(|event| matches!(
            event,
            Event::Iteration(MroIterationWork::TailRequest | MroIterationWork::FullMro)
                | Event::FullRequest(..)
                | Event::SourceBefore
                | Event::SourceAfter
        )));
    }
    Ok(())
}

#[test]
fn full_mro_source_refusal_prevents_consuming_success_and_invalid_fallback() -> anyhow::Result<()> {
    for name in ["Grandchild", "Invalid"] {
        for after in [false, true] {
            let db = database()?;
            let (class, specialization) = request(&db, name)?;
            let request = infallible(mro_tail_request_sync(
                &db,
                class,
                specialization,
                &InlineMroRootEffects::new(&db),
            ));
            executions(&db);
            let effects = Recording::new(&db, Some(usize::from(after)));
            let consumed = Cell::new(false);
            let result = full_mro_with(
                &db,
                request,
                &Boundary {
                    effects: &effects,
                    after: Cell::new(false),
                },
            )
            .inspect(|_| consumed.set(true));
            assert_eq!(result, Err(Refused(usize::from(after))));
            assert!(!consumed.get());
            assert_eq!(
                *effects.events.borrow(),
                if after {
                    vec![Event::SourceBefore, Event::SourceAfter]
                } else {
                    vec![Event::SourceBefore]
                }
            );
            let reads = executions(&db);
            assert_eq!(reads.is_empty(), !after);
            if after {
                assert!(
                    reads
                        .iter()
                        .any(|name| name.contains("try_mro_unspecialized")),
                    "{reads:?}"
                );
            }
            infallible(full_mro_with(&db, request, &UnrestrictedSourceRead));
            assert_eq!(executions(&db).is_empty(), after);
        }
    }
    Ok(())
}
