use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ty_python_core::ProgramFile;

use super::{
    CallableEntryDecision, CallableEntryFacts, CallableGuardEntryEffects,
    ExactCallableEntryDecision, OrdinaryCallableGuardEntryEffects,
    SynchronousCallableGuardEntryEffects, callable_enter_exact_in_place_with,
    callable_enter_in_place_with,
};
use crate::db::tests::setup_db;
use crate::place::global_symbol;
use crate::types::cyclic::{
    CallableExpansion, CallableRecursionGuard, CallableVisitScope, DefinitionUse, TypeIdentity,
};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{DescriptorDispatches, DescriptorOrigin, Type};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Probe,
    Dependency,
    ExactLookup,
    Definition,
    PreviousDefinition,
    RepeatedDefinition,
    Dispatch,
    Anchor,
    DefinitionInsertion,
    Identity,
    IdentityInsertion,
    Growth,
    ExactInsertion,
}

struct InterruptingEntryEffects<'a, 'db> {
    ordinary: OrdinaryCallableGuardEntryEffects<'a, 'db>,
    stop: Option<Event>,
    events: RefCell<Vec<Event>>,
}

impl InterruptingEntryEffects<'_, '_> {
    fn observe(&self, event: Event) -> Result<(), Event> {
        self.events.borrow_mut().push(event);
        if self.stop == Some(event) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

impl<'db> CallableGuardEntryEffects<'db> for InterruptingEntryEffects<'_, 'db> {
    type Error = Event;

    async fn constructor_probe(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<Option<CallableEntryDecision>, Event> {
        self.observe(Event::Probe)?;
        Ok(self.ordinary.constructor_probe(scope, key)?)
    }

    async fn record_dependency(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<(), Event> {
        self.observe(Event::Dependency)?;
        Ok(self.ordinary.record_dependency(scope, key)?)
    }

    async fn contains_exact(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<bool, Event> {
        self.observe(Event::ExactLookup)?;
        Ok(self.ordinary.contains_exact(scope, key)?)
    }

    async fn callable_definition(
        &self,
        ty: Type<'db>,
        mode: CallableExpansion,
    ) -> Result<Option<DefinitionUse<'db>>, Event> {
        self.observe(Event::Definition)?;
        Ok(self.ordinary.callable_definition(ty, mode)?)
    }

    async fn has_previous_definition(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
    ) -> Result<bool, Event> {
        self.observe(Event::PreviousDefinition)?;
        Ok(self.ordinary.has_previous_definition(scope, reference)?)
    }

    async fn repeated_definition_growth(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
    ) -> Result<bool, Event> {
        self.observe(Event::RepeatedDefinition)?;
        Ok(self.ordinary.repeated_definition_growth(scope, reference)?)
    }

    async fn dispatch(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
    ) -> Result<DescriptorOrigin<'db>, Event> {
        self.observe(Event::Dispatch)?;
        Ok(self.ordinary.dispatch(scope)?)
    }

    async fn retain_anchor(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
        dispatches: DescriptorDispatches<'db>,
    ) -> Result<(), Event> {
        self.observe(Event::Anchor)?;
        Ok(self.ordinary.retain_anchor(scope, reference, dispatches)?)
    }

    async fn insert_definition(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Result<(), Event> {
        self.observe(Event::DefinitionInsertion)?;
        Ok(self.ordinary.insert_definition(scope, reference, origin)?)
    }

    async fn recursive_identity(&self, ty: Type<'db>) -> Result<Option<TypeIdentity<'db>>, Event> {
        self.observe(Event::Identity)?;
        Ok(self.ordinary.recursive_identity(ty)?)
    }

    async fn insert_identity(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, TypeIdentity<'db>),
    ) -> Result<bool, Event> {
        self.observe(Event::IdentityInsertion)?;
        Ok(self.ordinary.insert_identity(scope, key)?)
    }

    async fn record_growth(&self, scope: &CallableVisitScope<'_, 'db>) -> Result<(), Event> {
        self.observe(Event::Growth)?;
        Ok(self.ordinary.record_growth(scope)?)
    }

    async fn insert_exact(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> Result<bool, Event> {
        self.observe(Event::ExactInsertion)?;
        Ok(self.ordinary.insert_exact(scope, key)?)
    }
}

impl From<std::convert::Infallible> for Event {
    fn from(never: std::convert::Infallible) -> Self {
        match never {}
    }
}

/// Exact entry distinguishes binding from upcast keys and releases a finished sibling's key.
/// It records repeated keys as dependencies without borrowing the ancestor's removal ownership.
#[test]
fn exact_entry_preserves_modes_and_sibling_ownership() {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let binding = (CallableExpansion::Bindings, Type::Never);
    let upcast = (CallableExpansion::Upcast, Type::Never);
    let effects = InterruptingEntryEffects {
        ordinary: OrdinaryCallableGuardEntryEffects { db: &db, env: &env },
        stop: Some(Event::Definition),
        events: RefCell::default(),
    };
    let mut ancestor = guard.begin_scope();
    assert_eq!(
        try_poll_immediate(callable_enter_exact_in_place_with(
            binding,
            &mut ancestor,
            &effects,
        )),
        Poll::Ready(Ok(ExactCallableEntryDecision::Entered)),
    );
    for _ in 0..2 {
        let mut sibling = guard.begin_scope();
        assert_eq!(
            try_poll_immediate(callable_enter_exact_in_place_with(
                upcast,
                &mut sibling,
                &effects,
            )),
            Poll::Ready(Ok(ExactCallableEntryDecision::Entered)),
        );
        let mut repeated = guard.begin_scope();
        assert_eq!(
            try_poll_immediate(callable_enter_exact_in_place_with(
                binding,
                &mut repeated,
                &effects,
            )),
            Poll::Ready(Ok(ExactCallableEntryDecision::ExactCycle)),
        );
        drop(repeated);
        assert!(guard.active.seen.borrow().contains(&binding));
        assert!(guard.active.seen.borrow().contains(&upcast));
        drop(sibling);
        assert!(!guard.active.seen.borrow().contains(&upcast));
    }
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Dependency,
            Event::ExactLookup,
            Event::ExactInsertion,
            Event::Dependency,
            Event::ExactLookup,
            Event::ExactInsertion,
            Event::Dependency,
            Event::ExactLookup,
            Event::Dependency,
            Event::ExactLookup,
            Event::ExactInsertion,
            Event::Dependency,
            Event::ExactLookup,
        ],
    );
    assert!(guard.cache.dependencies.borrow().contains(&binding));
    assert!(guard.cache.dependencies.borrow().contains(&upcast));
    assert!(guard.growth.active.seen.borrow().is_empty());
    assert!(guard.identities.seen.borrow().is_empty());
    drop(ancestor);
    assert!(guard.active.seen.borrow().is_empty());
}

/// An exact cycle records its dependency before returning and never asks for definition metadata.
#[test]
fn exact_cycle_records_dependency_without_definition() {
    let db = setup_db();
    let env = db.program_environment();
    let guard = CallableRecursionGuard::new();
    let key = (CallableExpansion::Bindings, Type::Never);
    let mut ancestor = guard.begin_scope();
    assert!(ancestor.insert_exact_ordinary(key));
    let mut scope = guard.begin_scope();
    let effects = InterruptingEntryEffects {
        ordinary: OrdinaryCallableGuardEntryEffects { db: &db, env: &env },
        stop: Some(Event::Definition),
        events: RefCell::default(),
    };
    assert_eq!(
        try_poll_immediate(callable_enter_in_place_with(
            key,
            &mut scope,
            CallableEntryFacts,
            &effects
        )),
        Poll::Ready(Ok(CallableEntryDecision::ExactCycle)),
    );
    assert_eq!(
        *effects.events.borrow(),
        [Event::Probe, Event::Dependency, Event::ExactLookup]
    );
    assert!(guard.cache.dependencies.borrow().contains(&key));
    drop(scope);
    assert!(guard.active.seen.borrow().contains(&key));
    drop(ancestor);
    assert!(guard.active.seen.borrow().is_empty());
}

/// Interrupted first entry retains its inserted definition until the caller drops the scope.
/// A new entry after that cleanup succeeds without requesting a repeated-definition proof.
#[test]
fn partial_definition_survives_refusal_until_scope_drop() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented("/src/a.py", "class C: pass")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/a.py")?,
        env.program(&db),
    );
    let ty = global_symbol(&db, file, "C").place.expect_type();
    let key = (CallableExpansion::Bindings, ty);
    let guard = CallableRecursionGuard::new();
    let mut scope = guard.begin_scope();
    let mut effects = InterruptingEntryEffects {
        ordinary: OrdinaryCallableGuardEntryEffects { db: &db, env: &env },
        stop: Some(Event::ExactInsertion),
        events: RefCell::default(),
    };
    assert_eq!(
        try_poll_immediate(callable_enter_in_place_with(
            key,
            &mut scope,
            CallableEntryFacts,
            &effects
        )),
        Poll::Ready(Err(Event::ExactInsertion)),
    );
    assert!(!effects.events.borrow().contains(&Event::RepeatedDefinition));
    assert_eq!(guard.growth.active.seen.borrow().len(), 1);
    assert!(guard.active.seen.borrow().is_empty());
    drop(scope);
    assert!(guard.growth.active.seen.borrow().is_empty());

    effects.stop = Some(Event::RepeatedDefinition);
    effects.events.borrow_mut().clear();
    let mut retry = guard.begin_scope();
    assert_eq!(
        try_poll_immediate(callable_enter_in_place_with(
            key,
            &mut retry,
            CallableEntryFacts,
            &effects
        )),
        Poll::Ready(Ok(CallableEntryDecision::Entered)),
    );
    assert!(!effects.events.borrow().contains(&Event::RepeatedDefinition));
    assert_eq!(guard.growth.active.seen.borrow().len(), 1);
    assert!(guard.active.seen.borrow().contains(&key));
    drop(retry);
    assert!(guard.active.seen.borrow().is_empty());
    assert!(guard.growth.active.seen.borrow().is_empty());
    assert_eq!(guard.cache.growth_recoveries.get(), 0);
    Ok(())
}

/// A second specialization requests the growth proof. Interruption while obtaining that proof
/// preserves the ancestor's entries.
#[test]
fn repeated_definition_refusal_preserves_ancestor() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        "class C[T]: pass\nfirst: C[int]\nsecond: C[str]",
    )?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/a.py")?,
        env.program(&db),
    );
    let first = (
        CallableExpansion::Bindings,
        global_symbol(&db, file, "first").place.expect_type(),
    );
    let second = (
        CallableExpansion::Bindings,
        global_symbol(&db, file, "second").place.expect_type(),
    );
    let guard = CallableRecursionGuard::new();
    let mut ancestor = guard.begin_scope();
    let effects = InterruptingEntryEffects {
        ordinary: OrdinaryCallableGuardEntryEffects { db: &db, env: &env },
        stop: Some(Event::RepeatedDefinition),
        events: RefCell::default(),
    };
    assert_eq!(
        try_poll_immediate(callable_enter_in_place_with(
            first,
            &mut ancestor,
            CallableEntryFacts,
            &effects
        )),
        Poll::Ready(Ok(CallableEntryDecision::Entered)),
    );
    effects.events.borrow_mut().clear();
    let mut scope = guard.begin_scope();
    assert_eq!(
        try_poll_immediate(callable_enter_in_place_with(
            second,
            &mut scope,
            CallableEntryFacts,
            &effects
        )),
        Poll::Ready(Err(Event::RepeatedDefinition)),
    );
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Probe,
            Event::Dependency,
            Event::ExactLookup,
            Event::Definition,
            Event::PreviousDefinition,
            Event::RepeatedDefinition
        ],
    );
    drop(scope);
    assert!(guard.active.seen.borrow().contains(&first));
    assert!(!guard.active.seen.borrow().contains(&second));
    assert_eq!(guard.growth.active.seen.borrow().len(), 1);
    assert_eq!(guard.cache.growth_recoveries.get(), 0);
    drop(ancestor);
    assert!(guard.active.seen.borrow().is_empty());
    assert!(guard.growth.active.seen.borrow().is_empty());
    Ok(())
}
