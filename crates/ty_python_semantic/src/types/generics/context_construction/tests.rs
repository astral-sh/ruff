use std::cell::{Cell, RefCell};

use ruff_python_ast::name::Name;
use salsa::attempt_probe::AttemptOutcome;
use salsa::execution_probe::{ExecutionLimits, RegistryBuilder, try_with_execution_budget};
use salsa::prepared_source_probe::Stamp;

use super::{ContextConstructionControl, ContextConstructionWork, GenericContext};
use crate::db::tests::{TestDb, setup_db};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::mro::attempt::AttemptMroEffects;
use crate::types::typevar::runtime::{
    register_bound_typevar_values, register_typevar_identity_values,
    register_typevar_instance_values,
};
use crate::types::{BindingContext, BoundTypeVarInstance, Type, TypeVarVariance};
use crate::{FxOrderMap, FxOrderSet};

#[test]
fn declaration_interner_schemas_register_without_evaluating_lazy_metadata() {
    let mut db = setup_db();
    db.clear_salsa_events();
    let outcome = try_with_execution_budget(
        &db,
        ExecutionLimits {
            semantic_work: 100_000,
            requested_bytes: 1_000_000,
        },
        |budget| {
            let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
            let _identity = register_typevar_identity_values(&db, &mut registry)?;
            let _instance = register_typevar_instance_values(&db, &mut registry)?;
            let _bound = register_bound_typevar_values(&db, &mut registry)?;
            let _context = super::register_generic_context_values(&db, &mut registry)?;
            registry.seal()?.run(|_| async { Ok(()) })
        },
    );
    assert!(matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))));
    assert!(
        db.take_salsa_events()
            .iter()
            .all(|event| { !matches!(event.kind, salsa::EventKind::WillExecute { .. }) })
    );
}

#[derive(Default)]
struct RecordingControl {
    work: RefCell<Vec<ContextConstructionWork>>,
    refuse_at: Option<usize>,
}

impl ContextConstructionControl for RecordingControl {
    type Error = ContextConstructionWork;

    fn checkpoint(&self, work: ContextConstructionWork) -> Result<(), Self::Error> {
        let mut recorded = self.work.borrow_mut();
        let index = recorded.len();
        recorded.push(work);
        if self.refuse_at == Some(index) {
            Err(work)
        } else {
            Ok(())
        }
    }
}

fn variables(db: &TestDb) -> [BoundTypeVarInstance<'_>; 4] {
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(db));
    let first = BoundTypeVarInstance::synthetic_self(db, Type::object(), context);
    let replacement = BoundTypeVarInstance::synthetic_self(db, Type::Never, context);
    let second = BoundTypeVarInstance::synthetic(
        db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    assert_ne!(first, replacement);
    assert_eq!(first.identity(db), replacement.identity(db));
    [first, second, first, replacement]
}

#[test]
fn construction_preserves_identity_replacement_and_encounter_order() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let variables = variables(&db);
    for values in [&[][..], &variables[..]] {
        let control = RecordingControl::default();
        let actual = GenericContext::from_typevar_instances_with(
            &db,
            &env,
            values.iter().copied(),
            &control,
        )
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let original = GenericContext::new_internal(
            &db,
            env.program(&db),
            values
                .iter()
                .map(|variable| (variable.identity(&db), *variable))
                .collect::<FxOrderMap<_, _>>(),
        );
        assert_eq!(actual, original);
        assert_eq!(
            control.work.borrow().last(),
            Some(&ContextConstructionWork::Publish)
        );
        if !values.is_empty() {
            assert_eq!(
                actual.variables(&db).collect::<Vec<_>>(),
                [variables[3], variables[1]]
            );
            let full_values: FxOrderSet<_> = values.iter().copied().collect();
            assert_eq!(full_values.len(), 3);
            assert_eq!(actual.len(&db), 2);
        }
    }
    Ok(())
}

#[test]
fn refusing_each_construction_step_stops_before_the_next_input() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let variables = variables(&db);
    // Filtering removes the exact lower bound and exercises map growth during insertion.
    let input = || variables.into_iter().filter(|_| true);
    let complete = RecordingControl::default();
    let expected = GenericContext::from_typevar_instances_with(&db, &env, input(), &complete)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let trace = complete.work.into_inner();
    assert!(trace.iter().any(
        |work| matches!(work, ContextConstructionWork::Insert { len, capacity } if len == capacity)
    ));
    for (index, refused) in trace.iter().copied().enumerate() {
        let control = RecordingControl {
            work: RefCell::default(),
            refuse_at: Some(index),
        };
        let yielded = Cell::new(0);
        let result = GenericContext::from_typevar_instances_with(
            &db,
            &env,
            input().inspect(|_| yielded.set(yielded.get() + 1)),
            &control,
        );
        assert_eq!(result, Err(refused));
        assert_eq!(*control.work.borrow(), trace[..=index]);
        let completed_advances = trace[..index]
            .iter()
            .filter(|work| **work == ContextConstructionWork::Advance)
            .count();
        assert_eq!(yielded.get(), completed_advances.min(variables.len()));
    }
    let retry = GenericContext::from_typevar_instances_with(
        &db,
        &env,
        input(),
        &RecordingControl::default(),
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(retry, expected);
    Ok(())
}

#[test]
fn installed_construction_refuses_and_retries_in_the_same_revision() {
    let db = setup_db();
    let env = db.program_environment();
    let variables = variables(&db);
    let expected = GenericContext::from_typevar_instances(&db, &env, variables);
    let stamp = Stamp::current(&db);
    let build = || {
        GenericContext::from_typevar_instances_with(
            &db,
            &env,
            variables,
            &AttemptMroEffects::new(&db),
        )
    };
    let (complete, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
    assert_eq!(complete, Ok(Ok(expected)));
    let mut first_complete = None;
    for allowance in 0..1024 {
        let (limited, _) = expansion_probe::run_mro_observed(&db, allowance, build);
        if limited == Ok(Ok(expected)) {
            first_complete = Some(allowance);
            break;
        }
        assert_eq!(limited, Err(Incomplete::Allowance));
        assert!(!expansion_probe::active());
        assert_eq!(Stamp::current(&db), stamp);
        for _ in 0..2 {
            let (retried, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
            assert_eq!(retried, Ok(Ok(expected)));
        }
    }
    assert!(first_complete.is_some_and(|allowance| allowance > 0));
}
