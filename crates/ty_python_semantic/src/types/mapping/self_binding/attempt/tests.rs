use std::future::Future;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{AttemptSelfBindingEffects, UnsupportedSelfBindingOperation};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::mapping::self_binding::{prepare_with, should_bind_with};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{
    BindingContext, TypeVarBoundOrConstraintsEvaluation, TypeVarIdentity, TypeVarInstance,
    TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, KnownInstanceType, SelfBinding, Type,
    class_mro_literals,
};
use crate::{Db, ProgramEnvironment};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/self_binding_attempt.py",
            "class Owner: ...\nclass Receiver(Owner): ...\nclass Unrelated: ...\nclass Invalid(1): ...\ntype OwnerAlias = Owner\nDynamicReceiver = type(\"DynamicReceiver\", (Owner,), {})\n",
        )
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/self_binding_attempt.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .ok_or_else(|| anyhow::anyhow!("missing source symbol {name}"))
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassLiteral<'db>> {
    symbol(db, name)?
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("{name} did not produce a class literal"))
}

fn instance<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    Ok(Type::instance(
        db,
        &env,
        ClassType::NonGeneric(class(db, name)?),
    ))
}

fn self_variable<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    upper_bound: Type<'db>,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::synthetic_self(
        db,
        upper_bound,
        BindingContext::Synthetic(env.program(db)),
    )
}

fn lazy_self_variable<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> BoundTypeVarInstance<'db> {
    let variable = TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static("Self"), None, TypeVarKind::TypingSelf),
        Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
        None,
        None,
    );
    BoundTypeVarInstance::new(
        db,
        variable,
        BindingContext::Synthetic(env.program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

fn attempt<T>(
    db: &dyn Db,
    allowance: usize,
    future: impl Future<Output = Result<T, Incomplete>>,
) -> anyhow::Result<Result<T, Incomplete>> {
    let (result, _) = expansion_probe::run_mro(db, allowance, || try_poll_immediate(future));
    match result {
        Err(error) => Ok(Err(error)),
        Ok(Poll::Ready(result)) => Ok(result),
        Ok(Poll::Pending) => anyhow::bail!("synchronous Self ownership unexpectedly suspended"),
    }
}

fn completed<T>(
    db: &dyn Db,
    future: impl Future<Output = Result<T, Incomplete>>,
) -> anyhow::Result<T> {
    attempt(db, 10_000, future)?.map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            let name = db.ingredient_debug_name(database_key.ingredient_index());
            Some(
                name.rsplit("::")
                    .next()
                    .unwrap_or(name.as_ref())
                    .trim_end_matches('_')
                    .to_owned(),
            )
        })
        .collect()
}

#[test]
fn stored_instance_preparation_and_matching_retain_ordinary_decisions() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    // Instance construction resolves inheritance flags. This fixture deliberately starts with
    // those ordinary class facts cached; cold receiver MRO reads are covered separately below.
    let owner = instance(&db, "Owner")?;
    let receiver = instance(&db, "Receiver")?;
    let unrelated = instance(&db, "Unrelated")?;
    let invalid = instance(&db, "Invalid")?;
    let variables = [
        self_variable(&db, &env, owner),
        self_variable(&db, &env, receiver),
        self_variable(&db, &env, unrelated),
        self_variable(&db, &env, invalid),
        self_variable(&db, &env, Type::unknown()),
    ];
    let effects = AttemptSelfBindingEffects::new(&db);
    for receiver in [
        owner,
        receiver,
        unrelated,
        invalid,
        Type::unknown(),
        Type::TypeVar(variables[0]),
    ] {
        for context in [None, Some(BindingContext::Synthetic(env.program(&db)))] {
            let actual = completed(&db, prepare_with(&db, &env, receiver, context, &effects))?;
            let expected = SelfBinding::new(&db, &env, receiver, context);
            assert_eq!(actual, expected);
            for variable in variables {
                assert_eq!(
                    completed(
                        &db,
                        should_bind_with(&db, &env, &actual, variable, &effects),
                    )?,
                    expected.should_bind(&db, &env, variable),
                );
            }
        }
    }
    Ok(())
}

fn cold_receiver_reads(installed: bool) -> anyhow::Result<(bool, Vec<String>)> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = class(&db, "Receiver")?;
    let variable = self_variable(&db, &env, instance(&db, "Owner")?);
    // Matching uses the stored class literal, not `ty`. Constructing a receiver instance here
    // would resolve its inheritance flags and warm the very MRO this fixture measures.
    let binding = SelfBinding {
        ty: Type::unknown(),
        class_literal: Some(receiver),
        binding_context: None,
    };
    executions(&db);
    let result = if installed {
        completed(
            &db,
            should_bind_with(
                &db,
                &env,
                &binding,
                variable,
                &AttemptSelfBindingEffects::new(&db),
            ),
        )?
    } else {
        binding.should_bind(&db, &env, variable)
    };
    Ok((result, executions(&db)))
}

#[test]
fn inherited_self_reads_the_cold_receiver_mro_in_ordinary_source_order() -> anyhow::Result<()> {
    let (ordinary, ordinary_reads) = cold_receiver_reads(false)?;
    let (installed, installed_reads) = cold_receiver_reads(true)?;
    assert!(ordinary);
    assert_eq!(installed, ordinary);
    assert_eq!(installed_reads, ordinary_reads);
    let wrapper = installed_reads
        .iter()
        .position(|name| name == "class_mro_literals")
        .ok_or_else(|| anyhow::anyhow!("missing class MRO read: {installed_reads:?}"))?;
    let source = installed_reads
        .iter()
        .position(|name| name == "try_mro_unspecialized")
        .ok_or_else(|| anyhow::anyhow!("receiver MRO was already warm: {installed_reads:?}"))?;
    assert!(wrapper < source);
    Ok(())
}

