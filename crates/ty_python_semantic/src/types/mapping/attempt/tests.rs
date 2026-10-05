use std::cell::{Cell, RefCell};

use ruff_db::files::system_path_to_file;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::frame_observation::{self, Event as FrameEvent, Outcome as FrameOutcome};
use super::{
    Activity, ActivityScope, AttemptMappingEffects, Mailbox, MappingRequest, MappingStatistics,
    NATIVE_DEPTH, OwnedMapping, Queued, UnsupportedMappingOperation, bind_self,
    compose_specialization, drive_type, specialize_base,
};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete, charge_ledger};
use crate::types::cyclic::TypeTransformerVisit;
use crate::types::generics::{ApplySpecialization, Specialization};
use crate::types::mapping::MappingStart;
use crate::types::mapping::effects::{MappingEffects, MappingFacts, MappingOperation};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::BindingContext;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ClassBase, ClassLiteral, ClassType,
    GenericAlias, MaterializationKind, SelfBinding, Type, TypeContext, TypeFormType, TypeMapping,
};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new().with_file("/src/mapping.py", "class Owner: ...\nclass Receiver(Owner): ...\nclass Unrelated: ...\nclass Invalid(1): ...\n").build()
}

fn raw_mapping(db: &TestDb) -> anyhow::Result<(BoundTypeVarInstance<'_>, Specialization<'_>)> {
    let Type::GenericAlias(alias) =
        generic(db, "Pair", &[Type::int_literal(1), Type::int_literal(2)])?
    else {
        anyhow::bail!("missing alias");
    };
    let specialization = alias.specialization(db);
    let variable = specialization
        .generic_context(db)
        .variables(db)
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing variable"))?;
    Ok((variable, specialization))
}

