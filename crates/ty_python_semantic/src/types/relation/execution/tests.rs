use std::cell::{Cell, RefCell};
use std::future::{pending, ready};
use std::rc::Rc;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::attempt::AttemptAdmission;
use super::resources::{CallBuilders, CallRelationOwners};
use super::{ExecutionAdmission, ExecutionWork, SchedulingFailure, Task, TaskDriver, TaskEndpoint};
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::relation::{TypeRelationChecker, TypeVarEvaluation};
use crate::types::typevar::TypeVarSet;
use crate::types::{ClassLiteral, Type};

#[derive(Default)]
struct Admission {
    remaining: Cell<Option<usize>>,
    work: RefCell<Vec<ExecutionWork>>,
}

impl ExecutionAdmission for Admission {
    type Error = Incomplete;

    fn scheduling_failure(&self, failure: SchedulingFailure) -> Incomplete {
        Incomplete::Scheduling(failure)
    }

    fn admit(&self, work: ExecutionWork) -> Result<(), Incomplete> {
        if let Some(remaining) = self.remaining.get() {
            let Some(remaining) = remaining.checked_sub(1) else {
                return Err(Incomplete::Allowance);
            };
            self.remaining.set(Some(remaining));
        }
        self.work.borrow_mut().push(work);
        Ok(())
    }
}

#[test]
fn weighted_admission_shares_allowance_and_preserves_refusal() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let admission = AttemptAdmission { db: &db };
    let (result, _) = expansion_probe::run(&db, 10, || {
        assert_eq!(admission.admit(ExecutionWork::Work { units: 6 }), Ok(()));
        assert_eq!(
            admission.admit(ExecutionWork::Allocation {
                requested_payload_bytes: 4,
            }),
            Ok(())
        );
        assert_eq!(
            admission.admit(ExecutionWork::Poll),
            Err(Incomplete::Allowance)
        );
        assert_eq!(
            admission.refuse(Incomplete::Interrupted),
            Incomplete::Allowance
        );
        assert_eq!(
            admission.admit(ExecutionWork::Work { units: 0 }),
            Err(Incomplete::Allowance)
        );
    });
    assert_eq!(result, Err(Incomplete::Allowance));

    let (result, _) = expansion_probe::run(&db, 10, || {
        assert_eq!(
            admission.admit(ExecutionWork::Work { units: usize::MAX }),
            Err(Incomplete::Allowance)
        );
    });
    assert_eq!(result, Err(Incomplete::Allowance));

    let (result, _) = expansion_probe::run(&db, 10, || {
        admission.admit(ExecutionWork::Work { units: 10 })
    });
    assert_eq!(result, Ok(Ok(())));
    Ok(())
}

struct Entered {
    index: usize,
    log: Rc<RefCell<Vec<usize>>>,
}

impl Drop for Entered {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.index);
    }
}

#[inline(never)]
fn poll_stack_marker() -> usize {
    let marker = 0u8;
    std::ptr::from_ref(&marker).addr()
}

fn nested(
    endpoint: TaskEndpoint<'_, Incomplete>,
    index: usize,
    limit: usize,
    log: Rc<RefCell<Vec<usize>>>,
    markers: Rc<RefCell<Vec<usize>>>,
) -> Task<'_, Incomplete> {
    Box::pin(async move {
        let _scope = Entered {
            index,
            log: Rc::clone(&log),
        };
        markers.borrow_mut().push(poll_stack_marker());
        if index < limit {
            let child_endpoint = endpoint.clone();
            endpoint
                .demand(move || nested(child_endpoint, index + 1, limit, log, markers))?
                .await?;
        }
        Ok(())
    })
}

