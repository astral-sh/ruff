use std::cell::RefCell;
use std::fmt::Debug;
use std::future::{Future, ready};
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{
    InlineSelfBindingEffects, SelfBindingEffects, SelfBindingWork, prepare_with, sealed,
    should_bind_with,
};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{
    BindingContext, TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, KnownInstanceType, SelfBinding, Type,
    class_mro_literals, self_typevar_owner_class_literal,
};
use crate::{Db, ProgramEnvironment};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/self_binding.py",
            "class Owner: ...\nclass Receiver(Owner): ...\nclass Unrelated: ...\nclass Invalid(1): ...\ntype OwnerAlias = Owner\nclass GenericOwner[T]: ...\ngeneric_first: GenericOwner[Owner]\ngeneric_second: GenericOwner[Unrelated]\nDynamicReceiver = type(\"DynamicReceiver\", (Owner,), {})\n",
        )
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/self_binding.py")?,
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

fn ordinary_variable<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> BoundTypeVarInstance<'db> {
    let variable = TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static("T"), None, TypeVarKind::Pep695TypeVar),
        None,
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

// These two bodies retain the decisions from before the effects extraction. Calling the
// public adapters would compare the shared implementation with itself.
fn original_prepare<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    self_type: Type<'db>,
    binding_context: Option<BindingContext<'db>>,
) -> SelfBinding<'db> {
    let class_literal = match self_type {
        Type::TypeVar(variable) if variable.typevar(db).is_self(db) => {
            self_typevar_owner_class_literal(db, env, variable)
        }
        _ => self_type
            .nominal_class(db, env)
            .map(|class| class.class_literal(db)),
    };
    SelfBinding {
        ty: self_type,
        class_literal,
        binding_context,
    }
}

fn original_should_bind<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    binding: &SelfBinding<'db>,
    variable: BoundTypeVarInstance<'db>,
) -> bool {
    if !variable.typevar(db).is_self(db) {
        return false;
    }
    if binding.binding_context == Some(variable.binding_context(db)) {
        return true;
    }
    binding.class_literal.is_some_and(|class| {
        let mro = class_mro_literals(db, class);
        self_typevar_owner_class_literal(db, env, variable).is_none_or(|owner| mro.contains(&owner))
    })
}

fn immediately_ready<T, E: Debug>(future: impl Future<Output = Result<T, E>>) -> anyhow::Result<T> {
    match try_poll_immediate(future) {
        Poll::Ready(Ok(value)) => Ok(value),
        Poll::Ready(Err(error)) => anyhow::bail!("unexpected binding error: {error:?}"),
        Poll::Pending => anyhow::bail!("ordinary binding suspended"),
    }
}