#[test]
fn raw_substitution_preserves_shared_visitors_and_recovers_after_interruption() -> anyhow::Result<()>
{
    let db = TestDbBuilder::new()
        .with_file("/src/mapping.py", "class Pair[A, B]: ...\n")
        .build()?;
    let env = db.program_environment();
    let (variable, specialization) = raw_mapping(&db)?;
    let root = wrapped(&db, Type::TypeVar(variable), 96);
    let expected = wrapped(&db, Type::int_literal(1), 96);
    let calibration_visitor = ApplyTypeMappingVisitor::new(&env);
    let last_work = Cell::new(None);
    let prefix_copies_requested = Cell::new(0);
    let calibration_control = AttemptMappingEffects {
        db: &db,
        last_work: &last_work,
        prefix_copies_requested: &prefix_copies_requested,
        mode: (),
    };
    let mut calibration_statistics = MappingStatistics::default();
    let ((calibration, _), charges) = charge_ledger::capture(true, || {
        expansion_probe::run_mro(&db, 1_000_000, || {
            drive_type(
                &db,
                MappingRequest {
                    ty: root,
                    tcx: TypeContext::default(),
                    mapping: OwnedMapping::Specialize {
                        specialization,
                        specialize_self_domain: false,
                    },
                },
                &calibration_visitor,
                &calibration_control,
                &mut calibration_statistics,
            )
        })
    });
    assert_eq!(calibration.and_then(|result| result), Ok(expected));
    assert_eq!(
        calibration_statistics.frames,
        calibration_statistics.dropped_frames
    );
    let mut charged = 0usize;
    let mut active_storage_accepted = false;
    let mut boundary_allowances = Vec::new();
    for charge in charges {
        if let charge_ledger::Event::Charge {
            work,
            units,
            outcome,
            ..
        } = charge
        {
            assert_eq!(outcome, Ok(()));
            if let Some(work) = work {
                active_storage_accepted |= work.kind.ends_with("TypeTransformationWork")
                    && work.value.starts_with("ActiveStorage {");
                if work.value == "\"MappingTaskCapacity\"" || work.value == "\"MappingFrame\"" {
                    assert!(active_storage_accepted);
                    assert!(units > 0);
                    boundary_allowances.push(charged);
                    if work.value == "\"MappingFrame\"" {
                        break;
                    }
                }
            }
            charged = charged
                .checked_add(units)
                .expect("finite mapping charge prefix");
        }
    }
    assert_eq!(boundary_allowances.len(), 2);
    assert!(boundary_allowances[0] < boundary_allowances[1]);
    let mut allowances = vec![0, 24, 32, 256, 2048, 16384];
    allowances.extend(boundary_allowances);
    allowances.sort_unstable();
    allowances.dedup();
    let mut refused_capacity_after_scope = false;
    let mut refused_frame_after_scope = false;
    for allowance in allowances {
        let visitor = ApplyTypeMappingVisitor::new(&env);
        for (attempt, allowance) in [allowance, 1_000_000, 1_000_000].into_iter().enumerate() {
            let last_work = Cell::new(None);
            let prefix_copies_requested = Cell::new(0);
            let control = AttemptMappingEffects {
                db: &db,
                last_work: &last_work,
                prefix_copies_requested: &prefix_copies_requested,
                mode: (),
            };
            let mut statistics = MappingStatistics::default();
            let ((outcome, _), charges) = charge_ledger::capture(true, || {
                expansion_probe::run_mro(&db, allowance, || {
                    drive_type(
                        &db,
                        MappingRequest {
                            ty: root,
                            tcx: TypeContext::default(),
                            mapping: OwnedMapping::Specialize {
                                specialization,
                                specialize_self_domain: false,
                            },
                        },
                        &visitor,
                        &control,
                        &mut statistics,
                    )
                })
            });
            let outcome = outcome.and_then(|result| result);
            assert_eq!(statistics.frames, statistics.dropped_frames);
            if attempt == 0 {
                assert_eq!(outcome, Err(Incomplete::Allowance));
                let mut active_storage_accepted = false;
                for charge in charges {
                    let charge_ledger::Event::Charge {
                        channel: charge_ledger::Channel::Weighted,
                        work: Some(work),
                        outcome,
                        ..
                    } = charge
                    else {
                        continue;
                    };
                    active_storage_accepted |= outcome.is_ok()
                        && work.kind.ends_with("TypeTransformationWork")
                        && work.value.starts_with("ActiveStorage {");
                    if outcome.is_err() && active_storage_accepted && statistics.frames == 0 {
                        refused_capacity_after_scope |= work.value == "\"MappingTaskCapacity\"";
                        refused_frame_after_scope |= work.value == "\"MappingFrame\"";
                    }
                }
            } else {
                assert_eq!(outcome, Ok(expected));
                if attempt == 2 {
                    assert_eq!(
                        statistics.frames, 0,
                        "completed outer sibling should reuse the same visitor cache"
                    );
                }
            }
        }
    }
    assert!(refused_capacity_after_scope);
    assert!(refused_frame_after_scope);

    let visitor = ApplyTypeMappingVisitor::new(&env);
    let mapping =
        TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization));
    let last_work = Cell::new(None);
    let prefix_copies_requested = Cell::new(0);
    let control = AttemptMappingEffects {
        db: &db,
        last_work: &last_work,
        prefix_copies_requested: &prefix_copies_requested,
        mode: (),
    };
    let (outcome, _) = expansion_probe::run_mro(&db, 1_000_000, || -> anyhow::Result<()> {
        let Ok(TypeTransformerVisit::Pending(scope)) =
            control.begin_transformation(&db, root, &mapping, &visitor)
        else {
            anyhow::bail!("expected a new active transformation");
        };
        // Active recovery must not publish a completed substitution into the shared cache.
        assert!(matches!(
            root.mapping_start_sync(&db, &mapping, TypeContext::default(), &visitor, &control),
            Ok(MappingStart::Complete(recovered)) if recovered == root
        ));
        drop(scope);
        for retry in 0..2 {
            let mut statistics = MappingStatistics::default();
            let result = drive_type(
                &db,
                MappingRequest {
                    ty: root,
                    tcx: TypeContext::default(),
                    mapping: OwnedMapping::Specialize {
                        specialization,
                        specialize_self_domain: false,
                    },
                },
                &visitor,
                &control,
                &mut statistics,
            );
            assert_eq!(result, Ok(expected));
            assert_eq!(statistics.frames, statistics.dropped_frames);
            if retry == 1 {
                assert_eq!(statistics.frames, 0);
            }
        }
        Ok(())
    });
    outcome.map_err(|reason| anyhow::anyhow!("active recovery attempt failed: {reason:?}"))??;
    Ok(())
}

