use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use rustc_hash::FxHashSet;
use salsa::plumbing::AsId;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::control::{GrowthPlan, TddError};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{
    BindingContext, TypeVarBoundOrConstraints, TypeVarDefaultEvaluation, TypeVarKind, TypeVarNonce,
};
use crate::types::visitor::SmallSetControl;

fn infallible<T>(result: Result<T, Infallible>) -> T {
    let Ok(value) = result;
    value
}

struct Trace<'env, 'visitor, 'db> {
    ordinary: OrdinarySelfReferenceEffects<'env, 'visitor, 'db>,
    events: RefCell<Vec<&'static str>>,
    checked: RefCell<Vec<TypeVarInstance<'db>>>,
    refuse: Option<usize>,
    specialization: Option<Specialization<'db>>,
    context: Option<GenericContext<'db>>,
    body: Type<'db>,
    recursive_arguments: Option<Specialization<'db>>,
    body_identity: TypeIdentity<'db>,
}

impl<'env, 'visitor, 'db> Trace<'env, 'visitor, 'db> {
    fn new(
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
        visitor: &'visitor TypeVarDefaultVisitor<'db>,
    ) -> Self {
        Self {
            ordinary: OrdinarySelfReferenceEffects { db, env, visitor },
            events: RefCell::default(),
            checked: RefCell::default(),
            refuse: None,
            specialization: None,
            context: None,
            body: Type::int_literal(17),
            recursive_arguments: None,
            body_identity: TypeIdentity::Other(Type::int_literal(19)),
        }
    }

    fn record(&self, event: &'static str) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse == Some(index) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

impl<'db> SynchronousSelfReferenceEffects<'db> for Trace<'_, '_, 'db> {
    type Error = &'static str;
    type State = SelfReferenceState<'db>;

    fn new_state(&self) -> Result<Self::State, Self::Error> {
        self.record("new_state")?;
        Ok(SelfReferenceState::new())
    }

    fn identity(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<TypeVarIdentity<'db>, Self::Error> {
        self.record("identity")?;
        Ok(infallible(self.ordinary.identity(variable)))
    }

    fn bound_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Self::Error> {
        self.record("bound_typevar")?;
        Ok(infallible(self.ordinary.bound_typevar(variable)))
    }

    fn remember_variable(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("remember_variable")?;
        Ok(infallible(self.ordinary.remember_variable(state, variable)))
    }

    fn checked_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record("checked_default")?;
        self.checked.borrow_mut().push(variable);
        Ok(infallible(self.ordinary.checked_default(variable)))
    }

    fn search(
        &self,
        state: &Self::State,
        ty: Type<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("search")?;
        let error = RefCell::new(None);
        let found = any_over_type(self.ordinary.db, self.ordinary.env, ty, false, |inner| {
            match self_reference_predicate_sync(inner, target, state, self) {
                Ok(found) => found,
                Err(refused) => {
                    *error.borrow_mut() = Some(refused);
                    true
                }
            }
        });
        match error.into_inner() {
            Some(error) => Err(error),
            None => Ok(found),
        }
    }

    fn variable_reference(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("variable_reference")?;
        variable_is_self_referential_sync(variable, target, state, SelfReferenceFacts, self)
    }

    fn alias_reference(
        &self,
        state: &Self::State,
        alias: TypeAliasType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("alias_reference")?;
        alias_is_self_referential_sync(alias, target, state, self)
    }

    fn recursive_reference(
        &self,
        state: &Self::State,
        recursive: RecursiveType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("recursive_reference")?;
        recursive_is_self_referential_sync(recursive, target, state, self)
    }

    fn alias_specialization(
        &self,
        _alias: TypeAliasType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        self.record("alias_specialization")?;
        Ok(self.specialization)
    }

    fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.record("specialization_types")?;
        Ok(infallible(
            self.ordinary.specialization_types(specialization),
        ))
    }

    fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record("next_type")?;
        Ok(infallible(self.ordinary.next_type(types, cursor)))
    }

    fn alias_generic_context(
        &self,
        _alias: TypeAliasType<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        self.record("alias_generic_context")?;
        Ok(self.context)
    }

    fn generic_variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error> {
        self.record("generic_variables")?;
        Ok(infallible(self.ordinary.generic_variables(context)))
    }

    fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        self.record("next_variable")?;
        Ok(infallible(self.ordinary.next_variable(variables, cursor)))
    }

    fn alias_identity(&self, _alias: TypeAliasType<'db>) -> Result<TypeIdentity<'db>, Self::Error> {
        self.record("alias_identity")?;
        Ok(self.body_identity)
    }

    fn recursive_identity(
        &self,
        _recursive: RecursiveType<'db>,
    ) -> Result<TypeIdentity<'db>, Self::Error> {
        self.record("recursive_identity")?;
        Ok(self.body_identity)
    }

    fn remember_type(
        &self,
        state: &Self::State,
        identity: TypeIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("remember_type")?;
        Ok(infallible(self.ordinary.remember_type(state, identity)))
    }

    fn alias_value(&self, _alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        self.record("alias_value")?;
        Ok(self.body)
    }

    fn alias_raw_value(&self, _alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        self.record("alias_raw_value")?;
        Ok(self.body)
    }

    fn recursive_arguments(
        &self,
        _recursive: RecursiveType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        self.record("recursive_arguments")?;
        Ok(self.recursive_arguments)
    }

    fn recursive_unfold(&self, _recursive: RecursiveType<'db>) -> Result<Type<'db>, Self::Error> {
        self.record("recursive_unfold")?;
        Ok(self.body)
    }
}

impl<'db> SelfReferenceEffects<'db> for Trace<'_, '_, 'db> {
    type Error = &'static str;
    type State = SelfReferenceState<'db>;
    async fn new_state(&self) -> Result<Self::State, Self::Error> {
        SynchronousSelfReferenceEffects::new_state(self)
    }

    async fn identity(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<TypeVarIdentity<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::identity(self, variable)
    }

    async fn bound_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::bound_typevar(self, variable)
    }

    async fn remember_variable(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousSelfReferenceEffects::remember_variable(self, state, variable)
    }

    async fn checked_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        SynchronousSelfReferenceEffects::checked_default(self, variable)
    }

    async fn search(
        &self,
        state: &Self::State,
        ty: Type<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousSelfReferenceEffects::search(self, state, ty, target)
    }

    async fn variable_reference(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousSelfReferenceEffects::variable_reference(self, state, variable, target)
    }

    async fn alias_reference(
        &self,
        state: &Self::State,
        alias: TypeAliasType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousSelfReferenceEffects::alias_reference(self, state, alias, target)
    }

    async fn recursive_reference(
        &self,
        state: &Self::State,
        recursive: RecursiveType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousSelfReferenceEffects::recursive_reference(self, state, recursive, target)
    }

    async fn alias_specialization(
        &self,
        alias: TypeAliasType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        SynchronousSelfReferenceEffects::alias_specialization(self, alias)
    }

    async fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        SynchronousSelfReferenceEffects::specialization_types(self, specialization)
    }

    async fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        SynchronousSelfReferenceEffects::next_type(self, types, cursor)
    }

    async fn alias_generic_context(
        &self,
        alias: TypeAliasType<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        SynchronousSelfReferenceEffects::alias_generic_context(self, alias)
    }

    async fn generic_variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::generic_variables(self, context)
    }

    async fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        SynchronousSelfReferenceEffects::next_variable(self, variables, cursor)
    }

    async fn alias_identity(
        &self,
        alias: TypeAliasType<'db>,
    ) -> Result<TypeIdentity<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::alias_identity(self, alias)
    }

    async fn recursive_identity(
        &self,
        recursive: RecursiveType<'db>,
    ) -> Result<TypeIdentity<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::recursive_identity(self, recursive)
    }

    async fn remember_type(
        &self,
        state: &Self::State,
        identity: TypeIdentity<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousSelfReferenceEffects::remember_type(self, state, identity)
    }

    async fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::alias_value(self, alias)
    }

    async fn alias_raw_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::alias_raw_value(self, alias)
    }

    async fn recursive_arguments(
        &self,
        recursive: RecursiveType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        SynchronousSelfReferenceEffects::recursive_arguments(self, recursive)
    }

    async fn recursive_unfold(
        &self,
        recursive: RecursiveType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        SynchronousSelfReferenceEffects::recursive_unfold(self, recursive)
    }
}

