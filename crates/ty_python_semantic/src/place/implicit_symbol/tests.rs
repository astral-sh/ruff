use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::TypeQualifiers;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Checkpoint,
    Docstring,
    Version(PythonVersion),
    Special(SpecialModuleGlobal),
    Membership,
    Member,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

#[derive(Clone, Copy, Default)]
struct Answers<'db> {
    docstring: bool,
    py314: bool,
    member: Option<PlaceAndQualifiers<'db>>,
}

struct Recording<'db> {
    answers: Answers<'db>,
    events: RefCell<Vec<Event>>,
    refusal: Option<usize>,
    suspensions: Cell<usize>,
}

impl Recording<'_> {
    fn record(&self, event: Event) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refusal == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }

    async fn suspend_and_record(&self, event: Event) -> Result<(), Refused> {
        let mut pending = true;
        poll_fn(|context| {
            if pending {
                pending = false;
                self.suspensions.set(self.suspensions.get() + 1);
                context.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        self.record(event)
    }
}

macro_rules! recording_effects {
    ($trait:ident, [$($async:tt)*], $record:ident, [$($await:tt)*]) => {
        impl<'db> $trait<'db> for Recording<'db> {
            type Error = Refused;

            $($async)* fn symbol_checkpoint(
                &self, _db: &'db dyn Db, _file: ProgramFile<'db>, _name: &str,
            ) -> Result<(), Refused> {
                self.$record(Event::Checkpoint) $($await)*
            }

            $($async)* fn has_module_docstring(
                &self, _db: &'db dyn Db, _file: ProgramFile<'db>,
            ) -> Result<bool, Refused> {
                self.$record(Event::Docstring) $($await)*?;
                Ok(self.answers.docstring)
            }

            $($async)* fn python_version_at_least(
                &self, _db: &'db dyn Db, _file: ProgramFile<'db>, minimum: PythonVersion,
            ) -> Result<bool, Refused> {
                self.$record(Event::Version(minimum)) $($await)*?;
                Ok(self.answers.py314)
            }

            $($async)* fn special_type(
                &self, _db: &'db dyn Db, _file: ProgramFile<'db>, special: SpecialModuleGlobal,
            ) -> Result<Type<'db>, Refused> {
                self.$record(Event::Special(special)) $($await)*?;
                Ok(Type::int_literal(match special {
                    SpecialModuleGlobal::String => 1,
                    SpecialModuleGlobal::Bool => 2,
                    SpecialModuleGlobal::WarningRegistry => 3,
                    SpecialModuleGlobal::Annotate => 4,
                }))
            }

            $($async)* fn is_module_global(
                &self, _db: &'db dyn Db, _file: ProgramFile<'db>, _name: &str,
            ) -> Result<bool, Refused> {
                self.$record(Event::Membership) $($await)*?;
                Ok(self.answers.member.is_some())
            }

            $($async)* fn module_global_member(
                &self, _db: &'db dyn Db, _file: ProgramFile<'db>, _name: &str,
            ) -> Result<PlaceAndQualifiers<'db>, Refused> {
                self.$record(Event::Member) $($await)*?;
                Ok(self.answers.member.unwrap_or_default())
            }
        }
    };
}
recording_effects!(SynchronousModuleGlobalSymbolEffects, [], record, []);
recording_effects!(ModuleGlobalSymbolEffects, [async], suspend_and_record, [.await]);

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file("/src/implicit.py", "")
        .build()
}

fn finish<T>(future: impl Future<Output = T>) -> anyhow::Result<T> {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..16 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return Ok(result);
        }
    }
    anyhow::bail!("module-global selection did not finish after its effects resumed")
}

