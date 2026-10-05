//! Runtime controls for dataclass-transform metadata, admission, and invocation ownership.

use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;
use std::pin::pin;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::ZalsaDatabase;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::call::bind::dataclass_transform_observations::{
    self as transform_observations, Event, Recording, Snapshot, Stage,
};
use crate::types::call::bind::ownership::observations::RetirementPhase;
use crate::types::call::bind::source_check::SourceCheckerAccess;
use crate::types::call::invocation::InvocationEffects;
use crate::types::cyclic::guard_storage::observations as guard_observations;
use crate::types::function::{DataclassTransformerFlags, DataclassTransformerParams};

const EMPTY_FACTORY: &str =
    "from typing import dataclass_transform\nready = dataclass_transform()\nleft = right = ready\n";
const NONEMPTY_FACTORY: &str = "from typing import dataclass_transform\ndef field(): ...\nready = dataclass_transform(field_specifiers=(field,))\nleft = right = ready\n";
const SYNTHETIC_FACTORY: &str = "def __dataclass_transform__(eq_default, order_default, kw_only_default, frozen_default, field_specifiers): ...\n";

fn database(source: &str) -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("src/main.py", source)?;
    Ok(db)
}

/// Supplies distinct literal types with a duplicate to expose changes in stored order or multiplicity.
/// The unannotated compatibility factory accepts this payload without requiring callable fields.
fn supplied_fields() -> [Type<'static>; 3] {
    [
        Type::bool_literal(true),
        Type::int_literal(7),
        Type::bool_literal(true),
    ]
}

fn supplied_flags() -> DataclassTransformerFlags {
    DataclassTransformerFlags::ORDER_DEFAULT | DataclassTransformerFlags::FROZEN_DEFAULT
}

/// Counts canonical dataclass-transformer values stored in the original Salsa ingredient.
fn parameter_count(db: &TestDb) -> usize {
    DataclassTransformerParams::ingredient(db.zalsa())
        .entries(db.zalsa())
        .count()
}

/// Supplies an already-inferred callable and arguments to the ordinary controlled invocation owner.
/// Records its supplied guard and actual suspensions so tests can compare owner lifetimes.
#[derive(Debug)]
struct SyntheticCall<'a, 'db> {
    callable: Type<'db>,
    arguments: &'a CallArguments<'a, 'db>,
}

impl<'db> MemberOperation<'db> for SyntheticCall<'_, 'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        let guard = InvocationEffects::new_guard(&effects).await?;
        transform_observations::supplied_guard(&guard);
        let mut invocation =
            pin!(effects.synthetic_call(&env, self.callable, self.arguments, Some(&guard)));
        let result = poll_fn(|cx| {
            let result = invocation.as_mut().poll(cx);
            if result.is_pending() {
                transform_observations::suspended();
            }
            result
        })
        .await?;
        let bindings =
            result.map_err(|_| RunError::Contract("synthetic dataclass-transform call failed"))?;
        SourceCheckerAccess::bindings_return_type(&effects, access.db(), &env, &bindings).await
    }
}

/// Creates only the retained inputs; the dataclass-transform factory is first called inside the run.
fn synthetic_inputs<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> anyhow::Result<(Type<'db>, CallArguments<'static, 'db>)> {
    let [Stmt::FunctionDef(function)] = prepared.parsed_module().syntax().body.as_slice() else {
        anyhow::bail!("synthetic fixture must define its compatibility factory");
    };
    let definition = prepared.semantic_index().expect_single_definition(function);
    let callable = crate::types::binding_type(db, definition);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let tuple = Type::heterogeneous_tuple(db, &env, supplied_fields());
    let arguments = CallArguments::positional([
        Type::bool_literal(false),
        Type::bool_literal(true),
        Type::bool_literal(false),
        Type::bool_literal(true),
        tuple,
    ]);
    Ok((callable, arguments))
}

fn callable_id(callable: Type<'_>) -> anyhow::Result<salsa::Id> {
    let Type::FunctionLiteral(function) = callable else {
        anyhow::bail!("synthetic callable must be a function literal");
    };
    Ok(function.as_id())
}

