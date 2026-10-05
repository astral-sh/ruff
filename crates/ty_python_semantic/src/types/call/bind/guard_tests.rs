use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{Binding, BindingError, Bindings, CallErrorKind, CallableBinding, CheckTypesMode};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::call::arguments::{Argument, CallArguments};
use crate::types::call::bind::constructor::ConstructorCallableKind;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cyclic::{CallableEntry, CallableExpansion, CallableRecursionGuard};
use crate::types::tuple::TupleType;
use crate::types::{Type, TypeContext};

fn fixture() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/guard.py",
            r#"
from collections.abc import Awaitable, Callable
from functools import partial
from typing import Self

async def waiter[T](value: T, mapping: dict[T, int]) -> None: ...
values: dict[int, int]

def start[*Ts](callback: Callable[[*Ts], Awaitable[object]], *args: *Ts) -> None: ...

class Entry:
    def __init__[*Ts](self, callback: Callable[[*Ts], Awaitable[object]], *args: *Ts) -> None: ...

class Downstream:
    def __new__(cls, *args: object) -> Self: ...
    def __init__[*Ts](self, callback: Callable[[*Ts], Awaitable[object]], *args: *Ts) -> None: ...

class Callback:
    def __call__(self, value: int) -> str: ...
instance: Callback

def forward[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
def target(value: int) -> None: ...
"#,
        )
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/guard.py")?,
        env.program(db),
    );
    Ok(global_symbol(db, file, name).place.expect_type())
}

fn callable_at_depth<'a, 'db>(
    bindings: &'a Bindings<'db>,
    depth: usize,
) -> anyhow::Result<&'a CallableBinding<'db>> {
    let item = bindings
        .single_item()
        .ok_or_else(|| anyhow::anyhow!("expected one callable item"))?;
    if depth == 0 {
        return Ok(item.callable());
    }
    let downstream = item
        .as_constructor()
        .and_then(|constructor| constructor.downstream_constructor())
        .ok_or_else(|| anyhow::anyhow!("expected a downstream constructor"))?;
    callable_at_depth(downstream, depth - 1)
}

fn single_overload<'a, 'db>(
    callable: &'a CallableBinding<'db>,
) -> anyhow::Result<&'a Binding<'db>> {
    let [binding] = callable.overloads() else {
        anyhow::bail!("expected one overload");
    };
    Ok(binding)
}

fn callback_parameter<'db>(binding: &Binding<'db>) -> anyhow::Result<Type<'db>> {
    binding
        .signature
        .parameters()
        .iter()
        .find(|parameter| parameter.name().is_some_and(|name| name == "callback"))
        .map(|parameter| parameter.annotated_type())
        .ok_or_else(|| anyhow::anyhow!("missing callback parameter"))
}

fn partial_return_type<'db>(
    db: &'db TestDb,
    bindings: &Bindings<'db>,
) -> anyhow::Result<Type<'db>> {
    let Type::KnownInstance(crate::types::KnownInstanceType::FunctoolsPartial(partial)) =
        bindings.return_type(db, &db.program_environment())
    else {
        anyhow::bail!("expected a precise partial instance");
    };
    let [signature] = partial.partial(db).signatures(db).overloads.as_slice() else {
        anyhow::bail!("expected one partial signature");
    };
    Ok(signature.return_ty)
}

fn check<'db>(
    db: &'db TestDb,
    mut bindings: Bindings<'db>,
    arguments: &CallArguments<'_, 'db>,
    mode: CheckTypesMode,
    guard: &CallableRecursionGuard<'db>,
) -> (Bindings<'db>, Result<(), CallErrorKind>) {
    let result = bindings.check_types_impl_with_recursion_guard(
        db,
        &db.program_environment(),
        &ConstraintSetBuilder::new(),
        arguments,
        TypeContext::default(),
        &[],
        mode,
        Some(guard),
    );
    (bindings, result)
}