fn variable<'db>(
    db: &'db dyn Db,
    name: &'static str,
    default: Option<Type<'db>>,
) -> TypeVarInstance<'db> {
    TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static(name), None, TypeVarKind::Pep695TypeVar),
        None,
        None,
        default.map(TypeVarDefaultEvaluation::Eager),
    )
}

fn bound<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    variable: TypeVarInstance<'db>,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        variable,
        BindingContext::Synthetic(env.program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

fn alias(db: &TestDb) -> anyhow::Result<TypeAliasType<'_>> {
    let file = system_path_to_file(db, "/src/self_reference.py")?;
    let program_file = ProgramFile::new(db, file, db.program_environment().program(db));
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
        global_symbol(db, program_file, "Alias").place.expect_type()
    else {
        anyhow::bail!("expected the runtime alias");
    };
    Ok(alias)
}

#[derive(Clone, Copy)]
enum Entry<'db> {
    Root(TypeVarInstance<'db>, Type<'db>),
    Variable(TypeVarInstance<'db>),
    Alias(TypeAliasType<'db>),
    Recursive(RecursiveType<'db>),
    Predicate(Type<'db>),
}

fn run<'db>(
    entry: Entry<'db>,
    asynchronous: bool,
    target: TypeVarIdentity<'db>,
    state: &SelfReferenceState<'db>,
    effects: &Trace<'_, '_, 'db>,
) -> Poll<Result<bool, &'static str>> {
    if asynchronous {
        match entry {
            Entry::Root(variable, ty) => {
                try_poll_immediate(type_is_self_referential_with(variable, ty, effects))
            }
            Entry::Variable(variable) => try_poll_immediate(variable_is_self_referential_with(
                variable,
                target,
                state,
                SelfReferenceFacts,
                effects,
            )),
            Entry::Alias(alias) => try_poll_immediate(alias_is_self_referential_with(
                alias, target, state, effects,
            )),
            Entry::Recursive(recursive) => try_poll_immediate(recursive_is_self_referential_with(
                recursive, target, state, effects,
            )),
            Entry::Predicate(ty) => {
                try_poll_immediate(self_reference_predicate_with(ty, target, state, effects))
            }
        }
    } else {
        Poll::Ready(match entry {
            Entry::Root(variable, ty) => type_is_self_referential_sync(variable, ty, effects),
            Entry::Variable(variable) => variable_is_self_referential_sync(
                variable,
                target,
                state,
                SelfReferenceFacts,
                effects,
            ),
            Entry::Alias(alias) => alias_is_self_referential_sync(alias, target, state, effects),
            Entry::Recursive(recursive) => {
                recursive_is_self_referential_sync(recursive, target, state, effects)
            }
            Entry::Predicate(ty) => self_reference_predicate_sync(ty, target, state, effects),
        })
    }
}

#[test]
fn logical_target_identity_precedes_full_instance_membership() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let target = variable(&db, "T", None);
    let other_instance = variable(&db, "T", Some(Type::int_literal(1)));
    assert_ne!(target, other_instance);
    for entry in [
        Entry::Variable(other_instance),
        Entry::Predicate(Type::KnownInstance(KnownInstanceType::TypeVar(
            other_instance,
        ))),
        Entry::Predicate(Type::TypeVar(bound(&db, &env, other_instance))),
        Entry::Root(
            target,
            Type::KnownInstance(KnownInstanceType::TypeVar(other_instance)),
        ),
    ] {
        for asynchronous in [false, true] {
            let effects = Trace::new(&db, &env, &visitor);
            let state = SelfReferenceState::new();
            assert_eq!(
                run(entry, asynchronous, target.identity(&db), &state, &effects),
                Poll::Ready(Ok(true))
            );
            assert!(state.seen_typevars.borrow().is_empty());
            assert!(effects.checked.borrow().is_empty());
            assert!(!effects.events.borrow().contains(&"remember_variable"));
        }
    }
    Ok(())
}

