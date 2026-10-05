use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::Type;
use crate::types::signatures::effects::try_poll_immediate;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/layout.py",
            r#"
from typing import Any, Protocol

def choose() -> tuple[str, ...]: ...

class Base:
    __slots__ = ("shared", "base")
class Child(Base):
    __slots__ = ("child", "shared")
class WithDictionary(Child):
    __slots__ = ("__dict__",)
class Plain: ...
class PlainChild(Plain):
    __slots__ = ()
class Unknown:
    __slots__ = choose()
class UnknownChild(Unknown):
    __slots__ = ("value",)
class DictionaryBeforeUnknown(Unknown):
    __slots__ = ("__dict__",)
class DictionaryAfterUnknown(Unknown, Plain):
    __slots__ = ()
class AnyBase(Any):
    __slots__ = ("value",)
class Empty:
    __slots__ = ()
class IntChild(int):
    __slots__ = ()
class ExceptionChild(Exception):
    __slots__ = ()
class Interface(Protocol): ...
"#,
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/layout.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(|class| class.as_static())
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

fn immediate<T>(future: impl Future<Output = Result<T, &'static str>>) -> Result<T, &'static str> {
    match try_poll_immediate(future) {
        Poll::Ready(result) => result,
        Poll::Pending => Err("recording layout unexpectedly suspended"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event<'db> {
    Checkpoint,
    Protocol(StaticClassLiteral<'db>),
    Unknown,
    NewSlots,
    StartMro(StaticClassLiteral<'db>),
    NextBase,
    ClassLiteral(ClassType<'db>),
    SlotNames(StaticClassLiteral<'db>),
    NextName,
    DictionaryName(Name),
    InsertSlot(Name),
    ExplicitSlots(StaticClassLiteral<'db>),
    Known(StaticClassLiteral<'db>),
    Finish(InstanceDictionary),
}

struct OwnedSlots {
    names: FxIndexSet<Name>,
    discarded: Rc<RefCell<Vec<Vec<Name>>>>,
    completed: bool,
}

impl Drop for OwnedSlots {
    fn drop(&mut self) {
        if !self.completed {
            self.discarded
                .borrow_mut()
                .push(self.names.iter().cloned().collect());
        }
    }
}

struct Recording<'db> {
    db: &'db TestDb,
    events: RefCell<Vec<Event<'db>>>,
    discarded: Rc<RefCell<Vec<Vec<Name>>>>,
    finished: Cell<bool>,
    reject: Option<usize>,
}

impl<'db> Recording<'db> {
    fn new(db: &'db TestDb) -> Self {
        Self {
            db,
            events: RefCell::default(),
            discarded: Rc::default(),
            finished: Cell::new(false),
            reject: None,
        }
    }

    fn record(&self, event: Event<'db>) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        events.push(event);
        if self.reject == Some(events.len() - 1) {
            Err("refused layout")
        } else {
            Ok(())
        }
    }
}

macro_rules! recording_effects {
    ($trait_name:ident $(, $async:tt)?) => {
        impl<'db> $trait_name<'db> for Recording<'db> {
            type Error = &'static str;
            type MroCursor = MroIterator<'db>;
            type Slots = OwnedSlots;
            type NamesCursor = std::slice::Iter<'db, Name>;

            $($async)? fn checkpoint(&self) -> Result<(), Self::Error> {
                self.record(Event::Checkpoint)
            }

            $($async)? fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
                self.record(Event::Protocol(class))?;
                Ok(class.is_protocol(self.db))
            }

            $($async)? fn unknown(&self) -> Result<InstanceLayout, Self::Error> {
                self.record(Event::Unknown)?;
                Ok(InstanceLayout::unknown())
            }

            $($async)? fn new_slots(&self) -> Result<Self::Slots, Self::Error> {
                self.record(Event::NewSlots)?;
                Ok(OwnedSlots {
                    names: FxIndexSet::default(),
                    discarded: self.discarded.clone(),
                    completed: false,
                })
            }

            $($async)? fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::MroCursor, Self::Error> {
                self.record(Event::StartMro(class))?;
                Ok(class.iter_mro(self.db, None))
            }

            $($async)? fn next_mro_base(&self, cursor: &mut Self::MroCursor) -> Result<Option<ClassBase<'db>>, Self::Error> {
                self.record(Event::NextBase)?;
                Ok(cursor.next())
            }

            $($async)? fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error> {
                self.record(Event::ClassLiteral(class))?;
                Ok(class.class_literal(self.db))
            }

            $($async)? fn slot_names(&self, class: StaticClassLiteral<'db>) -> Result<Option<Self::NamesCursor>, Self::Error> {
                self.record(Event::SlotNames(class))?;
                Ok(class.slot_names(self.db).map(<[Name]>::iter))
            }

            $($async)? fn next_name(&self, cursor: &mut Self::NamesCursor) -> Result<Option<&'db Name>, Self::Error> {
                self.record(Event::NextName)?;
                Ok(cursor.next())
            }

            $($async)? fn is_dictionary_name(&self, name: &Name) -> Result<bool, Self::Error> {
                self.record(Event::DictionaryName(name.clone()))?;
                Ok(name == "__dict__")
            }

            $($async)? fn insert_slot(&self, slots: &mut Self::Slots, name: &Name) -> Result<(), Self::Error> {
                self.record(Event::InsertSlot(name.clone()))?;
                slots.names.insert(name.clone());
                Ok(())
            }

            $($async)? fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
                self.record(Event::ExplicitSlots(class))?;
                Ok(class.has_explicit_slots(self.db))
            }

            $($async)? fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
                self.record(Event::Known(class))?;
                Ok(class.known(self.db))
            }

            $($async)? fn finish(&self, mut slots: Self::Slots, dictionary: InstanceDictionary) -> Result<InstanceLayout, Self::Error> {
                self.record(Event::Finish(dictionary))?;
                self.finished.set(true);
                slots.completed = true;
                Ok(finish_slots(std::mem::take(&mut slots.names), dictionary))
            }
        }
    };
}

recording_effects!(InstanceLayoutEffects, async);
recording_effects!(SynchronousInstanceLayoutEffects);

#[test]
fn inherited_slots_keep_first_occurrence_in_mro_order() -> anyhow::Result<()> {
    let db = database()?;
    let owner = class(&db, "WithDictionary")?;
    let asynchronous = Recording::new(&db);
    let synchronous = Recording::new(&db);
    let result = immediate(instance_layout_with(
        owner,
        InstanceLayoutFacts,
        &asynchronous,
    ))
    .map_err(anyhow::Error::msg)?;
    assert_eq!(
        result,
        instance_layout_sync(owner, InstanceLayoutFacts, &synchronous)
            .map_err(anyhow::Error::msg)?,
    );
    assert_eq!(
        result.slots.as_ref(),
        ["__dict__", "child", "shared", "base"]
    );
    assert_eq!(result.dictionary, InstanceDictionary::Present);
    assert_eq!(
        asynchronous.events.borrow().as_slice(),
        synchronous.events.borrow().as_slice()
    );

    let slot_owners: Vec<_> = asynchronous
        .events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::SlotNames(class) if class.known(&db) != Some(KnownClass::Object) => Some(*class),
            _ => None,
        })
        .collect();
    assert_eq!(
        slot_owners,
        [owner, class(&db, "Child")?, class(&db, "Base")?]
    );
    let inserted: Vec<_> = asynchronous
        .events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::InsertSlot(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(inserted, ["__dict__", "child", "shared", "shared", "base"]);
    assert!(asynchronous.discarded.borrow().is_empty());
    assert!(synchronous.discarded.borrow().is_empty());
    Ok(())
}

#[test]
fn dictionary_inheritance_matches_in_both_executions() -> anyhow::Result<()> {
    let db = database()?;
    for (name, slots, dictionary) in [
        ("Empty", &[][..], InstanceDictionary::Absent),
        ("IntChild", &[][..], InstanceDictionary::Absent),
        ("PlainChild", &[][..], InstanceDictionary::Present),
        ("ExceptionChild", &[][..], InstanceDictionary::Present),
        ("UnknownChild", &["value"][..], InstanceDictionary::Unknown),
        ("AnyBase", &["value"][..], InstanceDictionary::Unknown),
        (
            "DictionaryBeforeUnknown",
            &["__dict__"][..],
            InstanceDictionary::Present,
        ),
        (
            "DictionaryAfterUnknown",
            &[][..],
            InstanceDictionary::Present,
        ),
        // A protocol can be implemented by both slotted and unslotted classes.
        ("Interface", &[][..], InstanceDictionary::Unknown),
    ] {
        let owner = class(&db, name)?;
        let asynchronous = Recording::new(&db);
        let synchronous = Recording::new(&db);
        let result = immediate(instance_layout_with(
            owner,
            InstanceLayoutFacts,
            &asynchronous,
        ))
        .map_err(anyhow::Error::msg)?;
        assert_eq!(
            result,
            instance_layout_sync(owner, InstanceLayoutFacts, &synchronous)
                .map_err(anyhow::Error::msg)?,
            "{name}",
        );
        assert_eq!(result.slots.as_ref(), slots, "{name}");
        assert_eq!(result.dictionary, dictionary, "{name}");
        assert_eq!(
            asynchronous.events.borrow().as_slice(),
            synchronous.events.borrow().as_slice(),
            "{name}"
        );
        if name == "Interface" {
            assert_eq!(
                asynchronous.events.borrow().as_slice(),
                [Event::Checkpoint, Event::Protocol(owner), Event::Unknown]
            );
        }
    }
    Ok(())
}

#[test]
fn refusal_after_insertion_discards_owned_slots_and_stops_effects() -> anyhow::Result<()> {
    let db = database()?;
    let owner = class(&db, "Child")?;
    let complete = Recording::new(&db);
    immediate(instance_layout_with(owner, InstanceLayoutFacts, &complete))
        .map_err(anyhow::Error::msg)?;
    let events = complete.events.into_inner();
    let first_insert = events
        .iter()
        .position(|event| matches!(event, Event::InsertSlot(_)))
        .ok_or_else(|| anyhow::anyhow!("slotted class produced no slot insertion"))?;

    for rejected in first_insert + 1..events.len() {
        let mut asynchronous = Recording::new(&db);
        asynchronous.reject = Some(rejected);
        let mut synchronous = Recording::new(&db);
        synchronous.reject = Some(rejected);
        assert_eq!(
            immediate(instance_layout_with(
                owner,
                InstanceLayoutFacts,
                &asynchronous
            )),
            Err("refused layout"),
        );
        assert_eq!(
            instance_layout_sync(owner, InstanceLayoutFacts, &synchronous),
            Err("refused layout"),
        );
        // Operations are recorded before they can refuse, so a rejected insertion appears
        // in the trace without adding its name to the slots that must be discarded.
        let inserted: FxIndexSet<_> = events[..rejected]
            .iter()
            .filter_map(|event| match event {
                Event::InsertSlot(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let expected_discarded: Vec<_> = inserted.into_iter().collect();
        for effects in [&asynchronous, &synchronous] {
            assert_eq!(effects.events.borrow().as_slice(), &events[..=rejected]);
            assert_eq!(
                effects.discarded.borrow().as_slice(),
                [expected_discarded.clone()]
            );
            assert!(!effects.finished.get());
        }
    }
    Ok(())
}
