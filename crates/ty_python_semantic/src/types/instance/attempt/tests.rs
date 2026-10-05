use std::cell::RefCell;
use std::future::{Future, ready};
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{UnsupportedInstanceOperation, instance};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::Specialization;
use crate::types::instance::effects::{InstanceEffects, InstanceWork};
use crate::types::mapping::attempt::bind_self;
use crate::types::signatures::effects::{sealed, try_poll_immediate};
use crate::types::tuple::TupleType;
use crate::types::typevar::BindingContext;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, KnownClass, StaticClassLiteral, Type,
    TypeFormType,
};
use crate::{Db, ProgramEnvironment};

fn database() -> anyhow::Result<TestDb> {
    database_at("/src/instances.py")
}

fn database_at(path: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(path, "class Owner: ...\nclass Receiver(Owner): ...\n")
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassType<'db>> {
    class_at(db, "/src/instances.py", name)
}

fn class_at<'db>(db: &'db TestDb, path: &str, name: &str) -> anyhow::Result<ClassType<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(db, system_path_to_file(db, path)?, env.program(db));
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .map(ClassType::NonGeneric)
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
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

#[test]
fn cold_inherited_construction_and_self_mapping_share_one_attempt() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let owner = class(&db, "Owner")?;
    let receiver = class(&db, "Receiver")?;
    let setup_events = executions(&db);
    assert!(
        !setup_events
            .iter()
            .any(|name| name.contains("instance_flags_inner")
                || name.contains("try_mro_unspecialized")),
        "{setup_events:?}"
    );
    let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
        let owner = instance(&db, &env, owner)?;
        let receiver = instance(&db, &env, receiver)?;
        let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
            &db,
            owner,
            BindingContext::Synthetic(env.program(&db)),
        ));
        let root = TypeFormType::from_type_expression(&db, variable);
        let (mapped, statistics) = bind_self(&db, &env, root, receiver, None);
        assert_eq!(statistics.frames, statistics.dropped_frames);
        mapped.map(|mapped| (mapped, receiver))
    });
    let (mapped, actual_receiver) = result
        .and_then(|result| result)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(
        mapped,
        TypeFormType::from_type_expression(&db, actual_receiver)
    );
    let events = executions(&db);
    assert!(
        events
            .iter()
            .any(|name| name.contains("instance_flags_inner")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .filter(|name| name.contains("try_mro_unspecialized"))
            .count()
            >= 2,
        "{events:?}"
    );
    assert!(
        events.iter().any(|name| name.contains("place_table")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|name| name.contains("class_mro_literals")),
        "{events:?}"
    );
    assert_eq!(actual_receiver, Type::instance(&db, &env, receiver));
    Ok(())
}

#[test]
fn installed_adapter_preserves_cold_source_query_order() -> anyhow::Result<()> {
    fn evaluate(controlled: bool, path: &str) -> anyhow::Result<Vec<String>> {
        let db = database_at(path)?;
        let env = db.program_environment();
        let receiver = class_at(&db, path, "Receiver")?;
        executions(&db);
        let actual = if controlled {
            expansion_probe::run_mro(&db, 100_000, || Type::instance(&db, &env, receiver))
                .0
                .map_err(|error| anyhow::anyhow!("{error:?}"))?
        } else {
            Type::instance(&db, &env, receiver)
        };
        let events = executions(&db);
        let Type::NominalInstance(actual) = actual else {
            anyhow::bail!("expected nominal instance, got {actual:?}");
        };
        assert_eq!(actual.class(&db, &env), receiver);
        assert!(!actual.inherits_from_explicit_any());
        Ok(events)
    }
    for path in ["/src/instances.py", "/src/instances.pyi"] {
        let ordinary = evaluate(false, path)?;
        assert!(
            ordinary
                .iter()
                .any(|name| name.contains("instance_flags_inner")),
            "{ordinary:?}"
        );
        assert_eq!(evaluate(true, path)?, ordinary, "{path}");
    }
    Ok(())
}