#[test]
fn complete_instances_are_remembered_before_following_defaults() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let target = variable(&db, "T", None);
    let target_ty = Type::KnownInstance(KnownInstanceType::TypeVar(target));
    let plain = variable(&db, "U", None);
    let defaulted = variable(&db, "U", Some(target_ty));
    for asynchronous in [false, true] {
        let effects = Trace::new(&db, &env, &visitor);
        let state = SelfReferenceState::new();
        for (variable, expected) in [(plain, false), (plain, false), (defaulted, true)] {
            assert_eq!(
                run(
                    Entry::Variable(variable),
                    asynchronous,
                    target.identity(&db),
                    &state,
                    &effects
                ),
                Poll::Ready(Ok(expected))
            );
        }
        assert_eq!(*effects.checked.borrow(), [plain, defaulted]);
        assert_eq!(state.seen_typevars.borrow().len(), 2);
        let events = effects.events.borrow();
        assert_eq!(
            &events[..3],
            ["identity", "remember_variable", "checked_default"]
        );
        assert!(events.ends_with(&["search", "variable_reference", "identity"]));
    }
    Ok(())
}

/// An admission requested while inserting a complete type-variable instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VariableSetEvent<'db> {
    Access(TypeVarInstance<'db>),
    Grow(GrowthPlan),
}

/// Records admissions and optionally refuses the event at a zero-based index.
#[derive(Debug, Default)]
struct VariableSetControl<'db> {
    events: Vec<VariableSetEvent<'db>>,
    refuse: Option<usize>,
}

impl<'db> VariableSetControl<'db> {
    /// Records a requested admission and returns its index when it is refused.
    fn record(&mut self, event: VariableSetEvent<'db>) -> Result<(), TddError<usize>> {
        let index = self.events.len();
        self.events.push(event);
        if self.refuse == Some(index) {
            Err(TddError::Refused(index))
        } else {
            Ok(())
        }
    }
}

impl<'db> SmallSetControl<TypeVarInstance<'db>> for VariableSetControl<'db> {
    type Error = usize;

    fn access(&mut self, value: TypeVarInstance<'db>) -> Result<(), TddError<usize>> {
        self.record(VariableSetEvent::Access(value))
    }

    fn grow(&mut self, plan: GrowthPlan) -> Result<(), TddError<usize>> {
        self.record(VariableSetEvent::Grow(plan))
    }
}

/// The two insertions that can change the visited-variable set's storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VariableSetBoundary {
    FirstSpill,
    HashGrowth,
}

/// Creates a visited-variable set filled to a spill or hash-growth boundary using distinct defaults
/// on U.
fn full_variable_set(db: &dyn Db, boundary: VariableSetBoundary) -> SmallSet<TypeVarInstance<'_>, 8> {
    let mut seen = SelfReferenceState::new().seen_typevars.into_inner();
    let spilled = match boundary {
        VariableSetBoundary::FirstSpill => false,
        VariableSetBoundary::HashGrowth => true,
    };
    let mut default = 0;
    while seen.is_spilled() != spilled || seen.len() < seen.layout().capacity {
        assert!(seen.insert(variable(db, "U", Some(Type::int_literal(default)))));
        default += 1;
    }
    seen
}

/// Copies the keys in their current inline-buffer or hash-table iteration order.
fn variable_set_keys<'db>(seen: &SmallSet<TypeVarInstance<'db>, 8>) -> Vec<TypeVarInstance<'db>> {
    match seen {
        SmallSet::Inline(values) => values.to_vec(),
        SmallSet::Spilled(values) => values.iter().copied().collect(),
    }
}