#[test]
fn deep_raw_substitution_uses_a_flat_stack() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| -> anyhow::Result<()> {
            let db = TestDbBuilder::new()
                .with_file("/src/mapping.py", "class Pair[A, B]: ...\n")
                .build()?;
            let env = db.program_environment();
            let (variable, specialization) = raw_mapping(&db)?;
            let root = wrapped(&db, Type::TypeVar(variable), 2048);
            let visitor = ApplyTypeMappingVisitor::new(&env);
            let last_work = Cell::new(None);
            let prefix_copies_requested = Cell::new(0);
            let control = AttemptMappingEffects {
                db: &db,
                last_work: &last_work,
                prefix_copies_requested: &prefix_copies_requested,
                mode: (),
            };
            let mut statistics = MappingStatistics::default();
            let _root = ActivityScope::enter(Activity::Root);
            let ((outcome, _), frames) = frame_observation::capture(|| {
                expansion_probe::run_mro(&db, 100_000_000, || {
                    drive_type(
                        &db,
                        MappingRequest {
                            ty: root,
                            tcx: TypeContext::default(),
                            mapping: OwnedMapping::Specialize {
                                specialization,
                                specialize_self_domain: false,
                            },
                        },
                        &visitor,
                        &control,
                        &mut statistics,
                    )
                })
            });
            frame_observation::write_trace("deep-raw-substitution", &frames)?;
            assert!(frames.iter().any(|event| matches!(
                event,
                FrameEvent::Polled {
                    outcome: FrameOutcome::Pending,
                    ..
                }
            )));
            assert_eq!(outcome, Ok(Ok(wrapped(&db, Type::int_literal(1), 2048))));
            assert_eq!(statistics.max_pending, 2048);
            assert_eq!(statistics.frames, statistics.dropped_frames);
            let depth = NATIVE_DEPTH.with(Cell::get);
            assert_eq!(depth.max_drivers, 1);
            assert_eq!(depth.max_polls, 1);
            assert_eq!(depth.max_drops, 1);
            Ok(())
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("mapping worker panicked"))??;
    Ok(())
}

#[test]
fn raw_nominal_mapping_keeps_context_and_retained_self_flags() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/mapping.py",
            "class Owner: ...\nclass Pair[A, B]: ...\n",
        )
        .build()?;
    let env = db.program_environment();
    let (variable, specialization) = raw_mapping(&db)?;
    let argument = wrapped(&db, Type::TypeVar(variable), 5);
    let Type::GenericAlias(alias) = generic(&db, "Pair", &[argument, argument])? else {
        anyhow::bail!("missing alias");
    };
    let nominal = Type::instance(&db, &env, ClassType::Generic(alias));
    let mapping =
        TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization));
    let expected = nominal.apply_type_mapping(&db, &env, &mapping, TypeContext::default());
    let retained_self = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        &db,
        instance(&db, "Owner")?,
        BindingContext::Synthetic(env.program(&db)),
    ));
    let retained_self = wrapped(&db, retained_self, 2);
    for (root, tcx, flag, expected) in [
        (nominal, TypeContext::default(), false, Ok(expected)),
        (
            nominal,
            TypeContext::new(Some(nominal)),
            false,
            Err(Incomplete::UnsupportedMappingOperation(
                UnsupportedMappingOperation::Legacy(MappingOperation::AnnotationContext),
            )),
        ),
        (
            retained_self,
            TypeContext::default(),
            false,
            Ok(retained_self),
        ),
        (
            retained_self,
            TypeContext::default(),
            true,
            Err(Incomplete::UnsupportedMappingOperation(
                UnsupportedMappingOperation::Legacy(MappingOperation::RetainedSelf),
            )),
        ),
    ] {
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let last_work = Cell::new(None);
        let prefix_copies_requested = Cell::new(0);
        let control = AttemptMappingEffects {
            db: &db,
            last_work: &last_work,
            prefix_copies_requested: &prefix_copies_requested,
            mode: (),
        };
        let mut statistics = MappingStatistics::default();
        let (actual, _) = expansion_probe::run_mro(&db, 1_000_000, || {
            drive_type(
                &db,
                MappingRequest {
                    ty: root,
                    tcx,
                    mapping: OwnedMapping::Specialize {
                        specialization,
                        specialize_self_domain: flag,
                    },
                },
                &visitor,
                &control,
                &mut statistics,
            )
        });
        assert_eq!(actual.and_then(|value| value), expected);
        assert_eq!(statistics.frames, statistics.dropped_frames);
    }
    Ok(())
}

