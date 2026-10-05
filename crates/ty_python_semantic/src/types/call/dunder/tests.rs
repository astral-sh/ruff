use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::UnionType;
use crate::types::signatures::effects::try_poll_immediate;

fn fixture() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/dunder.py",
            r#"flag: bool

class Good:
    def __set__(self, target: object, value: int) -> None: ...

class Other:
    def __set__(self, target: object, value: int) -> None: ...

class Missing: ...

class Maybe:
    if flag:
        def __set__(self, target: object, value: int) -> None: ...

def setter(target: object, value: int) -> None: ...
class InstanceOnly:
    def __init__(self):
        self.__set__ = setter

good: Good
other: Other
missing: Missing
maybe: Maybe
instance_only: InstanceOnly
"#,
        )
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/dunder.py")?,
        env.program(db),
    );
    Ok(global_symbol(db, file, name).place.expect_type())
}

fn request(receiver: Type<'_>) -> DunderCallRequest<'static, '_> {
    DunderCallRequest::implicit(
        receiver,
        "__set__",
        TypeContext::default(),
        MemberLookupPolicy::default(),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Stopped;

#[derive(Default)]
struct Controlled {
    events: RefCell<Vec<&'static str>>,
    stop_at: Option<usize>,
    suspend_after_check: bool,
    released: Cell<bool>,
}

impl Controlled {
    fn record(&self, event: &'static str) -> Result<(), Stopped> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.stop_at == Some(index) {
            Err(Stopped)
        } else {
            Ok(())
        }
    }
}

impl sealed::Sealed for Controlled {}

macro_rules! finite_effects {
    ($(fn $method:ident($($argument:ident: $ty:ty),*) -> $output:ty;)*) => {
        $(async fn $method(&self, $($argument: $ty,)*) -> Result<$output, Stopped> {
            self.record(stringify!($method))?;
            let result = InlineDunderEffects.$method($($argument),*).await.map_err(|never| match never {})?;
            self.record(concat!(stringify!($method), " done"))?;
            Ok(result)
        })*
    };
}

impl<'db> DunderEffects<'db> for Controlled {
    type Error = Stopped;

    async fn admit(&self, _work: DunderWork) -> Result<(), Stopped> {
        self.record("admit")
    }

    async fn read<T>(
        &self,
        _read: DunderRead,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Stopped> {
        self.record("read")?;
        let result = operation();
        self.record("read done")?;
        Ok(result)
    }

    // Each fixture dependency deliberately completes through the ordinary implementation.
    // Refusal before and after it verifies consumption order, not supervision within the child.
    finite_effects! {
        fn finite_alternatives(db: &'db dyn Db, env: &ProgramEnvironment<'db>, intersection: IntersectionType<'db>) -> Option<Type<'db>>;
        fn lookup(db: &'db dyn Db, env: &ProgramEnvironment<'db>, request: DunderCallRequest<'_, 'db>) -> PlaceAndQualifiers<'db>;
        fn bindings(db: &'db dyn Db, env: &ProgramEnvironment<'db>, callable: Type<'db>) -> Bindings<'db>;
        fn match_parameters(db: &'db dyn Db, env: &ProgramEnvironment<'db>, bindings: Bindings<'db>, arguments: &CallArguments<'_, 'db>) -> Bindings<'db>;
        fn call(db: &'db dyn Db, env: &ProgramEnvironment<'db>, request: DunderCallRequest<'_, 'db>, arguments: &CallArguments<'_, 'db>) -> DunderCallResult<'db>;
        fn union_add(builder: UnionBuilder<'db>, ty: Type<'db>) -> UnionBuilder<'db>;
        fn union_build(builder: UnionBuilder<'db>) -> Type<'db>;
        fn merge_intersection(receiver: Type<'db>, bindings: Vec<Bindings<'db>>) -> Bindings<'db>;
    }

    async fn check_types(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: Bindings<'db>,
        arguments: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Stopped> {
        self.record("check_types")?;
        let result = InlineDunderEffects
            .check_types(db, env, bindings, arguments, tcx)
            .await
            .map_err(|never| match never {})?;
        if self.suspend_after_check {
            poll_fn(|_| {
                if self.released.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
        self.record("check_types done")?;
        Ok(result)
    }
}

#[test]
fn dunder_invocation_preserves_call_errors_and_lookup_modes() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let good = symbol(&db, "good")?;
    let missing = symbol(&db, "missing")?;
    let maybe = symbol(&db, "maybe")?;
    let instance_only = symbol(&db, "instance_only")?;
    let arguments = CallArguments::positional([Type::object(), Type::int_literal(1)]);
    assert!(request(good).evaluate(&db, &env, &arguments).is_ok());
    assert!(matches!(
        request(missing).evaluate(&db, &env, &arguments),
        Err(CallDunderError::MethodNotAvailable)
    ));
    assert!(matches!(
        request(maybe).evaluate(&db, &env, &arguments),
        Err(CallDunderError::PossiblyUnbound {
            unbound_on: None,
            ..
        })
    ));
    assert!(matches!(
        request(instance_only).evaluate(&db, &env, &arguments),
        Err(CallDunderError::MethodNotAvailable)
    ));
    assert!(
        DunderCallRequest::on_class(instance_only, "__set__", TypeContext::default())
            .evaluate(&db, &env, &arguments)
            .is_ok()
    );

    let union = UnionType::from_elements(&db, &env, [good, missing]);
    let Err(CallDunderError::PossiblyUnbound {
        unbound_on: Some(unbound),
        ..
    }) = request(union).evaluate(&db, &env, &arguments)
    else {
        anyhow::bail!("missing union element must be retained");
    };
    assert_eq!(&*unbound, &[missing]);

    // A completed invalid call takes precedence over possible absence, and preserves the
    // declaration provenance obtained before binding began.
    let arguments = CallArguments::positional([Type::object(), Type::string_literal(&db, "bad")]);
    let Place::Defined(defined) = good
        .member_lookup_with_policy(
            &db,
            &env,
            "__set__",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        )
        .place
    else {
        anyhow::bail!("missing fixture setter");
    };
    for receiver in [good, maybe, union] {
        let Err(CallDunderError::CallError(_, _, provenance)) =
            request(receiver).evaluate(&db, &env, &arguments)
        else {
            anyhow::bail!("invalid setter argument must retain its call error");
        };
        if receiver == good {
            assert_eq!(provenance, defined.provenance);
        }
    }
    Ok(())
}

#[test]
fn dunder_invocation_orders_union_and_intersection_children() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let good = symbol(&db, "good")?;
    let other = symbol(&db, "other")?;
    let missing = symbol(&db, "missing")?;
    let union = UnionType::from_elements(&db, &env, [good, missing]);
    let intersection = IntersectionType::from_elements(&db, &env, [good, other]);
    assert!(matches!(intersection, Type::Intersection(_)));
    let arguments = CallArguments::positional([Type::object(), Type::int_literal(1)]);

    let effects = Controlled::default();
    assert!(matches!(
        try_poll_immediate(request(union).evaluate_with(&db, &env, &arguments, &effects)),
        Poll::Ready(Ok(Err(CallDunderError::PossiblyUnbound { .. })))
    ));
    let events = effects.events.borrow();
    let lookups: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| **event == "lookup")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(lookups.len(), 2);
    let binding = events
        .iter()
        .position(|event| *event == "bindings")
        .ok_or_else(|| anyhow::anyhow!("missing union binding"))?;
    assert!(lookups.iter().all(|index| *index < binding));
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == "check_types")
            .count(),
        1
    );
    drop(events);

    let effects = Controlled::default();
    assert!(matches!(
        try_poll_immediate(request(intersection).evaluate_with(&db, &env, &arguments, &effects)),
        Poll::Ready(Ok(Ok(_)))
    ));
    assert_eq!(
        &*effects.events.borrow(),
        &[
            "finite_alternatives",
            "finite_alternatives done",
            "read",
            "read done",
            "admit",
            "call",
            "call done",
            "call",
            "call done",
            "merge_intersection",
            "merge_intersection done",
        ]
    );
    Ok(())
}

#[test]
fn dunder_invocation_stops_before_consuming_incomplete_children() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let good = symbol(&db, "good")?;
    let other = symbol(&db, "other")?;
    let missing = symbol(&db, "missing")?;
    let maybe = symbol(&db, "maybe")?;
    let union = UnionType::from_elements(&db, &env, [good, missing]);
    let intersection = IntersectionType::from_elements(&db, &env, [good, other]);
    for receiver in [good, maybe, union, intersection] {
        for value in [Type::int_literal(1), Type::string_literal(&db, "bad")] {
            let arguments = CallArguments::positional([Type::object(), value]);
            let complete = Controlled::default();
            assert!(matches!(
                try_poll_immediate(
                    request(receiver).evaluate_with(&db, &env, &arguments, &complete)
                ),
                Poll::Ready(Ok(_))
            ));
            let expected = complete.events.into_inner();
            for stop_at in 0..expected.len() {
                let effects = Controlled {
                    stop_at: Some(stop_at),
                    ..Controlled::default()
                };
                assert!(matches!(
                    try_poll_immediate(
                        request(receiver).evaluate_with(&db, &env, &arguments, &effects)
                    ),
                    Poll::Ready(Err(Stopped))
                ));
                assert_eq!(&*effects.events.borrow(), &expected[..=stop_at]);
                let retry = Controlled::default();
                assert!(matches!(
                    try_poll_immediate(
                        request(receiver).evaluate_with(&db, &env, &arguments, &retry)
                    ),
                    Poll::Ready(Ok(_))
                ));
                assert_eq!(*retry.events.borrow(), expected);
            }
        }
    }
    Ok(())
}

#[test]
fn dunder_invocation_suspends_before_wrapping_binder_results() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let maybe = symbol(&db, "maybe")?;
    for value in [Type::int_literal(1), Type::string_literal(&db, "bad")] {
        let arguments = CallArguments::positional([Type::object(), value]);
        let effects = Controlled {
            suspend_after_check: true,
            ..Controlled::default()
        };
        let mut task = Box::pin(request(maybe).evaluate_with(&db, &env, &arguments, &effects));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(task.as_mut().poll(&mut cx).is_pending());
        assert_eq!(effects.events.borrow().last(), Some(&"check_types"));
        let events = effects.events.borrow().clone();
        assert!(task.as_mut().poll(&mut cx).is_pending());
        assert_eq!(*effects.events.borrow(), events);
        effects.released.set(true);
        if value == Type::int_literal(1) {
            assert!(matches!(
                task.as_mut().poll(&mut cx),
                Poll::Ready(Ok(Err(CallDunderError::PossiblyUnbound { .. })))
            ));
        } else {
            assert!(matches!(
                task.as_mut().poll(&mut cx),
                Poll::Ready(Ok(Err(CallDunderError::CallError(..))))
            ));
        }
        drop(task);

        let effects = Controlled {
            suspend_after_check: true,
            ..Controlled::default()
        };
        let mut cancelled = Box::pin(request(maybe).evaluate_with(&db, &env, &arguments, &effects));
        assert!(cancelled.as_mut().poll(&mut cx).is_pending());
        drop(cancelled);
        assert_eq!(effects.events.borrow().last(), Some(&"check_types"));
    }
    Ok(())
}