#[test]
fn ordinary_decisions_match_the_original_bodies_and_are_immediately_ready() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let owner = instance(&db, "Owner")?;
    let receiver = instance(&db, "Receiver")?;
    let unrelated = instance(&db, "Unrelated")?;
    let invalid = instance(&db, "Invalid")?;
    let generic_first = symbol(&db, "generic_first")?;
    let generic_second = symbol(&db, "generic_second")?;
    let dynamic = instance(&db, "DynamicReceiver")?;
    let owner_self = self_variable(&db, &env, owner);
    let variables = [
        owner_self,
        self_variable(&db, &env, receiver),
        self_variable(&db, &env, unrelated),
        self_variable(&db, &env, invalid),
        self_variable(&db, &env, generic_first),
        self_variable(&db, &env, generic_second),
        self_variable(&db, &env, dynamic),
        self_variable(&db, &env, Type::unknown()),
        ordinary_variable(&db, &env),
    ];
    let context = BindingContext::Synthetic(env.program(&db));
    for receiver in [
        owner,
        receiver,
        unrelated,
        invalid,
        generic_first,
        generic_second,
        dynamic,
        Type::unknown(),
        Type::TypeVar(owner_self),
    ] {
        for binding_context in [None, Some(context)] {
            let expected = original_prepare(&db, &env, receiver, binding_context);
            assert_eq!(
                try_poll_immediate(prepare_with(
                    &db,
                    &env,
                    receiver,
                    binding_context,
                    &InlineSelfBindingEffects,
                )),
                Poll::Ready(Ok(expected.clone())),
            );
            assert_eq!(
                SelfBinding::new(&db, &env, receiver, binding_context),
                expected
            );
            for variable in variables {
                let expected_match = original_should_bind(&db, &env, &expected, variable);
                assert_eq!(
                    try_poll_immediate(should_bind_with(
                        &db,
                        &env,
                        &expected,
                        variable,
                        &InlineSelfBindingEffects,
                    )),
                    Poll::Ready(Ok(expected_match)),
                );
                assert_eq!(expected.should_bind(&db, &env, variable), expected_match);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Call<'db> {
    Work(SelfBindingWork),
    NominalOwner(Type<'db>),
    SelfOwner(BoundTypeVarInstance<'db>),
    Mro(ClassLiteral<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

#[derive(Default)]
struct Recording<'db> {
    calls: RefCell<Vec<Call<'db>>>,
    reads: RefCell<Vec<Call<'db>>>,
    refuse_at: Option<usize>,
}

impl<'db> Recording<'db> {
    fn refusing(index: usize) -> Self {
        Self {
            refuse_at: Some(index),
            ..Self::default()
        }
    }

    fn enter(&self, call: Call<'db>) -> Result<(), Refused> {
        let mut calls = self.calls.borrow_mut();
        let index = calls.len();
        calls.push(call);
        if self.refuse_at == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }

    fn read<T>(&self, call: Call<'db>, source: impl FnOnce() -> T) -> Result<T, Refused> {
        self.enter(call)?;
        self.reads.borrow_mut().push(call);
        Ok(source())
    }
}

impl sealed::Sealed for Recording<'_> {}

impl<'db> SelfBindingEffects<'db> for Recording<'db> {
    type Error = Refused;

    fn checkpoint(&self, work: SelfBindingWork) -> impl Future<Output = Result<(), Self::Error>> {
        ready(self.enter(Call::Work(work)))
    }

    fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(variable.typevar(db).is_self(db)))
    }

    fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<BindingContext<'db>, Self::Error>> {
        ready(Ok(variable.binding_context(db)))
    }

    fn nominal_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Self::Error>> {
        ready(self.read(Call::NominalOwner(ty), || {
            ty.nominal_class(db, env)
                .map(|class| class.class_literal(db))
        }))
    }

    fn self_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Self::Error>> {
        ready(self.read(Call::SelfOwner(variable), || {
            self_typevar_owner_class_literal(db, env, variable)
        }))
    }

    fn mro_literals(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<&'db [ClassLiteral<'db>], Self::Error>> {
        ready(self.read(Call::Mro(class), || class_mro_literals(db, class).as_ref()))
    }
}

#[test]
fn preparation_selects_one_owner_read_and_propagates_refusal() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Receiver")?;
    let variable = self_variable(&db, &env, receiver);
    for (receiver, read) in [
        (receiver, Call::NominalOwner(receiver)),
        (Type::TypeVar(variable), Call::SelfOwner(variable)),
    ] {
        let effects = Recording::default();
        let binding = immediately_ready(prepare_with(&db, &env, receiver, None, &effects))?;
        assert_eq!(binding, original_prepare(&db, &env, receiver, None));
        let expected = [Call::Work(SelfBindingWork::Prepare), read];
        assert_eq!(*effects.calls.borrow(), expected);
        assert_eq!(*effects.reads.borrow(), [read]);

        for refusal in 0..expected.len() {
            let effects = Recording::refusing(refusal);
            assert_eq!(
                try_poll_immediate(prepare_with(&db, &env, receiver, None, &effects)),
                Poll::Ready(Err(Refused(refusal))),
            );
            assert_eq!(*effects.calls.borrow(), expected[..=refusal]);
            assert!(effects.reads.borrow().is_empty());
        }
    }
    Ok(())
}