fn assert_typevartuple_guard(
    name: &str,
    depth: usize,
    mode: CheckTypesMode,
    splatted: bool,
) -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let waiter = symbol(&db, "waiter")?;
    let Type::FunctionLiteral(waiter_function) = waiter else {
        anyhow::bail!("expected waiter to be a function literal");
    };
    let values = [waiter, Type::int_literal(1), symbol(&db, "values")?];
    let arguments = if splatted {
        let mut arguments = CallArguments::none();
        arguments.push_argument(
            Argument::Variadic,
            Some(Type::tuple(TupleType::heterogeneous(&db, &env, values))),
        );
        arguments
    } else {
        CallArguments::positional(values)
    };
    let bindings = symbol(&db, name)?
        .bindings(&db, &env)
        .match_parameters(&db, &env, &arguments);
    if name == "Entry" {
        assert_eq!(
            bindings
                .single_item()
                .and_then(|item| item.as_constructor())
                .map(|constructor| constructor.context().kind()),
            Some(ConstructorCallableKind::Init),
        );
    } else if name == "Downstream" {
        assert_eq!(
            bindings
                .single_item()
                .and_then(|item| item.as_constructor())
                .map(|constructor| constructor.context().kind()),
            Some(ConstructorCallableKind::New),
        );
    }
    let declared = callback_parameter(single_overload(callable_at_depth(&bindings, depth)?)?)?;
    let guard = CallableRecursionGuard::new();
    let (baseline, result) = check(&db, bindings.clone(), &arguments, mode, &guard);
    assert!(result.is_ok(), "{result:?}: {baseline:?}");
    assert!(
        single_overload(callable_at_depth(&baseline, depth)?)?
            .errors()
            .is_empty()
    );

    // TypeVarTuple deferral accepts `waiter` for the `callback` parameter's declared
    // `Callable[[*Ts], Awaitable[object]]` type. With that conversion already active in this
    // guard, it recovers as a callable without `Ts`, so deferral no longer applies.
    {
        let CallableEntry::Entered(_scope) =
            guard.enter(&db, &env, (CallableExpansion::Upcast, declared))
        else {
            anyhow::bail!("the initial callback conversion should enter");
        };
        let (checked, result) = check(&db, bindings.clone(), &arguments, mode, &guard);
        let errors = single_overload(callable_at_depth(&checked, depth)?)?.errors();
        assert!(
            matches!(errors, [BindingError::InvalidArgumentType { provided_ty: Type::FunctionLiteral(provided_function), argument_index: Some(0), .. }] if provided_function.definition(&db) == waiter_function.definition(&db)),
            "expected the callback argument error: {errors:?}",
        );
        if mode == CheckTypesMode::Finalize {
            assert!(matches!(result, Err(CallErrorKind::BindingError)));
        } else {
            assert!(result.is_ok());
        }
    }

    let (retried, result) = check(&db, bindings, &arguments, mode, &guard);
    assert!(result.is_ok(), "{result:?}: {retried:?}");
    assert!(
        single_overload(callable_at_depth(&retried, depth)?)?
            .errors()
            .is_empty()
    );
    assert_eq!(
        retried.return_type(&db, &env),
        baseline.return_type(&db, &env)
    );
    Ok(())
}

#[test]
fn ordinary_typevartuple_deferral_retains_the_call_guard() -> anyhow::Result<()> {
    assert_typevartuple_guard("start", 0, CheckTypesMode::Finalize, false)
}

#[test]
fn ordinary_constructor_entry_retains_the_call_guard() -> anyhow::Result<()> {
    assert_typevartuple_guard("Entry", 0, CheckTypesMode::Finalize, false)
}

#[test]
fn ordinary_provisional_downstream_constructor_retains_the_call_guard() -> anyhow::Result<()> {
    assert_typevartuple_guard("Downstream", 1, CheckTypesMode::Provisional, false)
}

#[test]
fn ordinary_final_downstream_constructor_retains_the_call_guard() -> anyhow::Result<()> {
    assert_typevartuple_guard("Downstream", 1, CheckTypesMode::Finalize, false)
}