#[test]
fn stopped_adapter_does_not_start_classification_and_retries_without_edit() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = class(&db, "Receiver")?;
    executions(&db);
    let (stopped, _) = expansion_probe::run_mro(&db, 0, || {
        assert!(instance(&db, &env, receiver).is_err());
        assert_eq!(Type::instance(&db, &env, receiver), Type::unknown());
    });
    assert_eq!(stopped, Err(Incomplete::Allowance));
    assert!(executions(&db).is_empty());
    for _ in 0..2 {
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || instance(&db, &env, receiver));
        let actual = result
            .and_then(|result| result)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(actual, Type::instance(&db, &env, receiver));
    }
    Ok(())
}

#[test]
fn interrupted_classification_does_not_publish_a_nominal_instance() -> anyhow::Result<()> {
    let mut interrupted_after_source = false;
    for allowance in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let db = database()?;
        let env = db.program_environment();
        let receiver = class(&db, "Receiver")?;
        executions(&db);
        let mut stopped = false;
        let (result, _) = expansion_probe::run_mro(&db, allowance, || {
            let result = instance(&db, &env, receiver);
            stopped = result.is_err();
            result
        });
        let events = executions(&db);
        if stopped {
            assert_eq!(result, Err(Incomplete::Allowance));
            interrupted_after_source |= events
                .iter()
                .any(|name| name.contains("instance_flags_inner"));
        }
        for _ in 0..2 {
            let (result, _) =
                expansion_probe::run_mro(&db, 100_000, || instance(&db, &env, receiver));
            let actual = result
                .and_then(|result| result)
                .map_err(|error| anyhow::anyhow!("allowance {allowance}: {error:?}"))?;
            assert_eq!(actual, Type::instance(&db, &env, receiver));
        }
    }
    assert!(interrupted_after_source);
    Ok(())
}

#[test]
fn provider_requires_controlled_mro_mode_even_for_no_base_classes() -> anyhow::Result<()> {
    let db = database()?;
    let owner = class(&db, "Owner")?;
    executions(&db);
    let (result, _) = expansion_probe::run(&db, 100_000, || {
        instance(&db, &db.program_environment(), owner)
    });
    assert_eq!(
        result,
        Err(Incomplete::UnsupportedInstanceOperation(
            UnsupportedInstanceOperation::UncontrolledMro
        ))
    );
    assert!(executions(&db).is_empty());
    Ok(())
}

struct RefusePublication {
    events: RefCell<Vec<&'static str>>,
}

impl sealed::Sealed for RefusePublication {}

impl<'db> InstanceEffects<'db> for RefusePublication {
    type Error = ();

    async fn checkpoint(&self, work: InstanceWork) -> Result<(), ()> {
        match work {
            InstanceWork::Dispatch => {
                self.events.borrow_mut().push("dispatch");
                Ok(())
            }
            InstanceWork::Publish => {
                self.events.borrow_mut().push("publication refused");
                Err(())
            }
        }
    }

    fn class_literal_and_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<(ClassLiteral<'db>, Option<Specialization<'db>>), ()>> {
        ready(Ok(class.class_literal_and_specialization(db)))
    }

    fn known_class(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<Option<KnownClass>, ()>> {
        ready(Ok(class.known(db)))
    }

    fn is_typed_dict(
        &self,
        _: &'db dyn Db,
        _: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, ()>> {
        self.events.borrow_mut().push("typed dict");
        ready(Ok(false))
    }

    fn is_protocol(
        &self,
        _: &'db dyn Db,
        _: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, ()>> {
        self.events.borrow_mut().push("protocol");
        ready(Ok(false))
    }

    fn inherits_from_explicit_any(
        &self,
        _: &'db dyn Db,
        _: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, ()>> {
        self.events.borrow_mut().push("explicit any");
        ready(Ok(true))
    }

    fn tuple(
        &self,
        _: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        _: Option<Specialization<'db>>,
    ) -> impl Future<Output = Result<TupleType<'db>, ()>> {
        ready(Err(()))
    }
}

#[test]
fn publication_refusal_follows_classification_without_constructing_output() -> anyhow::Result<()> {
    let db = database()?;
    let owner = class(&db, "Owner")?;
    let effects = RefusePublication {
        events: RefCell::default(),
    };
    assert_eq!(
        try_poll_immediate(Type::instance_with(
            &db,
            &db.program_environment(),
            &effects,
            owner
        )),
        Poll::Ready(Err(()))
    );
    assert_eq!(
        *effects.events.borrow(),
        [
            "dispatch",
            "typed dict",
            "protocol",
            "explicit any",
            "publication refused"
        ]
    );
    Ok(())
}