#[test]
fn shortcuts_skip_owner_and_mro_reads_in_the_original_order() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Receiver")?;
    let owner = instance(&db, "Owner")?;
    let variable = self_variable(&db, &env, owner);
    let ordinary = ordinary_variable(&db, &env);
    let context = variable.binding_context(&db);
    for (receiver, binding_context, variable, expected) in [
        (receiver, Some(context), ordinary, false),
        (Type::unknown(), Some(context), variable, true),
        (Type::unknown(), None, variable, false),
    ] {
        let binding = original_prepare(&db, &env, receiver, binding_context);
        let effects = Recording::default();
        assert_eq!(
            immediately_ready(should_bind_with(&db, &env, &binding, variable, &effects))?,
            expected,
        );
        assert_eq!(
            *effects.calls.borrow(),
            [Call::Work(SelfBindingWork::MatchVariable)]
        );
        assert!(effects.reads.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn matching_reads_the_mro_before_owner_and_stops_at_the_first_member() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Receiver")?;
    let receiver_class = class(&db, "Receiver")?;
    let mro_len = class_mro_literals(&db, receiver_class).len();
    assert_eq!(mro_len, 3);
    let generic_first = symbol(&db, "generic_first")?;
    let generic_second = symbol(&db, "generic_second")?;
    let first_class = generic_first.nominal_class(&db, &env);
    let second_class = generic_second.nominal_class(&db, &env);
    assert!(matches!(first_class, Some(ClassType::Generic(_))));
    assert!(matches!(second_class, Some(ClassType::Generic(_))));
    assert_ne!(first_class, second_class);
    assert_eq!(
        first_class.map(|class| class.class_literal(&db)),
        second_class.map(|class| class.class_literal(&db)),
    );
    let dynamic = instance(&db, "DynamicReceiver")?;
    assert!(matches!(
        class(&db, "DynamicReceiver")?,
        ClassLiteral::Dynamic(_)
    ));
    for (receiver, bound, expected, comparisons) in [
        (receiver, receiver, true, 1),
        (receiver, instance(&db, "Owner")?, true, 2),
        (receiver, instance(&db, "Unrelated")?, false, mro_len),
        (receiver, Type::unknown(), true, 0),
        (generic_first, generic_second, true, 1),
        (dynamic, instance(&db, "Owner")?, true, 2),
    ] {
        let binding = original_prepare(&db, &env, receiver, None);
        let receiver_class = binding
            .class_literal
            .ok_or_else(|| anyhow::anyhow!("receiver has no class origin"))?;
        let variable = self_variable(&db, &env, bound);
        let effects = Recording::default();
        assert_eq!(
            original_should_bind(&db, &env, &binding, variable),
            expected
        );
        assert_eq!(
            immediately_ready(should_bind_with(&db, &env, &binding, variable, &effects))?,
            expected,
        );
        let mut expected_calls = vec![
            Call::Work(SelfBindingWork::MatchVariable),
            Call::Mro(receiver_class),
            Call::SelfOwner(variable),
        ];
        expected_calls.extend(std::iter::repeat_n(
            Call::Work(SelfBindingWork::MroMember),
            comparisons,
        ));
        assert_eq!(*effects.calls.borrow(), expected_calls);
        assert_eq!(
            *effects.reads.borrow(),
            [Call::Mro(receiver_class), Call::SelfOwner(variable)]
        );
    }
    Ok(())
}

#[test]
fn every_matching_boundary_refuses_before_later_reads_or_a_boolean_result() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Receiver")?;
    let receiver_class = class(&db, "Receiver")?;
    let variable = self_variable(&db, &env, instance(&db, "Owner")?);
    let binding = original_prepare(&db, &env, receiver, None);
    let expected = [
        Call::Work(SelfBindingWork::MatchVariable),
        Call::Mro(receiver_class),
        Call::SelfOwner(variable),
        Call::Work(SelfBindingWork::MroMember),
        Call::Work(SelfBindingWork::MroMember),
    ];
    for refusal in 0..expected.len() {
        let effects = Recording::refusing(refusal);
        assert_eq!(
            try_poll_immediate(should_bind_with(&db, &env, &binding, variable, &effects)),
            Poll::Ready(Err(Refused(refusal))),
        );
        assert_eq!(*effects.calls.borrow(), expected[..=refusal]);
        let reads: Vec<_> = expected[..refusal]
            .iter()
            .copied()
            .filter(|call| !matches!(call, Call::Work(_)))
            .collect();
        assert_eq!(*effects.reads.borrow(), reads);
    }
    Ok(())
}