fn check<'db>(
    db: &'db TestDb,
    name: &str,
    answers: Answers<'db>,
    expected: PlaceAndQualifiers<'db>,
    events: &[Event],
) -> anyhow::Result<()> {
    let file = db.program_file(system_path_to_file(db, "/src/implicit.py")?);
    for asynchronous in [false, true] {
        for refusal in (0..events.len()).map(Some).chain([None]) {
            let effects = Recording {
                answers,
                events: RefCell::new(Vec::new()),
                refusal,
                suspensions: Cell::new(0),
            };
            let result = if asynchronous {
                finish(module_type_implicit_global_symbol_with(
                    db, file, name, &effects,
                ))?
            } else {
                module_type_implicit_global_symbol_sync(db, file, name, &effects)
            };
            let expected_result = refusal.map_or(Ok(expected), |index| Err(Refused(index)));
            assert_eq!(
                result, expected_result,
                "{name}: async={asynchronous}, {refusal:?}"
            );
            let completed = refusal.map_or(events.len(), |index| index + 1);
            assert_eq!(*effects.events.borrow(), events[..completed], "{name}");
            assert_eq!(
                effects.suspensions.get(),
                if asynchronous { completed } else { 0 }
            );
        }
    }
    Ok(())
}

#[test]
fn special_globals_preserve_types_definedness_and_lazy_reads() -> anyhow::Result<()> {
    let db = database()?;
    for (name, special, literal, definedness) in [
        (
            "__file__",
            SpecialModuleGlobal::String,
            1,
            Definedness::AlwaysDefined,
        ),
        (
            "__doc__",
            SpecialModuleGlobal::String,
            1,
            Definedness::AlwaysDefined,
        ),
        (
            "__debug__",
            SpecialModuleGlobal::Bool,
            2,
            Definedness::AlwaysDefined,
        ),
        (
            "__warningregistry__",
            SpecialModuleGlobal::WarningRegistry,
            3,
            Definedness::PossiblyUndefined,
        ),
        (
            "__annotate__",
            SpecialModuleGlobal::Annotate,
            4,
            Definedness::PossiblyUndefined,
        ),
    ] {
        let mut events = vec![Event::Checkpoint];
        if name == "__doc__" {
            events.push(Event::Docstring);
        } else if name == "__annotate__" {
            events.push(Event::Version(PythonVersion::PY314));
        }
        events.push(Event::Special(special));
        let expected = Place::Defined(
            DefinedPlace::new(Type::int_literal(literal)).with_definedness(definedness),
        )
        .into();
        check(
            &db,
            name,
            Answers {
                docstring: true,
                py314: true,
                member: None,
            },
            expected,
            &events,
        )?;
    }
    check(
        &db,
        "__builtins__",
        Answers::default(),
        Place::bound(Type::any()).into(),
        &[Event::Checkpoint],
    )
}

#[test]
fn absent_docstrings_and_older_annotation_versions_fall_through_to_members() -> anyhow::Result<()> {
    let db = database()?;
    let member = Place::Defined(
        DefinedPlace::new(Type::int_literal(9)).with_definedness(Definedness::PossiblyUndefined),
    )
    .with_qualifiers(TypeQualifiers::FINAL);
    for (name, read) in [
        ("__doc__", Some(Event::Docstring)),
        ("__annotate__", Some(Event::Version(PythonVersion::PY314))),
        ("__name__", None),
    ] {
        for member in [None, Some(member), Some(Place::Undefined.into())] {
            let mut events = vec![Event::Checkpoint];
            events.extend(read);
            events.push(Event::Membership);
            if member.is_some() {
                events.push(Event::Member);
            }
            check(
                &db,
                name,
                Answers {
                    member,
                    ..Answers::default()
                },
                member.unwrap_or_default(),
                &events,
            )?;
        }
    }
    Ok(())
}

#[test]
fn ordinary_membership_excludes_attributes_unavailable_inside_modules() -> anyhow::Result<()> {
    let db = database()?;
    let file = db.program_file(system_path_to_file(&db, "/src/implicit.py")?);
    let effects = OrdinaryModuleGlobalSymbolEffects;
    for (name, expected) in [
        ("__name__", true),
        ("__doc__", true),
        ("__dict__", false),
        ("__init__", false),
        ("__getattr__", false),
        ("property", false),
        ("missing_global", false),
    ] {
        assert_eq!(
            effects.is_module_global(&db, file, name),
            Ok(expected),
            "{name}"
        );
        if !expected {
            assert_eq!(
                module_type_implicit_global_symbol_sync(&db, file, name, &effects),
                Ok(Place::Undefined.into()),
                "{name}"
            );
        }
    }
    Ok(())
}