#[test]
fn unsupported_owner_is_not_absence_and_follows_the_mro_read() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = class(&db, "Receiver")?;
    let owner = instance(&db, "Owner")?;
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) = symbol(&db, "OwnerAlias")?
    else {
        anyhow::bail!("OwnerAlias did not retain its runtime alias handle");
    };
    let variable = self_variable(&db, &env, Type::TypeAlias(alias));
    let binding = SelfBinding {
        ty: Type::unknown(),
        class_literal: Some(receiver),
        binding_context: None,
    };
    let effects = AttemptSelfBindingEffects::new(&db);
    executions(&db);
    for retry in 0..2 {
        assert_eq!(
            attempt(
                &db,
                10_000,
                should_bind_with(&db, &env, &binding, variable, &effects),
            )?,
            Err(Incomplete::UnsupportedSelfBindingOperation(
                UnsupportedSelfBindingOperation::AliasOwner,
            )),
        );
        let reads = executions(&db);
        assert!(
            !reads.iter().any(|name| name == "raw_value_type"),
            "{reads:?}"
        );
        assert_eq!(
            reads.iter().any(|name| name == "class_mro_literals"),
            retry == 0,
            "{reads:?}",
        );
    }
    assert!(completed(
        &db,
        should_bind_with(
            &db,
            &env,
            &binding,
            self_variable(&db, &env, owner),
            &effects,
        ),
    )?);
    assert!(executions(&db).is_empty());
    Ok(())
}

#[test]
fn matching_context_and_missing_receiver_skip_unsupported_owner_reads() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let variable = lazy_self_variable(&db, &env);
    let effects = AttemptSelfBindingEffects::new(&db);
    let receiver = class(&db, "Receiver")?;
    for (class_literal, binding_context, expected) in [
        (Some(receiver), Some(variable.binding_context(&db)), true),
        (None, None, false),
    ] {
        let binding = SelfBinding {
            ty: Type::unknown(),
            class_literal,
            binding_context,
        };
        executions(&db);
        assert_eq!(
            completed(
                &db,
                should_bind_with(&db, &env, &binding, variable, &effects),
            )?,
            expected,
        );
        assert!(executions(&db).is_empty());
    }
    Ok(())
}

#[test]
fn lazy_upper_bound_is_refused_before_the_ordinary_helper() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let variable = lazy_self_variable(&db, &env);
    executions(&db);
    assert_eq!(
        attempt(
            &db,
            10_000,
            prepare_with(
                &db,
                &env,
                Type::TypeVar(variable),
                None,
                &AttemptSelfBindingEffects::new(&db),
            ),
        )?,
        Err(Incomplete::UnsupportedSelfBindingOperation(
            UnsupportedSelfBindingOperation::LazyUpperBound,
        )),
    );
    assert!(executions(&db).is_empty());
    Ok(())
}

#[test]
fn owner_paths_that_need_source_or_dynamic_mro_remain_explicit() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let dynamic = instance(&db, "DynamicReceiver")?;
    for (receiver, operation) in [
        (
            Type::object(),
            UnsupportedSelfBindingOperation::BuiltinOwner,
        ),
        (dynamic, UnsupportedSelfBindingOperation::DynamicOwner),
    ] {
        executions(&db);
        assert_eq!(
            attempt(
                &db,
                10_000,
                prepare_with(
                    &db,
                    &env,
                    receiver,
                    None,
                    &AttemptSelfBindingEffects::new(&db),
                ),
            )?,
            Err(Incomplete::UnsupportedSelfBindingOperation(operation)),
        );
        assert!(executions(&db).is_empty());
    }
    Ok(())
}

#[test]
fn interrupted_source_read_retries_without_publishing_a_boolean() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = class(&db, "Receiver")?;
    let variable = self_variable(&db, &env, instance(&db, "Owner")?);
    let binding = SelfBinding {
        ty: Type::unknown(),
        class_literal: Some(receiver),
        binding_context: None,
    };
    let effects = AttemptSelfBindingEffects::new(&db);
    executions(&db);
    assert_eq!(
        attempt(
            &db,
            1,
            should_bind_with(&db, &env, &binding, variable, &effects),
        )?,
        Err(Incomplete::Allowance),
    );
    let reads = executions(&db);
    assert!(
        reads.iter().any(|name| name == "class_mro_literals"),
        "{reads:?}"
    );
    for retry in 0..2 {
        assert!(completed(
            &db,
            should_bind_with(&db, &env, &binding, variable, &effects),
        )?);
        let reads = executions(&db);
        assert_eq!(
            reads.iter().any(|name| name == "class_mro_literals"),
            retry == 0,
            "{reads:?}",
        );
    }
    Ok(())
}

#[test]
fn membership_refusal_keeps_a_completed_source_mro_for_retry() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Receiver")?;
    let variable = self_variable(&db, &env, instance(&db, "Owner")?);
    let binding = SelfBinding::new(&db, &env, receiver, None);
    let receiver_class = class(&db, "Receiver")?;
    class_mro_literals(&db, receiver_class);
    let effects = AttemptSelfBindingEffects::new(&db);
    executions(&db);
    // Matching, eager-bound inspection, and nominal-owner inspection consume three units.
    // The fourth compares Receiver; the comparison with Owner then refuses.
    assert_eq!(
        attempt(
            &db,
            4,
            should_bind_with(&db, &env, &binding, variable, &effects),
        )?,
        Err(Incomplete::Allowance),
    );
    assert!(executions(&db).is_empty());
    assert!(completed(
        &db,
        should_bind_with(&db, &env, &binding, variable, &effects),
    )?);
    assert!(executions(&db).is_empty());
    Ok(())
}