#[test]
fn both_mro_specialization_operations_match_their_canonical_bodies() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/mapping.py", "class Pair[A, B]: ...\n")
        .build()?;
    let env = db.program_environment();
    let (variable, additional) = raw_mapping(&db)?;
    let argument = wrapped(&db, Type::TypeVar(variable), 8);
    let Type::GenericAlias(alias) = generic(&db, "Pair", &[argument, argument])? else {
        anyhow::bail!("missing alias");
    };
    let base = alias.specialization(&db);
    let expected =
        base.apply_specialization_impl(&db, additional, &ApplyTypeMappingVisitor::new(&env));
    let (actual, _) = expansion_probe::run_mro(&db, 1_000_000, || {
        compose_specialization(&db, base, additional)
    });
    assert_eq!(actual, Ok(Ok(expected)));
    for base in [
        ClassBase::Class(ClassType::Generic(alias)),
        ClassBase::Generic,
        ClassBase::Protocol,
        ClassBase::unknown(),
        ClassBase::Any,
    ] {
        let expected = base.apply_optional_specialization(&db, Some(additional));
        let (actual, _) = expansion_probe::run_mro(&db, 1_000_000, || {
            specialize_base(&db, base, Some(additional))
        });
        assert_eq!(actual, Ok(Ok(expected)));
    }
    let materialized = additional.with_materialization_kind(&db, Some(MaterializationKind::Top));
    let (actual, _) = expansion_probe::run_mro(&db, 1_000_000, || {
        compose_specialization(&db, base, materialized)
    });
    assert!(matches!(
        actual,
        Err(Incomplete::UnsupportedMappingOperation(
            UnsupportedMappingOperation::Legacy(_)
        ))
    ));
    Ok(())
}

#[test]
fn identity_specialization_obeys_admission_and_publication() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/mapping.py", "class Pair[A, B]: ...\n")
        .build()?;
    let (_, specialization) = raw_mapping(&db)?;
    for additional in [None, Some(specialization)] {
        for allowance in [0, 1] {
            let (actual, _) = expansion_probe::run_mro(&db, allowance, || {
                specialize_base(&db, ClassBase::Protocol, additional)
            });
            assert_eq!(actual, Err(Incomplete::Allowance));
        }
        let (actual, _) = expansion_probe::run_mro(&db, 100, || {
            let reason = expansion_probe::refuse(&db, Incomplete::Interrupted);
            assert_eq!(
                specialize_base(&db, ClassBase::Protocol, additional),
                Err(reason)
            );
        });
        assert_eq!(actual, Err(Incomplete::Interrupted));
        for _ in 0..2 {
            let (actual, _) = expansion_probe::run_mro(&db, 100, || {
                specialize_base(&db, ClassBase::Protocol, additional)
            });
            assert_eq!(actual, Ok(Ok(ClassBase::Protocol)));
        }
    }
    Ok(())
}

#[test]
fn child_requests_do_not_borrow_another_visitor_or_partial_mapping() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/mapping.py", "class Pair[A, B]: ...\n")
        .build()?;
    let env = db.program_environment();
    let (_, specialization) = raw_mapping(&db)?;
    for different_visitor in [false, true] {
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let other = ApplyTypeMappingVisitor::new(&env);
        let mailbox = RefCell::new(Mailbox::default());
        let last_work = Cell::new(None);
        let prefix_copies_requested = Cell::new(0);
        let effects = AttemptMappingEffects {
            db: &db,
            last_work: &last_work,
            prefix_copies_requested: &prefix_copies_requested,
            mode: Queued {
                visitor: &visitor,
                mailbox: &mailbox,
            },
        };
        let mapping = if different_visitor {
            TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization))
        } else {
            TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                generic_context: specialization.generic_context(&db),
                types: (&[][..]).into(),
                skip: None,
            })
        };
        let (outcome, _) = expansion_probe::run_mro(&db, 1000, || {
            try_poll_immediate(effects.map_type(
                &db,
                Type::int_literal(1),
                &mapping,
                TypeContext::default(),
                if different_visitor { &other } else { &visitor },
            ))
        });
        assert_eq!(
            outcome,
            Err(Incomplete::UnsupportedMappingOperation(
                UnsupportedMappingOperation::ChildMapping
            ))
        );
        assert!(mailbox.borrow().request.is_none());
        assert!(mailbox.borrow().answer.is_none());
    }
    Ok(())
}