#[test]
fn ordinary_positional_splat_retains_the_call_guard() -> anyhow::Result<()> {
    assert_typevartuple_guard("start", 0, CheckTypesMode::Finalize, true)
}

#[test]
fn ordinary_partial_binding_preparation_retains_the_call_guard() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let instance = symbol(&db, "instance")?;
    let arguments = CallArguments::positional([instance]);
    let bindings = symbol(&db, "partial")?
        .bindings(&db, &env)
        .match_parameters(&db, &env, &arguments);
    let guard = CallableRecursionGuard::new();
    let (baseline, result) = check(
        &db,
        bindings.clone(),
        &arguments,
        CheckTypesMode::Finalize,
        &guard,
    );
    assert!(result.is_ok(), "{result:?}");
    assert!(!partial_return_type(&db, &baseline)?.is_unknown());
    // An active binding expansion recovers with an unknown signature. The partial's return
    // type loses precision only if its nested binding preparation sees this same guard;
    // dropping the entry must restore the ordinary return type.
    {
        let CallableEntry::Entered(_scope) =
            guard.enter(&db, &env, (CallableExpansion::Bindings, instance))
        else {
            anyhow::bail!("the initial wrapped-call binding should enter");
        };
        let (checked, result) = check(
            &db,
            bindings.clone(),
            &arguments,
            CheckTypesMode::Finalize,
            &guard,
        );
        assert!(result.is_ok(), "{result:?}");
        assert!(partial_return_type(&db, &checked)?.is_unknown());
    }
    let (retried, result) = check(&db, bindings, &arguments, CheckTypesMode::Finalize, &guard);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        partial_return_type(&db, &retried)?,
        partial_return_type(&db, &baseline)?
    );
    Ok(())
}

#[test]
fn ordinary_paramspec_provenance_retains_the_call_guard() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let arguments =
        CallArguments::positional([symbol(&db, "target")?, Type::string_literal(&db, "bad")]);
    let bindings = symbol(&db, "forward")?
        .bindings(&db, &env)
        .match_parameters(&db, &env, &arguments);
    let declared = callback_parameter(single_overload(callable_at_depth(&bindings, 0)?)?)?;
    let guard = CallableRecursionGuard::new();
    let has_parameter_source = |bindings: &Bindings<'_>| -> anyhow::Result<bool> {
        let errors = single_overload(callable_at_depth(bindings, 0)?)?.errors();
        let [
            BindingError::InvalidArgumentType {
                parameter_source,
                argument_index: Some(1),
                ..
            },
        ] = errors
        else {
            anyhow::bail!("expected the forwarded argument error: {errors:?}");
        };
        Ok(parameter_source.is_some())
    };
    let (baseline, result) = check(
        &db,
        bindings.clone(),
        &arguments,
        CheckTypesMode::Finalize,
        &guard,
    );
    assert!(matches!(result, Err(CallErrorKind::BindingError)));
    assert!(has_parameter_source(&baseline)?);
    // An active conversion of `callback`'s declared `Callable[P, None]` recovers without `P`.
    // Seeing that entry in this guard must remove the forwarded error's parameter source;
    // dropping the entry must restore that diagnostic provenance.
    {
        let CallableEntry::Entered(_scope) =
            guard.enter(&db, &env, (CallableExpansion::Upcast, declared))
        else {
            anyhow::bail!("the initial ParamSpec callable conversion should enter");
        };
        let (checked, result) = check(
            &db,
            bindings.clone(),
            &arguments,
            CheckTypesMode::Finalize,
            &guard,
        );
        assert!(matches!(result, Err(CallErrorKind::BindingError)));
        assert!(!has_parameter_source(&checked)?);
    }
    let (retried, result) = check(&db, bindings, &arguments, CheckTypesMode::Finalize, &guard);
    assert!(matches!(result, Err(CallErrorKind::BindingError)));
    assert!(has_parameter_source(&retried)?);
    Ok(())
}