/// Distinct defaults on one variable remain separate keys across spills and hash growth;
/// duplicates require only the incoming-key admission and leave storage unchanged.
#[test_case::test_case(VariableSetBoundary::FirstSpill; "first spill")]
#[test_case::test_case(VariableSetBoundary::HashGrowth; "hash growth")]
fn variable_set_preserves_complete_keys(boundary: VariableSetBoundary) -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let mut seen = full_variable_set(&db, boundary);
    let before = seen.layout();
    let keys = variable_set_keys(&seen);
    let incoming = variable(&db, "U", None);
    assert!(
        keys.iter()
            .all(|key| key.identity(&db) == incoming.identity(&db))
    );
    assert!(!keys.contains(&incoming));

    let mut control = VariableSetControl::default();
    assert_eq!(seen.insert_with(keys[0], &mut control), Ok(false));
    assert_eq!(control.events, [VariableSetEvent::Access(keys[0])]);
    assert_eq!(seen.layout(), before);
    assert_eq!(variable_set_keys(&seen), keys);

    control.events.clear();
    assert_eq!(seen.insert_with(incoming, &mut control), Ok(true));
    let accesses: Vec<_> = std::iter::once(incoming)
        .chain(keys.iter().copied())
        .map(VariableSetEvent::Access)
        .collect();
    assert_eq!(control.events.len(), accesses.len() + 1);
    assert_eq!(control.events[..accesses.len()], accesses);
    let VariableSetEvent::Grow(plan) = control.events[accesses.len()] else {
        anyhow::bail!("expected storage growth after all key admissions");
    };
    assert!(plan.requested_capacity > before.len);
    assert_eq!(
        plan.requested_payload_bytes,
        plan.requested_capacity * size_of::<TypeVarInstance<'_>>()
    );
    assert_eq!(plan.relocation_units, before.len);
    assert!(seen.is_spilled());
    assert!(seen.layout().capacity > before.capacity);
    let expected: FxHashSet<_> = keys.iter().copied().chain([incoming]).collect();
    assert_eq!(seen.len(), expected.len());
    assert_eq!(
        variable_set_keys(&seen).into_iter().collect::<FxHashSet<_>>(),
        expected
    );

    let after = seen.layout();
    control.events.clear();
    assert_eq!(seen.insert_with(incoming, &mut control), Ok(false));
    assert_eq!(control.events, [VariableSetEvent::Access(incoming)]);
    assert_eq!(seen.layout(), after);
    assert_eq!(
        variable_set_keys(&seen).into_iter().collect::<FxHashSet<_>>(),
        expected
    );
    Ok(())
}

/// A point where insertion must stop without moving or adding any key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VariableSetRefusal {
    IncomingKey,
    FirstRetainedKey,
    LastRetainedKey,
    Growth,
}

/// Refusing admission for incoming-key access, retained-key access, or storage growth preserves
/// every entry, the inline or spilled representation, and its capacity, and stops further admissions.
#[test_case::test_case(VariableSetBoundary::FirstSpill, VariableSetRefusal::IncomingKey; "spill incoming key")]
#[test_case::test_case(VariableSetBoundary::FirstSpill, VariableSetRefusal::FirstRetainedKey; "spill first retained key")]
#[test_case::test_case(VariableSetBoundary::FirstSpill, VariableSetRefusal::LastRetainedKey; "spill last retained key")]
#[test_case::test_case(VariableSetBoundary::FirstSpill, VariableSetRefusal::Growth; "spill growth")]
#[test_case::test_case(VariableSetBoundary::HashGrowth, VariableSetRefusal::IncomingKey; "hash incoming key")]
#[test_case::test_case(VariableSetBoundary::HashGrowth, VariableSetRefusal::FirstRetainedKey; "hash first retained key")]
#[test_case::test_case(VariableSetBoundary::HashGrowth, VariableSetRefusal::LastRetainedKey; "hash last retained key")]
#[test_case::test_case(VariableSetBoundary::HashGrowth, VariableSetRefusal::Growth; "hash growth")]
fn variable_set_refusal_preserves_storage(
    boundary: VariableSetBoundary,
    refusal: VariableSetRefusal,
) -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let mut seen = full_variable_set(&db, boundary);
    let before = seen.layout();
    let keys = variable_set_keys(&seen);
    let incoming = variable(&db, "U", None);
    let refuse = match refusal {
        VariableSetRefusal::IncomingKey => 0,
        VariableSetRefusal::FirstRetainedKey => 1,
        VariableSetRefusal::LastRetainedKey => keys.len(),
        VariableSetRefusal::Growth => keys.len() + 1,
    };
    let mut control = VariableSetControl {
        refuse: Some(refuse),
        ..VariableSetControl::default()
    };
    assert_eq!(
        seen.insert_with(incoming, &mut control),
        Err(TddError::Refused(refuse))
    );
    assert_eq!(seen.layout(), before);
    assert_eq!(variable_set_keys(&seen), keys);
    assert_eq!(control.events.len(), refuse + 1);
    let accesses: Vec<_> = std::iter::once(incoming)
        .chain(keys.iter().copied())
        .map(VariableSetEvent::Access)
        .collect();
    let requested_keys = control.events.len().min(accesses.len());
    assert_eq!(control.events[..requested_keys], accesses[..requested_keys]);
    match refusal {
        VariableSetRefusal::IncomingKey
        | VariableSetRefusal::FirstRetainedKey
        | VariableSetRefusal::LastRetainedKey => {}
        VariableSetRefusal::Growth => {
            let VariableSetEvent::Grow(_) = control.events[refuse] else {
                anyhow::bail!("expected the storage-growth admission to be refused");
            };
        }
    }
    Ok(())
}