#[test]
fn cold_source_callback_observes_nested_mapping_drivers() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().with_file("/src/mapping.pyi", "class Product: ...\nclass Base[T]:\n    def __new__(cls) -> type[Product]: ...\nclass Factory[T](Base[T]): ...\nclass Receiver(Factory[int]()): ...\n").build()?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/mapping.pyi")?;
    let receiver = global_symbol(&db, db.program_file(file), "Receiver")
        .place
        .expect_type()
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("missing receiver"))?;
    let product = global_symbol(&db, db.program_file(file), "Product")
        .place
        .expect_type()
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("missing product"))?;
    let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        &db,
        Type::instance(&db, &env, ClassType::NonGeneric(product)),
        BindingContext::Synthetic(env.program(&db)),
    ));
    let binding = SelfBinding {
        ty: Type::unknown(),
        class_literal: Some(receiver),
        binding_context: None,
    };
    let visitor = ApplyTypeMappingVisitor::new(&env);
    let last_work = Cell::new(None);
    let prefix_copies_requested = Cell::new(0);
    let control = AttemptMappingEffects {
        db: &db,
        last_work: &last_work,
        prefix_copies_requested: &prefix_copies_requested,
        mode: (),
    };
    let mut statistics = MappingStatistics::default();
    executions(&db);
    let _root = ActivityScope::enter(Activity::Root);
    let ((actual, _), frames) = frame_observation::capture(|| {
        expansion_probe::run_mro(&db, 1_000_000, || {
            drive_type(
                &db,
                MappingRequest {
                    ty: variable,
                    tcx: TypeContext::default(),
                    mapping: OwnedMapping::BindSelf(binding),
                },
                &visitor,
                &control,
                &mut statistics,
            )
        })
    });
    frame_observation::write_trace("cold-source-callback", &frames)?;
    assert!(frames.iter().any(|event| matches!(
        event,
        FrameEvent::DriverEntered {
            parent: Some(_),
            ..
        }
    )));
    let reads = executions(&db);
    assert!(
        reads
            .iter()
            .any(|name| name.contains("try_mro_unspecialized"))
    );
    assert!(NATIVE_DEPTH.with(|depth| depth.get().max_drivers) > 1);
    assert!(
        reads
            .iter()
            .any(|name| name.contains("infer_deferred_types"))
    );
    assert!(
        reads
            .iter()
            .any(|name| name.contains("GenericAlias") && name.contains("try_mro"))
    );
    assert_eq!(NATIVE_DEPTH.with(|depth| depth.get().max_polls), 1);
    assert_eq!(statistics.frames, statistics.dropped_frames);
    let ordinary = variable.apply_type_mapping(
        &db,
        &env,
        &TypeMapping::BindSelf(binding),
        TypeContext::default(),
    );
    assert_eq!(actual, Ok(Ok(ordinary)));
    Ok(())
}

fn instance<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/mapping.py")?,
        env.program(db),
    );
    let class = global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))?;
    Ok(Type::instance(db, &env, ClassType::NonGeneric(class)))
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

fn wrapped<'db>(db: &'db TestDb, mut ty: Type<'db>, depth: usize) -> Type<'db> {
    for _ in 0..depth {
        ty = TypeFormType::from_type_expression(db, ty);
    }
    ty
}

fn run<'db>(
    db: &'db TestDb,
    allowance: usize,
    root: Type<'db>,
    receiver: Type<'db>,
) -> (Result<Type<'db>, Incomplete>, MappingStatistics) {
    let mut statistics = MappingStatistics::default();
    let (result, _) = expansion_probe::run_mro(db, allowance, || {
        let (result, measured) = bind_self(db, &db.program_environment(), root, receiver, None);
        statistics = measured;
        result
    });
    (result.and_then(|result| result), statistics)
}

#[test]
fn actual_owned_self_substitution_uses_cold_mro_owners() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    // A no-base class avoids instance classification warming an inherited MRO before mapping.
    let receiver = instance(&db, "Owner")?;
    let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        &db,
        receiver,
        BindingContext::Synthetic(env.program(&db)),
    ));
    let root = wrapped(&db, variable, 3);
    executions(&db);
    let (result, statistics) = run(&db, 100_000, root, receiver);
    let events = executions(&db);
    assert_eq!(result, Ok(wrapped(&db, receiver, 3)));
    assert_eq!(statistics.max_pending, 4);
    assert_eq!(statistics.frames, statistics.dropped_frames);
    assert!(statistics.max_task_poll_depth <= 1);
    assert!(statistics.max_task_drop_depth <= 1);
    assert!(
        events
            .iter()
            .any(|name| name.contains("class_mro_literals")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|name| name.contains("try_mro_unspecialized")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|name| name.contains("known_class_to_class_literal")),
        "{events:?}"
    );
    Ok(())
}