#[test]
fn invalid_mro_retains_ordinary_fallback_membership() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let invalid = instance(&db, "Invalid")?;
    let binding = original_prepare(&db, &env, invalid, None);
    for (upper_bound, expected) in [
        (invalid, true),
        (Type::object(), true),
        (instance(&db, "Owner")?, false),
    ] {
        let variable = self_variable(&db, &env, upper_bound);
        assert_eq!(
            original_should_bind(&db, &env, &binding, variable),
            expected
        );
        assert_eq!(
            immediately_ready(should_bind_with(
                &db,
                &env,
                &binding,
                variable,
                &InlineSelfBindingEffects,
            ))?,
            expected,
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Engine {
    Original,
    Shared,
}

#[derive(Clone, Copy)]
enum ColdOperation {
    PrepareSelf,
    MatchOwner,
    MatchingContext,
    MissingReceiver,
}

fn executed_queries(db: &TestDb, reader: &mut TestDb) -> Vec<String> {
    reader
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

fn cold_reads(engine: Engine, operation: ColdOperation) -> anyhow::Result<(bool, Vec<String>)> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = instance(&db, "Receiver")?;
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) = symbol(&db, "OwnerAlias")?
    else {
        anyhow::bail!("OwnerAlias did not retain its runtime alias handle");
    };
    let variable = self_variable(&db, &env, Type::TypeAlias(alias));
    let binding = match operation {
        ColdOperation::MissingReceiver => original_prepare(&db, &env, Type::unknown(), None),
        ColdOperation::MatchingContext => {
            original_prepare(&db, &env, receiver, Some(variable.binding_context(&db)))
        }
        ColdOperation::PrepareSelf | ColdOperation::MatchOwner => {
            original_prepare(&db, &env, receiver, None)
        }
    };
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let result = match (engine, operation) {
        (Engine::Original, ColdOperation::PrepareSelf) => {
            original_prepare(&db, &env, Type::TypeVar(variable), None)
                .class_literal
                .is_some()
        }
        (Engine::Shared, ColdOperation::PrepareSelf) => immediately_ready(prepare_with(
            &db,
            &env,
            Type::TypeVar(variable),
            None,
            &InlineSelfBindingEffects,
        ))?
        .class_literal
        .is_some(),
        (Engine::Original, _) => original_should_bind(&db, &env, &binding, variable),
        (Engine::Shared, _) => immediately_ready(should_bind_with(
            &db,
            &env,
            &binding,
            variable,
            &InlineSelfBindingEffects,
        ))?,
    };
    Ok((result, executed_queries(&db, &mut reader)))
}

#[test]
fn cold_source_reads_match_independently_of_decision_results() -> anyhow::Result<()> {
    // Input construction can populate class facts, but neither the alias body nor the
    // class-literal MRO query is demanded until the operation measured here.
    for operation in [ColdOperation::PrepareSelf, ColdOperation::MatchOwner] {
        let (expected_result, expected_reads) = cold_reads(Engine::Original, operation)?;
        let (actual_result, actual_reads) = cold_reads(Engine::Shared, operation)?;
        assert_eq!(actual_reads, expected_reads);
        assert!(actual_result);
        assert_eq!(actual_result, expected_result);
        let alias_read = actual_reads
            .iter()
            .position(|name| name == "raw_value_type")
            .ok_or_else(|| anyhow::anyhow!("missing cold alias-body read: {actual_reads:?}"))?;
        match operation {
            ColdOperation::PrepareSelf => {
                assert!(!actual_reads.iter().any(|name| name == "class_mro_literals"));
            }
            ColdOperation::MatchOwner => {
                let mro_read = actual_reads
                    .iter()
                    .position(|name| name == "class_mro_literals")
                    .ok_or_else(|| anyhow::anyhow!("missing cold MRO read: {actual_reads:?}"))?;
                assert!(mro_read < alias_read);
            }
            ColdOperation::MatchingContext | ColdOperation::MissingReceiver => {}
        }
    }
    for (operation, expected_result) in [
        (ColdOperation::MatchingContext, true),
        (ColdOperation::MissingReceiver, false),
    ] {
        let (original_result, original_reads) = cold_reads(Engine::Original, operation)?;
        let (actual_result, actual_reads) = cold_reads(Engine::Shared, operation)?;
        assert_eq!(actual_reads, original_reads);
        assert!(actual_reads.is_empty());
        assert_eq!(original_result, expected_result);
        assert_eq!(actual_result, expected_result);
    }
    Ok(())
}