#[test]
fn structural_search_preserves_eager_bounds_and_defaults() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let target = variable(&db, "T", None);
    let target_ty = Type::KnownInstance(KnownInstanceType::TypeVar(target));
    let identity =
        TypeVarIdentity::new(&db, Name::new_static("U"), None, TypeVarKind::Pep695TypeVar);
    let bounded = TypeVarInstance::new(
        &db,
        identity,
        Some(TypeVarBoundOrConstraints::UpperBound(target_ty).into()),
        None,
        None,
    );
    let defaulted = variable(&db, "V", Some(target_ty));
    let ordinary = OrdinarySelfReferenceEffects {
        db: &db,
        env: &env,
        visitor: &visitor,
    };
    for instance in [bounded, defaulted] {
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(instance));
        assert!(infallible(type_is_self_referential_sync(
            target, ty, &ordinary
        )));
        for asynchronous in [false, true] {
            let effects = Trace::new(&db, &env, &visitor);
            let state = SelfReferenceState::new();
            assert_eq!(
                run(
                    Entry::Root(target, ty),
                    asynchronous,
                    target.identity(&db),
                    &state,
                    &effects
                ),
                Poll::Ready(Ok(true))
            );
        }
    }
    Ok(())
}

#[test]
fn alias_and_recursive_arguments_precede_shared_body_deduplication() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/self_reference.py", "type Alias[T] = int\n")
        .build()?;
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let alias = alias(&db)?;
    let target = variable(&db, "Target", None);
    let target_ty = Type::KnownInstance(KnownInstanceType::TypeVar(target));
    let parameter = variable(&db, "U", None);
    let context = GenericContext::from_typevar_instances(&db, &env, [bound(&db, &env, parameter)]);
    let arguments = context.specialize(&db, [target_ty].as_slice());
    let definition = alias.definition(&db);
    let recursive = RecursiveType::initial(&db, definition, definition.as_id(), None);
    for entry in [Entry::Alias(alias), Entry::Recursive(recursive)] {
        for asynchronous in [false, true] {
            let mut effects = Trace::new(&db, &env, &visitor);
            effects.specialization = Some(arguments);
            effects.recursive_arguments = Some(arguments);
            let state = SelfReferenceState::new();
            state.seen_types.borrow_mut().push(effects.body_identity);
            assert_eq!(
                run(entry, asynchronous, target.identity(&db), &state, &effects),
                Poll::Ready(Ok(true))
            );
            let events = effects.events.borrow();
            assert_eq!(
                &events[1..4],
                ["specialization_types", "next_type", "search"]
            );
            assert!(!events.contains(&"remember_type"));
            assert!(!events.contains(&"alias_value"));
            assert!(!events.contains(&"recursive_unfold"));
        }
    }

    let effects = Trace::new(&db, &env, &visitor);
    let state = SelfReferenceState::new();
    assert_eq!(
        run(
            Entry::Alias(alias),
            false,
            target.identity(&db),
            &state,
            &effects
        ),
        Poll::Ready(Ok(false))
    );
    assert_eq!(
        run(
            Entry::Recursive(recursive),
            false,
            target.identity(&db),
            &state,
            &effects
        ),
        Poll::Ready(Ok(false))
    );
    assert_eq!(state.seen_types.borrow().len(), 1);
    assert!(!effects.events.borrow().contains(&"recursive_unfold"));
    Ok(())
}