#[test]
fn prepared_instance_mapping_matches_ordinary_owner_decisions() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    // These controls compare mapping of existing instances. Constructing inherited instances
    // already resolves some class facts, so cold source classification is tested separately.
    let owner = instance(&db, "Owner")?;
    let unrelated = instance(&db, "Unrelated")?;
    for receiver_name in ["Owner", "Receiver", "Unrelated", "Invalid"] {
        let receiver = instance(&db, receiver_name)?;
        for bound in [owner, unrelated, Type::unknown()] {
            let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                &db,
                bound,
                BindingContext::Synthetic(env.program(&db)),
            ));
            let root = wrapped(&db, variable, 4);
            let expected = root.apply_type_mapping(
                &db,
                &env,
                &TypeMapping::BindSelf(SelfBinding::new(&db, &env, receiver, None)),
                TypeContext::default(),
            );
            let (actual, _) = run(&db, 100_000, root, receiver);
            assert_eq!(actual, Ok(expected), "{receiver_name}");
        }
    }
    Ok(())
}

#[test]
fn interrupted_deep_mapping_aborts_scopes_and_retries_without_an_edit() -> anyhow::Result<()> {
    let post_poll_allowance = {
        let db = database()?;
        let env = db.program_environment();
        let receiver = instance(&db, "Owner")?;
        let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
            &db,
            receiver,
            BindingContext::Synthetic(env.program(&db)),
        ));
        let root = wrapped(&db, variable, 96);
        let ((result, statistics), charges) =
            charge_ledger::capture(true, || run(&db, 1_000_000, root, receiver));
        assert_eq!(result, Ok(wrapped(&db, receiver, 96)));
        assert_eq!(statistics.max_pending, 97);
        assert_eq!(statistics.frames, statistics.dropped_frames);
        let mut charged = 0usize;
        let mut frames = 0;
        let mut allowance = None;
        for charge in charges {
            let charge_ledger::Event::Charge {
                work,
                units,
                outcome,
                ..
            } = charge
            else {
                continue;
            };
            assert_eq!(outcome, Ok(()));
            if let Some(work) = work
                && work.value == "\"MappingFrame\""
            {
                frames += 1;
                if frames == 2 {
                    allowance = Some(charged);
                    break;
                }
            }
            charged = charged
                .checked_add(units)
                .ok_or_else(|| anyhow::anyhow!("mapping charge prefix overflow"))?;
        }
        allowance.ok_or_else(|| anyhow::anyhow!("second mapping frame was not reached"))?
    };
    let mut refused = 0;
    let mut refused_after_poll = false;
    for allowance in [0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048]
        .into_iter()
        .chain([post_poll_allowance])
    {
        let db = database()?;
        let env = db.program_environment();
        let receiver = instance(&db, "Owner")?;
        let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
            &db,
            receiver,
            BindingContext::Synthetic(env.program(&db)),
        ));
        let root = wrapped(&db, variable, 96);
        let ((result, statistics), frames) =
            frame_observation::capture(|| run(&db, allowance, root, receiver));
        frame_observation::write_trace(&format!("interrupted-deep-{allowance}"), &frames)?;
        if result.is_err() {
            assert_eq!(result, Err(Incomplete::Allowance));
            assert_eq!(statistics.frames, statistics.dropped_frames);
            assert!(statistics.max_task_poll_depth <= 1);
            assert!(statistics.max_task_drop_depth <= 1);
            refused_after_poll |= frames.iter().any(|event| {
                matches!(
                    event,
                    FrameEvent::Polled {
                        outcome: FrameOutcome::Pending | FrameOutcome::Error(_),
                        ..
                    }
                )
            });
            refused += 1;
        }
        for retry in 0..2 {
            let ((result, statistics), frames) =
                frame_observation::capture(|| run(&db, 1_000_000, root, receiver));
            frame_observation::write_trace(
                &format!("interrupted-deep-{allowance}-retry{retry}"),
                &frames,
            )?;
            assert_eq!(result, Ok(wrapped(&db, receiver, 96)));
            assert_eq!(statistics.max_pending, 97);
            assert_eq!(statistics.frames, statistics.dropped_frames);
            assert!(statistics.max_task_poll_depth <= 1);
            assert!(statistics.max_task_drop_depth <= 1);
        }
    }
    assert!(refused > 8);
    assert!(refused_after_poll);
    Ok(())
}

#[test]
fn receiver_equality_stops_before_child_mapping() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Owner")?;
    let (outer, _) = expansion_probe::run_mro(&db, 100_000, || {
        bind_self(&db, &env, receiver, receiver, None)
    });
    let (result, statistics) = outer.map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(result, Ok(receiver));
    assert_eq!(statistics.frames, 0);
    assert_eq!(statistics.polls, 0);
    Ok(())
}

