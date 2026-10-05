use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::convert::Infallible;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{collect_start_sync, collect_start_with_root_sync};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::mro::Mro;
use crate::types::mro::base::{BaseMroStart, ClassMroStart};
use crate::types::mro::collection::{MroCollectionWork, SynchronousMroCollectionEffects};
use crate::types::mro::iteration::{
    MroIterationWork, SynchronousMroIterationEffects, full_mro_with,
};
use crate::types::mro::root::{
    InlineMroRootEffects, MroRootFacts, MroRootWork, MroTailRequest, SynchronousMroRootEffects,
    sealed,
};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{ClassType, StaticClassLiteral, Type};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/base_collection.py",
            r#"
class Root: ...
class Parent: ...
class Child(Parent): ...
class Generic[T]: ...
Specialized = Generic[Parent]
class Invalid(1): ...
Dynamic = type("Dynamic", (Parent,), {})
"#,
        )
        .build()
}

fn start<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<BaseMroStart<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/base_collection.py")?,
        env.program(db),
    );
    let start = match global_symbol(db, file, name).place.expect_type() {
        Type::ClassLiteral(class) => ClassMroStart {
            class,
            specialization: None,
        },
        Type::GenericAlias(alias) => ClassMroStart {
            class: alias.origin(db).into(),
            specialization: Some(alias.specialization(db)),
        },
        _ => anyhow::bail!("{name} must be a class or specialized class"),
    };
    Ok(BaseMroStart::Class(start))
}

fn root(db: &TestDb) -> anyhow::Result<ClassType<'_>> {
    let BaseMroStart::Class(start) = start(db, "Root")? else {
        anyhow::bail!("Root must be a class start");
    };
    Ok(ClassType::NonGeneric(start.class))
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn fixed_starts<'db>() -> [BaseMroStart<'db>; 2] {
    [
        BaseMroStart::Length2([ClassBase::Any, ClassBase::unknown()]),
        BaseMroStart::Length3([ClassBase::Protocol, ClassBase::Generic, ClassBase::Any]),
    ]
}

#[test]
fn raw_collection_and_root_order_match_iterator_collection() -> anyhow::Result<()> {
    let db = database()?;
    let root = root(&db)?;
    let effects = InlineMroRootEffects::new(&db);
    let mut starts = fixed_starts().to_vec();
    for name in ["Child", "Generic", "Specialized", "Dynamic", "Invalid"] {
        starts.push(start(&db, name)?);
    }
    for start in starts {
        let expected: VecDeque<_> = start.into_iter(&db).collect();
        assert_eq!(
            infallible(collect_start_sync(&db, start, &effects)),
            expected
        );
        let expected: Mro<'_> = std::iter::once(ClassBase::Class(root))
            .chain(start.into_iter(&db))
            .collect();
        assert_eq!(
            infallible(collect_start_with_root_sync(&db, root, start, &effects)),
            expected,
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Iteration(MroIterationWork),
    Collection(MroCollectionWork),
    Root(&'static str),
    Context,
    Default,
    Normalize,
    Full,
    SourceBefore,
    SourceAfter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

struct Recording<'db> {
    db: &'db dyn Db,
    events: RefCell<Vec<Event>>,
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

    fn record(&self, event: Event) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse_at == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }

    fn boundary(&self) -> Boundary<'_, 'db> {
        Boundary {
            effects: self,
            after: Cell::new(false),
        }
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

impl sealed::Sealed for Recording<'_> {}

impl<'db> MroRootFacts<'db> for Recording<'db> {
    type Error = Refused;
}

impl<'db> SynchronousMroRootEffects<'db> for Recording<'db> {
    fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Refused> {
        self.record(Event::Context)?;
        read_source(&self.boundary(), || class.generic_context(self.db))
    }

    fn checkpoint(&self, work: MroRootWork) -> Result<(), Refused> {
        self.record(Event::Root(match work {
            MroRootWork::GenericContext => "context",
            MroRootWork::DefaultSpecialization => "default",
            MroRootWork::TupleRuntimeSpecialization => "tuple",
            MroRootWork::GenericAlias => "alias",
        }))
    }

    fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Refused> {
        self.record(Event::Default)?;
        read_source(&self.boundary(), || class.default_specialization(self.db))
    }

    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Refused> {
        self.record(Event::Normalize)?;
        Ok(specialization.tuple_runtime_element_specialization(self.db))
    }
}

impl<'db> SynchronousMroIterationEffects<'db> for Recording<'db> {
    fn iteration_checkpoint(&self, work: MroIterationWork) -> Result<(), Refused> {
        self.record(Event::Iteration(work))
    }

    fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Refused> {
        self.record(Event::Full)?;
        full_mro_with(self.db, request, &self.boundary())
    }
}

impl<'db> SynchronousMroCollectionEffects<'db> for Recording<'db> {
    fn collection_checkpoint(&self, work: MroCollectionWork) -> Result<(), Refused> {
        self.record(Event::Collection(work))
    }
}