/// Extracts canonical parameters only from a completed dataclass-transform result.
fn completed<'db>(
    result: Result<AnalysisOutcome<Type<'db>>, AnalysisFailure>,
) -> anyhow::Result<DataclassTransformerParams<'db>> {
    match result {
        Ok(AnalysisOutcome::Complete(Type::DataclassTransformer(params))) => Ok(params),
        result => anyhow::bail!("dataclass-transform factory did not complete: {result:?}"),
    }
}

/// Checks that drainage retires the helper before its bindings, then its guard, with no active attempt.
fn assert_interrupted_cleanup(snapshot: &Snapshot, helper_constructed: bool) -> anyhow::Result<()> {
    assert_eq!(snapshot.live_helpers, 0);
    let guard = snapshot
        .guard
        .ok_or_else(|| anyhow::anyhow!("no invocation guard observed"))?;
    let guards = guard_observations::snapshot();
    assert!(!guards.overflowed);
    assert_eq!(guards.active_scopes, 0);
    assert!(guards.events.iter().flatten().any(|event| {
        matches!(event, guard_observations::Event::StorageDropped {
            id, outstanding_removal_weights: [0, 0, 0],
        } if *id == guard)
    }));
    let binding_begin = snapshot
        .events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::BindingsRetired {
                    phase: RetirementPhase::Begin,
                    helper_live: false,
                    guard_retired: false
                }
            )
        })
        .ok_or_else(|| anyhow::anyhow!("bindings did not retire before the guard: {snapshot:?}"))?;
    assert!(snapshot.events.iter().any(|event| matches!(
        event,
        Event::BindingsRetired {
            phase: RetirementPhase::Complete,
            helper_live: false,
            guard_retired: false
        }
    )));
    if helper_constructed {
        let helper_end = snapshot
            .events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    Event::HelperEnded {
                        guard_retired: false
                    }
                )
            })
            .ok_or_else(|| {
                anyhow::anyhow!("helper did not retire before the guard: {snapshot:?}")
            })?;
        assert!(helper_end < binding_begin);
    } else {
        assert!(
            !snapshot
                .events
                .iter()
                .any(|event| matches!(event, Event::HelperStarted))
        );
    }
    assert!(
        invocation_observations::invocation_snapshot()
            .events
            .iter()
            .flatten()
            .any(|event| { event.stage == invocation_observations::InvocationStage::BinderCheck })
    );
    assert_no_active_attempt();
    Ok(())
}

/// Returns the stages and allowances observed by a fresh, possibly interrupted synthetic invocation.
/// Fresh databases prevent calibration from reusing earlier canonical parameters.
fn calibration(policy: &AnalysisPolicy) -> anyhow::Result<Snapshot> {
    let db = database(SYNTHETIC_FACTORY)?;
    let prepared = prepare(&db);
    let (callable, arguments) = synthetic_inputs(&db, &prepared)?;
    let recording = Recording::start(&db, Some(callable_id(callable)?), None);
    let _result = controlled_member_operation(
        &prepared,
        SyntheticCall {
            callable,
            arguments: &arguments,
        },
        policy,
    );
    drop(recording);
    assert_no_active_attempt();
    Ok(transform_observations::snapshot())
}

#[derive(Clone, Copy, Debug)]
enum Limit {
    Work,
    Bytes,
}

/// Leaves no budget of the selected kind after reaching the requested boundary.
fn refusal_policy(target: Stage, limit: Limit) -> anyhow::Result<AnalysisPolicy> {
    let mut policy = funded();
    match limit {
        Limit::Work => {
            let remaining = calibration(&policy)?
                .remaining_at(target)
                .ok_or_else(|| anyhow::anyhow!("calibration did not reach {target:?}"))?;
            policy.semantic_work_limit = policy
                .semantic_work_limit
                .checked_sub(remaining)
                .ok_or_else(|| anyhow::anyhow!("missing work before {target:?}"))?;
        }
        Limit::Bytes => {
            let mut lower = 0;
            let mut upper = policy.requested_bytes_limit;
            if !calibration(&policy)?.reached(target) {
                anyhow::bail!("calibration did not reach {target:?}");
            }
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                policy.requested_bytes_limit = middle;
                if calibration(&policy)?.reached(target) {
                    upper = middle;
                } else {
                    lower = middle + 1;
                }
            }
            policy.requested_bytes_limit = lower;
        }
    }
    Ok(policy)
}