#[test]
fn deep_mapping_runs_on_a_small_native_stack() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| -> anyhow::Result<()> {
            let db = database()?;
            let env = db.program_environment();
            let receiver = instance(&db, "Owner")?;
            let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                &db,
                receiver,
                BindingContext::Synthetic(env.program(&db)),
            ));
            let root = wrapped(&db, variable, 2048);
            let (result, statistics) = run(&db, 100_000_000, root, receiver);
            assert_eq!(result, Ok(wrapped(&db, receiver, 2048)));
            assert_eq!(statistics.max_pending, 2049);
            assert_eq!(statistics.frames, statistics.dropped_frames);
            assert!(statistics.max_task_poll_depth <= 1);
            assert!(statistics.max_task_drop_depth <= 1);
            Ok(())
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("mapping worker panicked"))??;
    Ok(())
}

fn generic<'db>(db: &'db TestDb, name: &str, arguments: &[Type<'db>]) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/mapping.py")?,
        env.program(db),
    );
    let class = global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing generic class"))?;
    let context = class
        .generic_context(db)
        .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
    Ok(Type::GenericAlias(GenericAlias::new(
        db,
        class,
        Specialization::new(db, context, arguments, None, None),
    )))
}

#[test]
fn sibling_tasks_reuse_one_transformation_cache() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/mapping.py",
            "class Owner: ...\nclass Pair[A, B]: ...\n",
        )
        .build()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Owner")?;
    let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        &db,
        receiver,
        BindingContext::Synthetic(env.program(&db)),
    ));
    let shared = wrapped(&db, variable, 16);
    let root = generic(&db, "Pair", &[shared, shared])?;
    let replacement = wrapped(&db, receiver, 16);
    let expected = generic(&db, "Pair", &[replacement, replacement])?;
    let (result, statistics) = run(&db, 100_000, root, receiver);
    assert_eq!(result, Ok(expected));
    // One generic root, sixteen wrappers and their Self-binding leaf. The cached sibling
    // completes before allocating a frame.
    assert_eq!(statistics.frames, 18);
    assert_eq!(statistics.max_pending, 18);
    assert_eq!(statistics.frames, statistics.dropped_frames);
    assert!(statistics.max_task_poll_depth <= 1);
    assert!(statistics.max_task_drop_depth <= 1);
    Ok(())
}

#[test]
fn late_wide_reconstruction_can_refuse_before_copying_the_prefix() -> anyhow::Result<()> {
    let parameters = (0..64)
        .map(|index| format!("T{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("class Owner: ...\nclass Wide[{parameters}]: ...\n");
    let db = TestDbBuilder::new()
        .with_file("/src/mapping.py", &source)
        .build()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Owner")?;
    let variable = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        &db,
        receiver,
        BindingContext::Synthetic(env.program(&db)),
    ));
    let mut arguments = vec![Type::int_literal(1); 63];
    arguments.push(wrapped(&db, variable, 1));
    let root = generic(&db, "Wide", &arguments)?;
    arguments[63] = wrapped(&db, receiver, 1);
    let expected = generic(&db, "Wide", &arguments)?;
    assert_eq!(run(&db, 1_000_000, root, receiver).0, Ok(expected));

    // Warm source children make the admission boundary repeatable without a source edit.
    // Find the first allowance that reaches the prefix-copy checkpoint, regardless of
    // the fixed frame size chosen by the compiler.
    let mut low = 0;
    let mut high = 1_000_000;
    let mut found = false;
    while low <= high {
        let allowance = low + (high - low) / 2;
        let (result, statistics) = run(&db, allowance, root, receiver);
        match statistics.last_work {
            Some(crate::types::mapping::effects::MappingWork::ArgumentPrefixCopy { len: 63 }) => {
                assert_eq!(result, Err(Incomplete::Allowance));
                assert_eq!(statistics.frames, statistics.dropped_frames);
                assert!(statistics.max_task_poll_depth <= 1);
                assert!(statistics.max_task_drop_depth <= 1);
                found = true;
                break;
            }
            _ if statistics.prefix_copies_requested > 0 => {
                if allowance == 0 {
                    break;
                }
                high = allowance - 1;
            }
            _ => low = allowance + 1,
        }
    }
    assert!(found, "prefix-copy refusal was not reached");
    for _ in 0..2 {
        assert_eq!(run(&db, 1_000_000, root, receiver).0, Ok(expected));
    }
    Ok(())
}

#[test]
fn inactive_owner_control_refuses_before_mapping() -> anyhow::Result<()> {
    let db = database()?;
    let receiver = instance(&db, "Owner")?;
    executions(&db);
    let (result, _) = expansion_probe::run(&db, 10_000, || {
        bind_self(
            &db,
            &db.program_environment(),
            Type::unknown(),
            receiver,
            None,
        )
    });
    assert!(matches!(
        result,
        Err(Incomplete::UnsupportedMappingOperation(
            super::UnsupportedMappingOperation::UncontrolledMro
        ))
    ));
    assert!(
        !executions(&db)
            .iter()
            .any(|name| name.contains("class_mro_literals"))
    );
    Ok(())
}

