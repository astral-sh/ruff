use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::future::{Future, poll_fn};
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{BindingsEffects, InlineBindingsEffects, InstanceBindingsWork, instance_bindings_with};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::{DefinedPlace, Definedness, Place, global_symbol};
use crate::types::call::{Bindings, CallableBinding};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{
    ClassType, DescriptorOrigin, MemberLookupError, MemberLookupErrorKind, MemberLookupPolicy,
    MemberLookupResult, Parameters, ResolvedMember, Signature, Type,
};
use crate::{Db, ProgramEnvironment};

fn ready<T, E: Debug>(future: impl Future<Output = Result<T, E>>) -> anyhow::Result<T> {
    match try_poll_immediate(future) {
        Poll::Ready(Ok(result)) => Ok(result),
        Poll::Ready(Err(error)) => anyhow::bail!("unexpected error: {error:?}"),
        Poll::Pending => anyhow::bail!("ordinary binding suspended"),
    }
}

fn member<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    definedness: Definedness,
    origin: DescriptorOrigin<'db>,
) -> ResolvedMember<'db> {
    let mut place = Place::bound(ty);
    if let Place::Defined(defined) = &mut place {
        defined.definedness = definedness;
    }
    ResolvedMember::with_metadata(db, place.into(), None, origin)
}

// Preserve the branch decisions independently of the shared async body. The child operation
// is supplied separately so raw member fixtures do not need source inference.
fn original_branch<'db>(
    db: &'db dyn Db,
    receiver: Type<'db>,
    raw: MemberLookupResult<'db>,
    child: impl FnOnce(Type<'db>, DescriptorOrigin<'db>) -> Bindings<'db>,
) -> Bindings<'db> {
    let member = raw.unwrap_or_else(|error| error.fallback_member(db));
    match member.member(db).place {
        Place::Defined(DefinedPlace {
            ty, definedness, ..
        }) => {
            let mut bindings = child(ty, member.descriptor_origin(db));
            bindings.replace_callable_type(ty, receiver);
            if definedness == Definedness::PossiblyUndefined {
                bindings.set_dunder_call_is_possibly_unbound();
            }
            bindings
        }
        Place::Undefined => CallableBinding::not_callable(receiver).into(),
    }
}