#[test]
fn execution_stack_cancels_inside_out_and_keeps_poll_depth_flat() -> anyhow::Result<()> {
    for allowance in [Some(0), Some(1), Some(31), Some(1024), None] {
        let admission = Admission {
            remaining: Cell::new(allowance),
            ..Admission::default()
        };
        let log = Rc::new(RefCell::new(Vec::new()));
        let markers = Rc::new(RefCell::new(Vec::new()));
        let result = TaskDriver::new(&admission).and_then(|driver| {
            let endpoint = driver.endpoint();
            driver.run(|| nested(endpoint, 0, 4096, Rc::clone(&log), Rc::clone(&markers)))
        });
        if allowance.is_none() {
            assert_eq!(result, Ok(()));
            assert_eq!(log.borrow().len(), 4097);
        } else {
            assert_eq!(result, Err(Incomplete::Allowance));
        }
        let entered = markers.borrow().len();
        assert_eq!(*log.borrow(), (0..entered).rev().collect::<Vec<_>>());
        let markers = markers.borrow();
        if let (Some(min), Some(max)) = (markers.iter().min(), markers.iter().max()) {
            assert!(
                max - min < 4096,
                "task nesting should not grow the native poll stack"
            );
        }
        for _ in 0..2 {
            let retry_admission = Admission::default();
            let retry = TaskDriver::new(&retry_admission)
                .map_err(|error| anyhow::anyhow!("unexpected admission error {error:?}"))?;
            let endpoint = retry.endpoint();
            assert_eq!(
                retry.run(|| nested(
                    endpoint,
                    0,
                    3,
                    Rc::new(RefCell::new(Vec::new())),
                    Rc::new(RefCell::new(Vec::new()))
                )),
                Ok(())
            );
        }
    }
    Ok(())
}

#[test]
fn execution_reports_unscheduled_or_concurrent_children() -> anyhow::Result<()> {
    let admission = Admission::default();
    let driver = TaskDriver::new(&admission).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(
        driver.run(pending::<Result<(), Incomplete>>),
        Err(Incomplete::Scheduling(SchedulingFailure::MissingChild))
    );

    let driver = TaskDriver::new(&admission).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let endpoint = driver.endpoint();
    assert_eq!(
        driver.run(|| async move {
            let first = endpoint.demand(|| async { Ok(1u8) })?;
            let _second = endpoint.demand(|| async { Ok(2u8) })?;
            first.await
        }),
        Err(Incomplete::Scheduling(
            SchedulingFailure::ConcurrentChildren
        ))
    );

    let driver = TaskDriver::new(&admission).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let endpoint = driver.endpoint();
    let escaped = endpoint.clone();
    assert_eq!(
        driver.run(|| async move {
            let _child = endpoint.demand(|| async { Ok(1u8) })?;
            Ok(())
        }),
        Err(Incomplete::Scheduling(SchedulingFailure::UnexpectedChild))
    );
    assert!(matches!(
        escaped.demand(|| async { Ok(()) }),
        Err(Incomplete::Scheduling(SchedulingFailure::Cancelled))
    ));
    Ok(())
}

#[test]
fn execution_retains_separate_call_and_root_constraint_owners() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/call_owners.py",
            "class Generic[T]:\n    value: T\n\nclass Data:\n    value: int\n",
        )
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/call_owners.py")?,
        env.program(&db),
    );
    let instance = |name| -> anyhow::Result<Type<'_>> {
        let class = global_symbol(&db, file, name)
            .place
            .expect_type()
            .as_class_literal()
            .and_then(ClassLiteral::as_static)
            .ok_or_else(|| anyhow::anyhow!("missing {name}"))?;
        Ok(Type::instance(
            &db,
            &env,
            class.identity_specialization(&db),
        ))
    };
    let variable = instance("Generic")?
        .member(&db, &env, "value")
        .place
        .expect_type()
        .as_typevar()
        .ok_or_else(|| anyhow::anyhow!("missing type variable"))?;
    let int = instance("Data")?
        .member(&db, &env, "value")
        .place
        .expect_type();
    let inferable = TypeVarSet::from_typevars(&db, [variable]);
    let root_builder = ConstraintSetBuilder::new();
    let owners = CallRelationOwners::default();
    let admission = Admission::default();
    let driver = TaskDriver::new(&admission).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let endpoint = driver.endpoint();
    let root_owners = owners
        .allocate(&endpoint, &env, &root_builder)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let root_checker = TypeRelationChecker {
        typevar_evaluation: TypeVarEvaluation::Lazy,
        ..root_owners.assignability(inferable)
    };
    let expected = root_checker.check_type_pair(&db, int, Type::TypeVar(variable));
    assert!(!expected.is_trivially_always_satisfied());
    assert!(!expected.is_trivially_never_satisfied());
    drop(driver);
    let root_result = run_with_call_owners(
        &db,
        &env,
        root_checker,
        int,
        Type::TypeVar(variable),
        inferable,
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert!(root_result.ownership_probe_same_set(expected));
    Ok(())
}