#[test]
fn every_collection_boundary_can_refuse_without_later_work() -> anyhow::Result<()> {
    let db = database()?;
    let root = root(&db)?;
    let mut starts = fixed_starts().to_vec();
    starts.push(start(&db, "Child")?);
    starts.push(start(&db, "Specialized")?);
    for start in starts {
        for with_root in [false, true] {
            let collect = |effects: &Recording<'_>| {
                if with_root {
                    collect_start_with_root_sync(&db, root, start, effects).map(|_| ())
                } else {
                    collect_start_sync(&db, start, effects).map(|_| ())
                }
            };
            let baseline = Recording::new(&db, None);
            assert_eq!(collect(&baseline), Ok(()));
            let expected = baseline.events.into_inner();
            assert_eq!(
                expected.last(),
                Some(&Event::Collection(MroCollectionWork::Publish))
            );
            if with_root {
                assert_eq!(
                    expected[0],
                    Event::Collection(MroCollectionWork::Append {
                        len: 0,
                        capacity: 0
                    })
                );
                assert!(matches!(
                    expected[expected.len() - 2],
                    Event::Collection(MroCollectionWork::BoxOutput { .. })
                ));
            }
            let array_len = match start {
                BaseMroStart::Length2(_) => Some(2),
                BaseMroStart::Length3(_) => Some(3),
                BaseMroStart::Class(_) => None,
            };
            if let Some(len) = array_len {
                assert_eq!(
                    expected
                        .iter()
                        .filter(|event| **event == Event::Iteration(MroIterationWork::Advance))
                        .count(),
                    len + 1
                );
                let appends: Vec<_> = expected
                    .iter()
                    .filter_map(|event| match event {
                        Event::Collection(MroCollectionWork::Append { len, .. }) => Some(*len),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    appends,
                    (0..len + usize::from(with_root)).collect::<Vec<_>>()
                );
                assert!(expected.iter().all(|event| matches!(
                    event,
                    Event::Iteration(MroIterationWork::Advance) | Event::Collection(_)
                )));
            }
            for index in 0..expected.len() {
                let effects = Recording::new(&db, Some(index));
                assert_eq!(collect(&effects), Err(Refused(index)));
                assert_eq!(*effects.events.borrow(), expected[..=index]);
            }
        }
    }
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

fn cold_trace(name: &str, with_root: bool, original: bool) -> anyhow::Result<Vec<String>> {
    let db = database()?;
    let start = start(&db, name)?;
    let root = root(&db)?;
    executions(&db);
    if with_root {
        if original {
            let _: Mro<'_> = std::iter::once(ClassBase::Class(root))
                .chain(start.into_iter(&db))
                .collect();
        } else {
            infallible(collect_start_with_root_sync(
                &db,
                root,
                start,
                &InlineMroRootEffects::new(&db),
            ));
        }
    } else if original {
        let _: VecDeque<_> = start.into_iter(&db).collect();
    } else {
        infallible(collect_start_sync(
            &db,
            start,
            &InlineMroRootEffects::new(&db),
        ));
    }
    Ok(executions(&db))
}

#[test]
fn cold_source_order_is_independent_of_raw_collection_equality() -> anyhow::Result<()> {
    for name in ["Child", "Specialized", "Dynamic", "Invalid"] {
        for with_root in [false, true] {
            let expected = cold_trace(name, with_root, true)?;
            let actual = cold_trace(name, with_root, false)?;
            assert_eq!(actual, expected, "{name}, with_root={with_root}");
            if name == "Child" {
                assert!(
                    actual
                        .iter()
                        .any(|name| name.contains("try_mro_unspecialized")),
                    "missing cold MRO query: {actual:?}"
                );
            }
        }
    }
    Ok(())
}
