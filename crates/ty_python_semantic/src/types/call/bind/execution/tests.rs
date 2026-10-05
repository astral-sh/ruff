use std::cell::{Cell, RefCell};
use std::future::{Future, ready};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::call::bind::effects::InlineBinderEffects;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::relation::execution::{
    ExecutionAdmission, ExecutionWork, SchedulingFailure, TaskDriver,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{ClassLiteral, TypeContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Failure {
    Stopped,
    WrongContext,
    UnexpectedCondition,
    Scheduling(SchedulingFailure),
}

#[derive(Default)]
struct Control {
    stop_at: Option<usize>,
    events: RefCell<Vec<&'static str>>,
    comparisons: Cell<usize>,
}

impl Control {
    fn perform<T>(
        &self,
        before: &'static str,
        after: &'static str,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Failure> {
        self.event(before)?;
        let result = operation();
        self.event(after)?;
        Ok(result)
    }

    fn event(&self, name: &'static str) -> Result<(), Failure> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(name);
        if self.stop_at == Some(index) {
            Err(Failure::Stopped)
        } else {
            Ok(())
        }
    }
}

impl ExecutionAdmission for Control {
    type Error = Failure;
    fn admit(&self, _work: ExecutionWork) -> Result<(), Failure> {
        self.event("admit")
    }
    fn scheduling_failure(&self, failure: SchedulingFailure) -> Failure {
        Failure::Scheduling(failure)
    }
}

struct Finite<'a, 'db> {
    control: &'a Control,
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    builder: &'a ConstraintSetBuilder<'db>,
}

// These leaves deliberately use finite ordinary comparisons. The test checks the canonical binder
// and separate builder ownership; it does not establish supervision inside relation checking.
impl<'run, 'a: 'run, 'c: 'run, 'db: 'run> ConditionExecutor<'run, 'c, 'db> for Finite<'a, 'db> {
    type Error = Failure;

    fn context_mismatch(&self) -> Failure {
        Failure::WrongContext
    }

    fn relate(
        &'run self,
        checker: TypeRelationChecker<'run, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Failure>> + 'run {
        ready(self.control.perform("relate", "related", || {
            assert!(std::ptr::eq(checker.constraints, self.builder));
            let result = checker.check_type_pair(self.db, source, target);
            self.control
                .comparisons
                .set(self.control.comparisons.get() + 1);
            result
        }))
    }

    fn equivalent(
        &'run self,
        checker: EquivalenceChecker<'run, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Failure>> + 'run {
        ready(self.control.perform("equivalent", "equated", || {
            assert!(std::ptr::eq(checker.constraints, self.builder));
            checker.check_type_pair(self.db, left, right)
        }))
    }

    fn satisfaction(
        &'run self,
        constraints: ConstraintSet<'db, 'c>,
        test: Satisfaction,
    ) -> impl Future<Output = Result<bool, Failure>> + 'run {
        ready(
            self.control
                .perform("satisfaction", "satisfied", || match test {
                    Satisfaction::Always => constraints.is_always_satisfied(self.db, self.env),
                    Satisfaction::Never => constraints.is_never_satisfied(self.db, self.env),
                }),
        )
    }
}

impl effects::sealed::Sealed for Finite<'_, '_> {}

impl<'db> BinderEffects<'db> for Finite<'_, 'db> {
    type Error = Failure;

    fn recursion_guard(&self) -> Option<&CallableRecursionGuard<'db>> {
        None
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Failure> {
        self.legacy(BinderLegacyEffect::KnownFunction, || {
            request.read_ordinary()
        })
    }

    fn legacy<T>(
        &self,
        _effect: BinderLegacyEffect,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Failure> {
        self.control.event("legacy")?;
        let result = operation();
        self.control.event("legacy done")?;
        Ok(result)
    }
    fn inspect_argument_expansions(
        &self,
        arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> Result<bool, Failure> {
        self.control.event("expansions")?;
        let Ok(result) =
            InlineBinderEffects::default().inspect_argument_expansions(arguments, inspect);
        self.control.event("expansions done")?;
        Ok(result)
    }
    fn condition(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _constraints: &ConstraintSetBuilder<'db>,
        _condition: BinderCondition<'db>,
    ) -> impl Future<Output = Result<bool, Failure>> {
        ready(Err(Failure::UnexpectedCondition))
    }
    fn defer_typevartuple_check(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
    ) -> Result<bool, Failure> {
        self.control.event("typevartuple")?;
        let Ok(result) = InlineBinderEffects::default()
            .defer_typevartuple_check(checker, declared, expected, argument);
        self.control.event("typevartuple done")?;
        Ok(result)
    }
    fn trace_span(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _arguments: &CallArguments<'_, 'db>,
        _signature: Type<'db>,
    ) -> tracing::Span {
        tracing::Span::none()
    }
}

impl<'db> BindingsEffects<'db> for Finite<'_, 'db> {
    async fn constructor(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
        mode: CheckTypesMode,
    ) -> Result<(), Failure> {
        self.control.event("constructor")?;
        let Ok(()) = InlineBinderEffects::default()
            .constructor(binding, context, mode)
            .await;
        self.control.event("constructor done")
    }
    async fn known_cases(
        &self,
        bindings: &mut Bindings<'db>,
        context: CheckContext<'_, 'db>,
    ) -> Result<(), Failure> {
        self.control.event("known")?;
        let Ok(()) = InlineBinderEffects::default()
            .known_cases(bindings, context)
            .await;
        self.control.event("known done")
    }
    async fn downstream(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
    ) -> Result<(), Failure> {
        self.control.event("downstream")?;
        let Ok(()) = InlineBinderEffects::default()
            .downstream(binding, context)
            .await;
        self.control.event("downstream done")
    }
    fn step<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Failure> {
        self.control.event("step")?;
        let result = operation();
        self.control.event("stepped")?;
        Ok(result)
    }
}

fn fixture() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/binding.py",
            "def parse(value: int) -> str: ...\nclass Generic[T]:\n    value: T\n",
        )
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/binding.py")?,
        env.program(db),
    );
    Ok(global_symbol(db, file, name).place.expect_type())
}

#[test]
fn scheduled_binder_drives_canonical_checks_and_stops_before_consumption() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let function = symbol(&db, "parse")?;
    for value in [Type::int_literal(1), Type::string_literal(&db, "bad")] {
        let arguments = CallArguments::positional([value]);
        let bindings = function
            .bindings(&db, &env)
            .match_parameters(&db, &env, &arguments);
        let baseline_builder = ConstraintSetBuilder::new();
        let expected = bindings.clone().check_types(
            &db,
            &env,
            &baseline_builder,
            &arguments,
            TypeContext::default(),
            &[],
        );
        let run = |control: &Control| {
            let builder = ConstraintSetBuilder::new();
            let owners = CallRelationOwners::default();
            let finite = Finite {
                control,
                db: &db,
                env: &env,
                builder: &builder,
            };
            let driver = TaskDriver::new(control)?;
            let provider = ScheduledBinder {
                conditions: BinderConditions {
                    endpoint: driver.endpoint(),
                    env: &env,
                    constraints: &builder,
                    owners: &owners,
                    executor: &finite,
                },
                other: &finite,
            };
            let bindings = bindings.clone();
            let db = &db;
            let env = &env;
            let builder = &builder;
            let arguments = &arguments;
            driver.run(move || async move {
                bindings
                    .check_types_with_effects(
                        CheckContext {
                            db,
                            env,
                            constraints: builder,
                            arguments,
                            tcx: TypeContext::default(),
                            dataclass_field_specifiers: &[],
                        },
                        &provider,
                    )
                    .await
            })
        };
        let complete = Control::default();
        let actual = run(&complete).map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        assert!(complete.comparisons.get() > 0);
        let events = complete.events.borrow();
        for index in 0..events.len() {
            let stop = Control {
                stop_at: Some(index),
                ..Control::default()
            };
            assert!(matches!(run(&stop), Err(Failure::Stopped)), "event {index}");
            assert_eq!(*stop.events.borrow(), events[..=index]);
            let retry = Control::default();
            let retried = run(&retry).map_err(|error| anyhow::anyhow!("retry: {error:?}"))?;
            assert_eq!(format!("{retried:?}"), format!("{expected:?}"));
        }
    }
    Ok(())
}

#[test]
fn scheduled_conditions_preserve_eager_roots_and_equivalence() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let class = symbol(&db, "Generic")?
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing Generic"))?;
    let variable = Type::instance(&db, &env, class.identity_specialization(&db))
        .member(&db, &env, "value")
        .place
        .expect_type()
        .as_typevar()
        .ok_or_else(|| anyhow::anyhow!("missing T"))?;
    let inferable = TypeVarSet::from_typevars(&db, [variable]);
    let int = Type::int_literal(1);
    let builder = ConstraintSetBuilder::new();
    let owners = CallRelationOwners::default();
    let control = Control::default();
    let finite = Finite {
        control: &control,
        db: &db,
        env: &env,
        builder: &builder,
    };
    let driver = TaskDriver::new(&control).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let conditions = BinderConditions {
        endpoint: driver.endpoint(),
        env: &env,
        constraints: &builder,
        owners: &owners,
        executor: &finite,
    };
    let db = &db;
    let env = &env;
    let builder = &builder;
    driver
        .run(move || async move {
            for comparison in [
                BinderComparison::Assignable {
                    source: int,
                    target: Type::TypeVar(variable),
                    inferable_typevars: inferable,
                },
                BinderComparison::Assignable {
                    source: int,
                    target: Type::TypeVar(variable),
                    inferable_typevars: TypeVarSet::None,
                },
                BinderComparison::Equivalent {
                    left: int,
                    right: int,
                },
                BinderComparison::Equivalent {
                    left: int,
                    right: Type::string_literal(db, "x"),
                },
            ] {
                for condition in [
                    BinderCondition::Always(comparison),
                    BinderCondition::Never(comparison),
                ] {
                    let expected = condition.evaluate(db, env, builder);
                    assert_eq!(
                        conditions.evaluate(env, builder, condition).await?,
                        expected
                    );
                }
            }
            let wrong_builder = ConstraintSetBuilder::new();
            assert_eq!(
                conditions
                    .evaluate(
                        env,
                        &wrong_builder,
                        BinderCondition::Always(BinderComparison::Equivalent {
                            left: int,
                            right: int
                        })
                    )
                    .await,
                Err(Failure::WrongContext)
            );
            Ok(())
        })
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(())
}