// A cold real factory call publishes its definition memo and canonical parameters; another call
// reuses them in the same revision. Ordinary construction follows the first controlled call, so it
// cannot supply that result.
#[test]
fn cold_empty_factory_publishes_canonical_metadata() -> anyhow::Result<()> {
    let db = database(EMPTY_FACTORY)?;
    let prepared = prepare(&db);
    let [_, Stmt::Assign(assignment), _] = prepared.parsed_module().syntax().body.as_slice() else {
        anyhow::bail!("empty factory fixture must assign and read its result");
    };
    let definition = assignment_definition(&prepared, assignment);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let actual = completed(expression_type_with_policy(
        &prepared,
        expression_key(&prepared),
        &funded(),
    ))?;
    assert_eq!(actual.flags(&db), DataclassTransformerFlags::EQ_DEFAULT);
    assert!(actual.field_specifiers(&db).is_empty());
    assert_eq!(
        actual,
        DataclassTransformerParams::new(
            &db,
            DataclassTransformerFlags::EQ_DEFAULT,
            Box::<[Type<'_>]>::from([])
        )
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .is_ok()
    );
    let memo = infer_definition_types(&db, definition);
    assert_eq!(
        memo.binding_type(definition),
        Type::DataclassTransformer(actual)
    );
    assert_eq!(
        completed(expression_type_with_policy(
            &prepared,
            expression_key(&prepared),
            &funded()
        ))?,
        actual
    );
    assert!(std::ptr::eq(memo, infer_definition_types(&db, definition)));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    Ok(())
}

// A cold source call completes with its function field specifier retained in canonical metadata.
// Keeping tuple inference inside this call ensures an unavailable preparation dependency fails this control.
#[test]
fn cold_nonempty_factory_publishes_canonical_metadata() -> anyhow::Result<()> {
    let db = database(NONEMPTY_FACTORY)?;
    let prepared = prepare(&db);
    let [_, Stmt::FunctionDef(field), _, _] = prepared.parsed_module().syntax().body.as_slice()
    else {
        anyhow::bail!("nonempty fixture must define a field specifier");
    };
    let actual = completed(expression_type_with_policy(
        &prepared,
        expression_key(&prepared),
        &funded(),
    ))?;
    let field = crate::types::binding_type(
        &db,
        prepared.semantic_index().expect_single_definition(field),
    );
    assert_eq!(actual.flags(&db), DataclassTransformerFlags::EQ_DEFAULT);
    assert_eq!(actual.field_specifiers(&db), &[field]);
    assert_eq!(
        actual,
        DataclassTransformerParams::new(
            &db,
            DataclassTransformerFlags::EQ_DEFAULT,
            Box::<[Type<'_>]>::from([field])
        )
    );
    assert_no_active_attempt();
    Ok(())
}

// Already-inferred inputs isolate the production helper from tuple-expression inference, while the
// existing synthetic invocation still supplies its bindings, argument checker, and callable guard.
// The result preserves flags, ordered duplicate fields, and the ordinary constructor's canonical identity.
#[test]
fn synthetic_nonempty_factory_preserves_canonical_metadata() -> anyhow::Result<()> {
    let db = database(SYNTHETIC_FACTORY)?;
    let prepared = prepare(&db);
    let (callable, arguments) = synthetic_inputs(&db, &prepared)?;
    let before = parameter_count(&db);
    invocation_observations::reset_invocations();
    let recording = Recording::start(&db, Some(callable_id(callable)?), None);
    let actual = completed(controlled_member_operation(
        &prepared,
        SyntheticCall {
            callable,
            arguments: &arguments,
        },
        &funded(),
    ))?;
    drop(recording);
    assert_eq!(
        transform_observations::snapshot()
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Stage { stage, len, .. } => Some((*stage, *len)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            (Stage::BeforeBuffer, 3),
            (Stage::BufferReady, 0),
            (Stage::Populated, 3),
            (Stage::BeforeInterner, 3),
            (Stage::BeforeReturn, 3),
            (Stage::Returned, 3),
        ]
    );
    assert_eq!(actual.flags(&db), supplied_flags());
    assert_eq!(actual.field_specifiers(&db), &supplied_fields());
    assert_eq!(parameter_count(&db), before + 1);
    assert_eq!(
        actual,
        DataclassTransformerParams::new(
            &db,
            supplied_flags(),
            Box::<[Type<'_>]>::from(supplied_fields())
        )
    );
    assert!(
        invocation_observations::invocation_snapshot()
            .events
            .iter()
            .flatten()
            .any(|event| { event.stage == invocation_observations::InvocationStage::BinderCheck })
    );
    assert_no_active_attempt();
    Ok(())
}

// Independent work and byte limits interrupt real admissions at each ownership boundary. A late
// refusal may leave canonical parameters interned, but cannot return a completed call result.
// After allocation, suspension keeps the helper's local owner, bindings, and guard live. Drainage
// retires the helper and bindings before the guard, and a funded retry in the same revision returns
// the canonical metadata.
#[test_case::test_case(Stage::BeforeBuffer, Stage::BufferReady, Limit::Work; "work before buffer")]
#[test_case::test_case(Stage::BeforeBuffer, Stage::BufferReady, Limit::Bytes; "bytes before buffer")]
#[test_case::test_case(Stage::Populated, Stage::BeforeInterner, Limit::Work; "work with populated buffer")]
#[test_case::test_case(Stage::Populated, Stage::BeforeInterner, Limit::Bytes; "bytes with populated buffer")]
#[test_case::test_case(Stage::BeforeInterner, Stage::BeforeReturn, Limit::Work; "work before interner")]
#[test_case::test_case(Stage::BeforeInterner, Stage::BeforeReturn, Limit::Bytes; "bytes before interner")]
#[test_case::test_case(Stage::BeforeReturn, Stage::Returned, Limit::Work; "work before return")]
#[test_case::test_case(Stage::BeforeReturn, Stage::Returned, Limit::Bytes; "bytes before return")]
fn interrupted_nonempty_factory_retains_owners_and_retries(
    target: Stage,
    successor: Stage,
    limit: Limit,
) -> anyhow::Result<()> {
    let policy = refusal_policy(target, limit)?;
    let db = database(SYNTHETIC_FACTORY)?;
    let prepared = prepare(&db);
    let (callable, arguments) = synthetic_inputs(&db, &prepared)?;
    let revision = salsa::plumbing::current_revision(&db);
    let before = parameter_count(&db);
    invocation_observations::reset_invocations();
    let recording = Recording::start(&db, Some(callable_id(callable)?), None);
    let result = controlled_member_operation(
        &prepared,
        SyntheticCall {
            callable,
            arguments: &arguments,
        },
        &policy,
    );
    let snapshot = transform_observations::snapshot();
    let reason = match limit {
        Limit::Work => AnalysisIncomplete::WorkLimit,
        Limit::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
    };
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason,
            completed: ()
        })
    );
    assert!(snapshot.reached(target), "{snapshot:?}");
    assert!(!snapshot.reached(successor), "{snapshot:?}");
    assert!(!snapshot.reached(Stage::Returned));
    assert_interrupted_cleanup(&snapshot, target != Stage::BeforeBuffer)?;
    if target != Stage::BeforeBuffer {
        assert!(
            snapshot.events.iter().any(|event| matches!(
                event,
                Event::Suspended {
                    helper_live: true,
                    guard_retired: false
                }
            )),
            "the retained helper never suspended: {snapshot:?}"
        );
    }
    assert_eq!(
        parameter_count(&db),
        before + usize::from(target == Stage::BeforeReturn)
    );
    drop(recording);
    let actual = completed(controlled_member_operation(
        &prepared,
        SyntheticCall {
            callable,
            arguments: &arguments,
        },
        &funded(),
    ))?;
    assert_eq!(actual.flags(&db), supplied_flags());
    assert_eq!(actual.field_specifiers(&db), &supplied_fields());
    assert_eq!(parameter_count(&db), before + 1);
    assert_eq!(
        actual,
        DataclassTransformerParams::new(
            &db,
            supplied_flags(),
            Box::<[Type<'_>]>::from(supplied_fields())
        )
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    Ok(())
}

// Cancellation is armed with a populated buffer and takes effect at the next normal runtime
// boundary. The helper and bindings retire before their invocation guard, and the call retries in the same revision.
#[test]
fn cancelled_nonempty_factory_retains_owners_and_retries() -> anyhow::Result<()> {
    let db = database(SYNTHETIC_FACTORY)?;
    let prepared = prepare(&db);
    let (callable, arguments) = synthetic_inputs(&db, &prepared)?;
    let revision = salsa::plumbing::current_revision(&db);
    let before = parameter_count(&db);
    invocation_observations::reset_invocations();
    let recording = Recording::start(&db, Some(callable_id(callable)?), Some(Stage::Populated));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(
            &prepared,
            SyntheticCall {
                callable,
                arguments: &arguments,
            },
            &funded(),
        )
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let snapshot = transform_observations::snapshot();
    assert!(snapshot.reached(Stage::Populated));
    assert!(!snapshot.reached(Stage::BeforeInterner));
    assert_interrupted_cleanup(&snapshot, true)?;
    assert_eq!(parameter_count(&db), before);
    drop(recording);
    let actual = completed(controlled_member_operation(
        &prepared,
        SyntheticCall {
            callable,
            arguments: &arguments,
        },
        &funded(),
    ))?;
    assert_eq!(
        actual,
        DataclassTransformerParams::new(
            &db,
            supplied_flags(),
            Box::<[Type<'_>]>::from(supplied_fields())
        )
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    Ok(())
}

// Refusing the final return assignment in a real call leaves its assignment memo unpublished,
// even though the canonical empty parameter value can already be reused by the same-revision retry.
#[test]
fn refused_real_factory_withholds_parent_memo_and_retries() -> anyhow::Result<()> {
    let measured_db = database(EMPTY_FACTORY)?;
    let measured_prepared = prepare(&measured_db);
    let recording = Recording::start(&measured_db, None, None);
    completed(expression_type_with_policy(
        &measured_prepared,
        expression_key(&measured_prepared),
        &funded(),
    ))?;
    drop(recording);
    let remaining = transform_observations::snapshot()
        .remaining_at(Stage::BeforeReturn)
        .ok_or_else(|| {
            anyhow::anyhow!("real factory calibration did not reach return assignment")
        })?;
    let limit = funded()
        .semantic_work_limit
        .checked_sub(remaining)
        .ok_or_else(|| anyhow::anyhow!("invalid real factory work measurement"))?;

    let db = database(EMPTY_FACTORY)?;
    let prepared = prepare(&db);
    let [_, Stmt::Assign(assignment), _] = prepared.parsed_module().syntax().body.as_slice() else {
        anyhow::bail!("empty factory fixture must assign and read its result");
    };
    let definition = assignment_definition(&prepared, assignment);
    let revision = salsa::plumbing::current_revision(&db);
    let before = parameter_count(&db);
    let recording = Recording::start(&db, None, None);
    let result = expression_type_with_policy(
        &prepared,
        expression_key(&prepared),
        &AnalysisPolicy {
            semantic_work_limit: limit,
            ..funded()
        },
    );
    drop(recording);
    let snapshot = transform_observations::snapshot();
    assert!(snapshot.reached(Stage::BeforeReturn), "{snapshot:?}");
    assert!(!snapshot.reached(Stage::Returned));
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_eq!(parameter_count(&db), before + 1);
    assert_no_active_attempt();
    let actual = completed(expression_type_with_policy(
        &prepared,
        expression_key(&prepared),
        &funded(),
    ))?;
    assert_eq!(parameter_count(&db), before + 1);
    assert_eq!(
        actual,
        DataclassTransformerParams::new(
            &db,
            DataclassTransformerFlags::EQ_DEFAULT,
            Box::<[Type<'_>]>::from([])
        )
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    Ok(())
}

// Admitting dataclass-transform metadata must preserve the separate refusal of repr customization.
#[test]
fn unrelated_known_function_remains_refused() -> anyhow::Result<()> {
    let db = database("ready = repr(True)\nleft = right = ready\n")?;
    let prepared = prepare(&db);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(unavailable(OperationId::CheckerKnownFunction))
    );
    assert_no_active_attempt();
    Ok(())
}