fn rich_child<'db>(callable: Type<'db>, sibling: Type<'db>) -> Bindings<'db> {
    let overloads = |ty, returns: &[i64]| {
        Bindings::from(CallableBinding::from_overloads(
            ty,
            returns
                .iter()
                .map(|value| Signature::new(Parameters::standard([]), Type::int_literal(*value))),
        ))
    };
    Bindings::from_union(
        callable,
        [
            overloads(callable, &[1, 2]),
            Bindings::from_intersection(
                callable,
                [overloads(callable, &[3]), overloads(sibling, &[4, 5])],
            ),
        ],
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Checkpoint(InstanceBindingsWork),
    Lookup(Type<'db>),
    Child(Type<'db>, DescriptorOrigin<'db>),
    ChildReady,
    ChildDropped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refusal {
    Checkpoint(InstanceBindingsWork),
    Interrupted,
}

struct RecordingEffects<'db> {
    raw: MemberLookupResult<'db>,
    child: Bindings<'db>,
    events: RefCell<Vec<Event<'db>>>,
    checkpoints: Cell<usize>,
    refuse_at: Option<usize>,
    interrupt_after_lookup: bool,
    interrupt_after_child: bool,
    interrupted: Cell<bool>,
    child_ready: Cell<bool>,
    active_children: Cell<usize>,
    child_entries: Cell<usize>,
    child_polls: Cell<usize>,
}

impl<'db> RecordingEffects<'db> {
    fn new(raw: MemberLookupResult<'db>, child: Bindings<'db>) -> Self {
        Self {
            raw,
            child,
            events: RefCell::default(),
            checkpoints: Cell::new(0),
            refuse_at: None,
            interrupt_after_lookup: false,
            interrupt_after_child: false,
            interrupted: Cell::new(false),
            child_ready: Cell::new(true),
            active_children: Cell::new(0),
            child_entries: Cell::new(0),
            child_polls: Cell::new(0),
        }
    }
}

struct ChildScope<'a, 'db>(&'a RecordingEffects<'db>);

impl Drop for ChildScope<'_, '_> {
    fn drop(&mut self) {
        self.0.active_children.set(self.0.active_children.get() - 1);
        self.0.events.borrow_mut().push(Event::ChildDropped);
    }
}

impl<'db> BindingsEffects<'db> for RecordingEffects<'db> {
    type Error = Refusal;

    fn checkpoint(&self, _db: &dyn Db, work: InstanceBindingsWork) -> Result<(), Refusal> {
        self.events.borrow_mut().push(Event::Checkpoint(work));
        let index = self.checkpoints.get();
        self.checkpoints.set(index + 1);
        if self.interrupted.get() {
            Err(Refusal::Interrupted)
        } else if self.refuse_at == Some(index) {
            Err(Refusal::Checkpoint(work))
        } else {
            Ok(())
        }
    }

    fn call_member(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        _guard: &CallableRecursionGuard<'db>,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Refusal>> {
        self.events.borrow_mut().push(Event::Lookup(ty));
        self.interrupted.set(self.interrupt_after_lookup);
        std::future::ready(Ok(self.raw))
    }

    async fn bindings_from_descriptor(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Result<Bindings<'db>, Refusal> {
        self.events.borrow_mut().push(Event::Child(ty, origin));
        self.child_entries.set(self.child_entries.get() + 1);
        self.active_children.set(self.active_children.get() + 1);
        assert_eq!(self.active_children.get(), 1);
        let _scope = ChildScope(self);
        poll_fn(|_| {
            self.child_polls.set(self.child_polls.get() + 1);
            if self.child_ready.get() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
        let mut result = self.child.clone();
        result.add_descriptor_origin(db, origin);
        self.events.borrow_mut().push(Event::ChildReady);
        self.interrupted.set(self.interrupt_after_child);
        Ok(result)
    }
}

#[test]
fn raw_members_preserve_absence_fallback_and_descriptor_metadata() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let receiver = Type::object();
    let callable = Type::unknown();
    let sibling = Type::any();
    let origin = DescriptorOrigin {
        incomplete: true,
        return_contains_recursive_recovery: true,
        ..DescriptorOrigin::default()
    };
    let absent: MemberLookupResult<'_> = Place::Undefined.into();
    let defined = member(&db, callable, Definedness::AlwaysDefined, origin);
    let possible = member(&db, callable, Definedness::PossiblyUndefined, origin);
    let semantic_error = MemberLookupError::new(
        &db,
        possible,
        MemberLookupErrorKind::GetAttr {
            receiver,
            name: Type::string_literal(&db, "__call__"),
        },
    );
    for raw in [absent, Ok(defined), Ok(possible), Err(semantic_error)] {
        let child = rich_child(callable, sibling);
        let effects = RecordingEffects::new(raw, child.clone());
        let actual = ready(instance_bindings_with(
            &db, &env, receiver, &guard, &effects,
        ))?;
        let expected = original_branch(&db, receiver, raw, |ty, passed_origin| {
            assert_eq!(ty, callable);
            assert_eq!(passed_origin, origin);
            let mut result = child.clone();
            result.add_descriptor_origin(&db, passed_origin);
            result
        });
        assert_eq!(actual.callable_type(), receiver);
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        if raw == absent {
            assert_eq!(effects.child_entries.get(), 0);
            assert_eq!(
                *effects.events.borrow(),
                [
                    Event::Checkpoint(InstanceBindingsWork::Lookup),
                    Event::Lookup(receiver),
                    Event::Checkpoint(InstanceBindingsWork::Resolve),
                    Event::Checkpoint(InstanceBindingsWork::Publish),
                ]
            );
        } else {
            assert_eq!(effects.child_entries.get(), 1);
            assert!(
                effects
                    .events
                    .borrow()
                    .contains(&Event::Child(callable, origin))
            );
            assert_eq!(
                actual
                    .iter_flat()
                    .map(|binding| binding.signature_type)
                    .collect::<Vec<_>>(),
                [callable, callable, sibling]
            );
        }
    }
    Ok(())
}

#[test]
fn every_checkpoint_refuses_without_later_work_or_mutating_a_sibling() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let receiver = Type::object();
    let callable = Type::unknown();
    let raw = Ok(member(
        &db,
        callable,
        Definedness::PossiblyUndefined,
        DescriptorOrigin::default(),
    ));
    let child = rich_child(callable, Type::any());
    let original = format!("{child:?}");
    let complete = RecordingEffects::new(raw, child.clone());
    let expected = ready(instance_bindings_with(
        &db, &env, receiver, &guard, &complete,
    ))?;
    let trace = complete.events.into_inner();
    assert_eq!(
        trace
            .iter()
            .filter(|event| **event == Event::Checkpoint(InstanceBindingsWork::Receiver))
            .count(),
        11
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| **event == Event::Checkpoint(InstanceBindingsWork::PossibleAbsence))
            .count(),
        3
    );
    for (checkpoint, (position, work)) in trace
        .iter()
        .enumerate()
        .filter_map(|(position, event)| match event {
            Event::Checkpoint(work) => Some((position, *work)),
            _ => None,
        })
        .enumerate()
    {
        let mut effects = RecordingEffects::new(raw, child.clone());
        effects.refuse_at = Some(checkpoint);
        assert!(matches!(
            try_poll_immediate(instance_bindings_with(&db, &env, receiver, &guard, &effects)),
            Poll::Ready(Err(Refusal::Checkpoint(refused))) if refused == work
        ));
        assert_eq!(*effects.events.borrow(), trace[..=position]);
        assert_eq!(effects.active_children.get(), 0);
        assert_eq!(format!("{:?}", effects.child), original);
    }
    let retry = RecordingEffects::new(raw, child);
    let actual = ready(instance_bindings_with(&db, &env, receiver, &guard, &retry))?;
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    Ok(())
}