// The caller's invariant builder lifetime cannot be shortened to these local arenas.
fn run_with_call_owners<'db, 'root>(
    db: &'db dyn Db,
    env: &crate::ProgramEnvironment<'db>,
    root_checker: TypeRelationChecker<'_, 'root, 'db>,
    int: Type<'db>,
    variable: Type<'db>,
    inferable: TypeVarSet<'db>,
) -> Result<ConstraintSet<'db, 'root>, Incomplete> {
    let builders = CallBuilders::default();
    let owners = CallRelationOwners::default();
    let admission = Admission::default();
    let driver = TaskDriver::new(&admission)?;
    let endpoint = driver.endpoint();
    let builders = &builders;
    let owners = &owners;
    let root_builder = root_checker.constraints;
    driver.run(|| async move {
        let child_builder = builders.allocate(&endpoint)?;
        let child_owners = owners.allocate(&endpoint, env, child_builder)?;
        let child_checker = TypeRelationChecker {
            typevar_evaluation: TypeVarEvaluation::Lazy,
            ..child_owners.assignability(inferable)
        };
        assert!(!std::ptr::eq(root_builder, child_builder));
        let root_result = endpoint
            .demand(move || async move { Ok(root_checker.check_type_pair(db, int, variable)) })?
            .await?;
        let nested_endpoint = endpoint.clone();
        let (child_result, nested_result) = endpoint
            .demand(move || async move {
                let nested_builder = builders.allocate(&nested_endpoint)?;
                assert!(!std::ptr::eq(nested_builder, child_builder));
                assert!(!std::ptr::eq(nested_builder, root_builder));
                let nested_owners = owners.allocate(&nested_endpoint, env, nested_builder)?;
                let nested_checker = TypeRelationChecker {
                    typevar_evaluation: TypeVarEvaluation::Lazy,
                    ..nested_owners.assignability(inferable)
                };
                let nested_result = nested_endpoint
                    .demand(move || async move {
                        Ok(nested_checker.check_type_pair(db, int, variable))
                    })?
                    .await?;
                let child_result = child_checker.check_type_pair(db, int, variable);
                Ok((child_result, nested_result))
            })?
            .await?;
        for result in [child_result, nested_result] {
            assert!(!result.is_trivially_always_satisfied());
            assert!(!result.is_trivially_never_satisfied());
        }
        Ok(root_result)
    })
}

#[test]
fn execution_refuses_before_creating_task_or_call_resources() -> anyhow::Result<()> {
    let prepared = Cell::new(false);
    let admission = Admission::default();
    let driver = TaskDriver::new(&admission).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let endpoint = driver.endpoint();
    admission.remaining.set(Some(0));
    assert!(matches!(
        endpoint.demand(|| {
            prepared.set(true);
            async { Ok(()) }
        }),
        Err(Incomplete::Allowance)
    ));
    assert!(!prepared.get());
    let builders = CallBuilders::default();
    assert!(matches!(
        builders.allocate(&endpoint),
        Err(Incomplete::Allowance)
    ));
    // Refusal happens before even the arena's first backing chunk is created.
    assert!(!builders.is_initialized());

    // Admit the root and the child's storage, but refuse the child's first poll. A factory
    // returning an already-ready future must not execute before that polling admission.
    admission.remaining.set(Some(3));
    let task_prepared = &prepared;
    assert_eq!(
        driver.run(move || async move {
            endpoint
                .demand(move || {
                    task_prepared.set(true);
                    ready(Ok(()))
                })?
                .await
        }),
        Err(Incomplete::Allowance)
    );
    assert!(!prepared.get());
    Ok(())
}