#[test]
fn alias_generic_defaults_precede_identity_and_select_raw_body() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/self_reference.py", "type Alias[T] = int\n")
        .build()?;
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let alias = alias(&db)?;
    let target = variable(&db, "Target", None);
    let target_ty = Type::KnownInstance(KnownInstanceType::TypeVar(target));
    for (default, expected) in [(Some(target_ty), true), (None, false)] {
        let parameter = variable(&db, "U", default);
        let context =
            GenericContext::from_typevar_instances(&db, &env, [bound(&db, &env, parameter)]);
        for asynchronous in [false, true] {
            let mut effects = Trace::new(&db, &env, &visitor);
            effects.context = Some(context);
            let state = SelfReferenceState::new();
            assert_eq!(
                run(
                    Entry::Alias(alias),
                    asynchronous,
                    target.identity(&db),
                    &state,
                    &effects
                ),
                Poll::Ready(Ok(expected))
            );
            let events = effects.events.borrow();
            assert_eq!(
                &events[..6],
                [
                    "alias_specialization",
                    "alias_generic_context",
                    "generic_variables",
                    "next_variable",
                    "bound_typevar",
                    "variable_reference"
                ]
            );
            assert_eq!(events.contains(&"alias_identity"), !expected);
            assert_eq!(events.contains(&"alias_raw_value"), !expected);
            assert!(!events.contains(&"alias_value"));
        }
    }
    Ok(())
}

#[test]
fn selected_effect_refusals_preserve_sync_async_order() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/self_reference.py", "type Alias[T] = int\n")
        .build()?;
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let alias = alias(&db)?;
    let target = variable(&db, "Target", None);
    let parameter = variable(&db, "U", Some(Type::int_literal(3)));
    let context = GenericContext::from_typevar_instances(&db, &env, [bound(&db, &env, parameter)]);
    let specialization = context.specialize(&db, [Type::int_literal(5)].as_slice());
    let definition = alias.definition(&db);
    let recursive = RecursiveType::initial(&db, definition, definition.as_id(), None);
    for (entry, specialized) in [
        (
            Entry::Root(
                target,
                Type::KnownInstance(KnownInstanceType::TypeVar(parameter)),
            ),
            false,
        ),
        (Entry::Alias(alias), false),
        (Entry::Alias(alias), true),
        (Entry::Recursive(recursive), true),
        (
            Entry::Predicate(Type::KnownInstance(KnownInstanceType::TypeAliasType(alias))),
            false,
        ),
    ] {
        let mut baseline = Trace::new(&db, &env, &visitor);
        baseline.context = Some(context);
        baseline.specialization = specialized.then_some(specialization);
        baseline.recursive_arguments = specialized.then_some(specialization);
        assert_eq!(
            run(
                entry,
                false,
                target.identity(&db),
                &SelfReferenceState::new(),
                &baseline
            ),
            Poll::Ready(Ok(false))
        );
        let expected = baseline.events.into_inner();
        if matches!(entry, Entry::Alias(_)) {
            assert_eq!(expected.contains(&"alias_value"), specialized);
            assert_eq!(expected.contains(&"alias_raw_value"), !specialized);
        }
        for asynchronous in [false, true] {
            for refuse in std::iter::once(None).chain((0..expected.len()).map(Some)) {
                let mut effects = Trace::new(&db, &env, &visitor);
                effects.context = Some(context);
                effects.specialization = specialized.then_some(specialization);
                effects.recursive_arguments = specialized.then_some(specialization);
                effects.refuse = refuse;
                let result = run(
                    entry,
                    asynchronous,
                    target.identity(&db),
                    &SelfReferenceState::new(),
                    &effects,
                );
                assert_eq!(
                    result,
                    Poll::Ready(refuse.map_or(Ok(false), |index| Err(expected[index])))
                );
                assert_eq!(
                    *effects.events.borrow(),
                    expected[..refuse.map_or(expected.len(), |index| index + 1)]
                );
            }
        }
    }
    Ok(())
}