#[test]
fn interrupted_lookup_and_completed_child_stop_before_continuation() {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let receiver = Type::object();
    let callable = Type::unknown();
    let fallback = member(
        &db,
        callable,
        Definedness::AlwaysDefined,
        DescriptorOrigin::default(),
    );
    let raw = Err(MemberLookupError::new(
        &db,
        fallback,
        MemberLookupErrorKind::GetAttr {
            receiver,
            name: Type::string_literal(&db, "__call__"),
        },
    ));
    for after_lookup in [true, false] {
        let mut effects = RecordingEffects::new(raw, rich_child(callable, Type::any()));
        effects.interrupt_after_lookup = after_lookup;
        effects.interrupt_after_child = !after_lookup;
        assert!(matches!(
            try_poll_immediate(instance_bindings_with(
                &db, &env, receiver, &guard, &effects
            )),
            Poll::Ready(Err(Refusal::Interrupted))
        ));
        assert_eq!(effects.child_entries.get(), usize::from(!after_lookup));
        let trace = effects.events.borrow();
        assert_eq!(
            trace.last(),
            Some(&Event::Checkpoint(if after_lookup {
                InstanceBindingsWork::Resolve
            } else {
                InstanceBindingsWork::Receiver
            }))
        );
        assert!(!trace.contains(&Event::Checkpoint(InstanceBindingsWork::Publish)));
        assert_eq!(effects.active_children.get(), 0);
    }
}

#[test]
fn a_pending_child_is_retained_once_and_dropped_without_parent_updates() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let receiver = Type::object();
    let callable = Type::unknown();
    let raw = Ok(member(
        &db,
        callable,
        Definedness::AlwaysDefined,
        DescriptorOrigin::default(),
    ));
    let effects = RecordingEffects::new(raw, rich_child(callable, Type::any()));
    effects.child_ready.set(false);
    let mut pending = Box::pin(instance_bindings_with(
        &db, &env, receiver, &guard, &effects,
    ));
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        assert!(pending.as_mut().poll(&mut context).is_pending());
        assert_eq!(effects.child_entries.get(), 1);
        assert_eq!(effects.active_children.get(), 1);
    }
    drop(pending);
    assert_eq!(effects.child_polls.get(), 3);
    assert_eq!(effects.active_children.get(), 0);
    assert_eq!(effects.events.borrow().last(), Some(&Event::ChildDropped));
    assert!(!effects.events.borrow().iter().any(|event| matches!(
        event,
        Event::Checkpoint(InstanceBindingsWork::Receiver | InstanceBindingsWork::Publish)
    )));

    effects.child_ready.set(true);
    let result = ready(instance_bindings_with(
        &db, &env, receiver, &guard, &effects,
    ))?;
    assert_eq!(result.callable_type(), receiver);
    assert_eq!(effects.child_entries.get(), 2);
    assert_eq!(effects.active_children.get(), 0);
    assert_eq!(
        effects.events.borrow().last(),
        Some(&Event::Checkpoint(InstanceBindingsWork::Publish))
    );
    Ok(())
}