#[test]
fn controlled_mapping_cache_growth_and_warm_hits_share_one_visitor() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/mapping.py", "class Pair[A, B]: ...\n")
        .build()?;
    let env = db.program_environment();
    let (_, specialization) = raw_mapping(&db)?;
    let mapping =
        TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization));
    let mut hit_quote = None;
    for width in [2usize, 32, 128] {
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let inputs: Vec<_> = (0..width)
            .map(|index| wrapped(&db, Type::int_literal(index as i64), 1))
            .collect();
        let expected: Vec<_> = inputs
            .iter()
            .map(|input| input.apply_type_mapping(&db, &env, &mapping, TypeContext::default()))
            .collect();
        assert_eq!(expected, inputs);
        let last_work = Cell::new(None);
        let prefix_copies_requested = Cell::new(0);
        let control = AttemptMappingEffects {
            db: &db,
            last_work: &last_work,
            prefix_copies_requested: &prefix_copies_requested,
            mode: (),
        };
        let invoke = |input, statistics: &mut MappingStatistics| {
            drive_type(
                &db,
                MappingRequest {
                    ty: input,
                    tcx: TypeContext::default(),
                    mapping: OwnedMapping::Specialize {
                        specialization,
                        specialize_self_domain: false,
                    },
                },
                &visitor,
                &control,
                statistics,
            )
        };
        let ((outcome, _), cold) = charge_ledger::capture(true, || {
            expansion_probe::run_mro(&db, 10_000_000, || {
                for (input, expected) in inputs.iter().zip(&expected) {
                    let mut statistics = MappingStatistics::default();
                    let actual = invoke(*input, &mut statistics)?;
                    assert_eq!(actual, *expected);
                    assert_eq!(statistics.frames, statistics.dropped_frames);
                }
                Ok(())
            })
        });
        assert_eq!(outcome.and_then(|value| value), Ok(()));
        let cold_units: usize = cold
            .iter()
            .filter_map(|event| match event {
                charge_ledger::Event::Charge { units, .. } => Some(*units),
                _ => None,
            })
            .sum();
        assert!(
            cold_units <= 8192 * width,
            "the shallow fill has linear work and allocation debits: {width} {cold_units}"
        );
        assert_eq!(cold.iter().any(|event| matches!(event, charge_ledger::Event::Charge { work: Some(work), .. } if work.value.starts_with("Grow { storage: Cache"))), width > 2);
        let ((outcome, _), hit) = charge_ledger::capture(true, || {
            expansion_probe::run_mro(&db, 100_000, || {
                let mut statistics = MappingStatistics::default();
                let value = invoke(inputs[0], &mut statistics)?;
                assert_eq!(statistics.frames, 0);
                assert_eq!(value, inputs[0]);
                Ok(())
            })
        });
        assert_eq!(outcome.and_then(|value| value), Ok(()));
        let units: usize = hit
            .iter()
            .filter_map(|event| match event {
                charge_ledger::Event::Charge { units, .. } => Some(*units),
                _ => None,
            })
            .sum();
        assert!(units > 0);
        if let Some(previous) = hit_quote {
            assert_eq!(units, previous);
        } else {
            hit_quote = Some(units);
        }
        assert!(!hit.iter().any(|event| matches!(event, charge_ledger::Event::Charge { work: Some(work), .. } if work.value.starts_with("Grow {") || work.value.starts_with("AncestorComparison") || work.value.starts_with("CacheStorage") || work.value.starts_with("RehashKey"))));
        let completed = Cell::new(0);
        let (outcome, _) = expansion_probe::run_mro(&db, 3 * units, || {
            for _ in 0..8 {
                let value = invoke(inputs[0], &mut MappingStatistics::default())?;
                assert_eq!(value, inputs[0]);
                completed.set(completed.get() + 1);
            }
            Ok(())
        });
        assert_eq!(outcome.and_then(|value| value), Err(Incomplete::Allowance));
        assert_eq!(completed.get(), 3);
        let (retry, _) = expansion_probe::run_mro(&db, 100_000, || {
            invoke(inputs[0], &mut MappingStatistics::default())
        });
        assert_eq!(retry.and_then(|value| value), Ok(inputs[0]));
    }
    Ok(())
}