#[test]
fn inline_descriptor_dependency_retains_recursive_recovery() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let effects = InlineBindingsEffects {
        recursion_guard: &guard,
    };
    for recovery in [false, true] {
        let origin = DescriptorOrigin {
            return_contains_recursive_recovery: recovery,
            ..DescriptorOrigin::default()
        };
        let actual = ready(effects.bindings_from_descriptor(&db, &env, Type::unknown(), origin))?;
        assert_eq!(
            actual
                .iter_flat()
                .flat_map(CallableBinding::overloads)
                .count(),
            1
        );
        assert!(
            actual
                .iter_flat()
                .flat_map(CallableBinding::overloads)
                .all(|binding| binding.signature.is_recursion_recovery() == recovery)
        );
        let expected = guard.with_dependency(&db, origin, || {
            Type::unknown().bindings_from_descriptor(&db, &env, &guard, origin)
        });
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    }
    Ok(())
}

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/instance_bindings.py",
            r#"
class Empty: ...
class Callable:
    def __call__(self) -> int: ...
class Illegal:
    __call__ = 1
"#,
        )
        .build()
}

fn receiver<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/instance_bindings.py")?,
        env.program(db),
    );
    let class = global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))?;
    Ok(Type::instance(db, &env, ClassType::NonGeneric(class)))
}

fn original_source<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    guard: &CallableRecursionGuard<'db>,
) -> Bindings<'db> {
    let raw = ty.member_lookup_with_recursion_guard(
        db,
        env,
        "__call__",
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        None,
        Some(guard),
    );
    original_branch(db, ty, raw, |callable, origin| {
        guard.with_dependency(db, origin, || {
            callable.bindings_from_descriptor(db, env, guard, origin)
        })
    })
}

#[test]
fn ordinary_source_branches_match_the_original_and_are_immediately_ready() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    for name in ["Empty", "Callable", "Illegal"] {
        let receiver = receiver(&db, name)?;
        let guard = CallableRecursionGuard::new();
        let actual = ready(instance_bindings_with(
            &db,
            &env,
            receiver,
            &guard,
            &InlineBindingsEffects {
                recursion_guard: &guard,
            },
        ))?;
        let expected = original_source(&db, &env, receiver, &CallableRecursionGuard::new());
        assert_eq!(actual.callable_type(), receiver);
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{name}");
        assert_eq!(
            actual
                .iter_flat()
                .flat_map(CallableBinding::overloads)
                .count(),
            usize::from(name == "Callable")
        );
    }
    Ok(())
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

fn cold_reads(shared: bool, name: &str) -> anyhow::Result<Vec<String>> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = receiver(&db, name)?;
    let guard = CallableRecursionGuard::new();
    executions(&db);
    if shared {
        let _ = ready(instance_bindings_with(
            &db,
            &env,
            receiver,
            &guard,
            &InlineBindingsEffects {
                recursion_guard: &guard,
            },
        ))?;
    } else {
        let _ = original_source(&db, &env, receiver, &guard);
    }
    Ok(executions(&db))
}

#[test]
fn ordinary_source_read_order_is_compared_on_independent_cold_databases() -> anyhow::Result<()> {
    for name in ["Empty", "Callable", "Illegal"] {
        let expected = cold_reads(false, name)?;
        let actual = cold_reads(true, name)?;
        assert!(
            !expected.is_empty(),
            "{name}: source fixture was already warm"
        );
        assert_eq!(actual, expected, "{name}");
    }
    Ok(())
}
