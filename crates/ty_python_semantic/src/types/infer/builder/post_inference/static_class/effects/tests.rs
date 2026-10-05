use std::cell::{Cell, RefCell};
use std::collections::TryReserveError;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_text_size::{Ranged, TextRange};
use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};

use super::*;
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::control::{GrowthPlan, TddError, sequence_growth};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{ClassLiteral, GenericAlias, Type};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/classes.py",
            r#"
from typing import NamedTuple, TypedDict, Protocol
class Plain: pass
class Other: pass
class Fields(NamedTuple):
    required: int
    first: int = 1
    latest_default_with_a_name_long_enough_to_share_storage: int = 2
    _last: int
    after: int
class Empty(NamedTuple): pass
class Mapping(TypedDict):
    value: int
class GenericMapping[T](TypedDict):
    value: T
class ProtocolBase(Protocol): pass
class Header(Plain, Mapping, GenericMapping[int]): pass
"#,
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = db.program_file(system_path_to_file(db, "/src/classes.py")?);
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(|class| class.as_static())
        .ok_or_else(|| anyhow::anyhow!("missing {name} class"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event<'db> {
    Fields,
    Advance,
    Remember(Name, Option<Definition<'db>>),
    Underscore(Name, Option<Definition<'db>>),
    Required(Name, Option<Definition<'db>>, Name, Option<Definition<'db>>),
    Protocol(ClassType<'db>),
    Object(ClassType<'db>),
    TypedDict(ClassType<'db>),
    InvalidProtocol {
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source: TextRange,
    },
    InvalidTypedDict {
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source: TextRange,
    },
    Append,
}

// Semantic answers are supplied explicitly. These controls establish decision order and shared
// lowering, while ordinary integration exercises the real source and diagnostic providers.
struct Recording<'db> {
    fields: &'db FxIndexMap<Name, Field<'db>>,
    events: RefCell<Vec<Event<'db>>>,
    reject_at: Option<usize>,
    protocol: bool,
    object: bool,
    typed_dict: bool,
}

impl<'db> Recording<'db> {
    fn new(fields: &'db FxIndexMap<Name, Field<'db>>) -> Self {
        Self {
            fields,
            events: RefCell::default(),
            reject_at: None,
            protocol: false,
            object: false,
            typed_dict: false,
        }
    }

    fn record(&self, event: Event<'db>) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let position = events.len();
        events.push(event);
        if self.reject_at == Some(position) {
            Err("refused")
        } else {
            Ok(())
        }
    }
}

impl SynchronousStaticClassLocalEffects for Recording<'_> {
    type Error = &'static str;

    fn next_map_entry<'a, K, V>(
        &self,
        fields: &'a FxIndexMap<K, V>,
        cursor: &mut usize,
    ) -> Result<Option<(&'a K, &'a V)>, Self::Error> {
        if *cursor < fields.len() {
            self.record(Event::Advance)?;
        }
        Ok(next_map_entry(fields, cursor))
    }

    fn remember_named_tuple_default<'db>(
        &self,
        previous: &mut PreviousNamedTupleDefault<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        // Declaration identities are asserted by report requests; local replacement is generic
        // over the owner's database lifetime and records only its work boundary here.
        self.record(Event::Remember(name.clone(), None))?;
        remember_named_tuple_default(previous, name, declaration);
        Ok(())
    }

    fn append_copy<T: Copy>(&self, values: &mut Vec<T>, value: T) -> Result<(), Self::Error> {
        self.record(Event::Append)?;
        append_copy(values, value);
        Ok(())
    }
}

impl StaticClassLocalEffects for Recording<'_> {
    type Error = &'static str;
    async fn next_map_entry<'a, K, V>(
        &self,
        fields: &'a FxIndexMap<K, V>,
        cursor: &mut usize,
    ) -> Result<Option<(&'a K, &'a V)>, Self::Error> {
        SynchronousStaticClassLocalEffects::next_map_entry(self, fields, cursor)
    }
    async fn remember_named_tuple_default<'db>(
        &self,
        previous: &mut PreviousNamedTupleDefault<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        SynchronousStaticClassLocalEffects::remember_named_tuple_default(
            self,
            previous,
            name,
            declaration,
        )
    }
    async fn append_copy<T: Copy>(&self, values: &mut Vec<T>, value: T) -> Result<(), Self::Error> {
        SynchronousStaticClassLocalEffects::append_copy(self, values, value)
    }
}

impl<'db> SynchronousNamedTupleFieldEffects<'db> for Recording<'db> {
    fn named_tuple_fields(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<&'db FxIndexMap<Name, Field<'db>>, Self::Error> {
        self.record(Event::Fields)?;
        Ok(self.fields)
    }
    fn report_leading_underscore(
        &self,
        _class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        self.record(Event::Underscore(name.clone(), declaration))
    }
    fn report_required_after_default(
        &self,
        _class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
        previous: &(Name, Option<Definition<'db>>),
    ) -> Result<(), Self::Error> {
        self.record(Event::Required(
            name.clone(),
            declaration,
            previous.0.clone(),
            previous.1,
        ))
    }
}

impl<'db> NamedTupleFieldEffects<'db> for Recording<'db> {
    async fn named_tuple_fields(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db FxIndexMap<Name, Field<'db>>, Self::Error> {
        SynchronousNamedTupleFieldEffects::named_tuple_fields(self, class)
    }
    async fn report_leading_underscore(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        SynchronousNamedTupleFieldEffects::report_leading_underscore(self, class, name, declaration)
    }
    async fn report_required_after_default(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
        previous: &(Name, Option<Definition<'db>>),
    ) -> Result<(), Self::Error> {
        SynchronousNamedTupleFieldEffects::report_required_after_default(
            self,
            class,
            name,
            declaration,
            previous,
        )
    }
}

impl<'db> SynchronousExplicitBaseKindEffects<'db> for Recording<'db> {
    fn base_is_protocol(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        self.record(Event::Protocol(base))?;
        Ok(self.protocol)
    }
    fn base_is_object(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        self.record(Event::Object(base))?;
        Ok(self.object)
    }
    fn base_is_typed_dict(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        self.record(Event::TypedDict(base))?;
        Ok(self.typed_dict)
    }
    fn report_invalid_protocol_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        self.record(Event::InvalidProtocol {
            class,
            base,
            source: source_node.range(),
        })
    }
    fn report_invalid_typed_dict_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        self.record(Event::InvalidTypedDict {
            class,
            base,
            source: source_node.range(),
        })
    }
}

impl<'db> ExplicitBaseKindEffects<'db> for Recording<'db> {
    async fn base_is_protocol(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        SynchronousExplicitBaseKindEffects::base_is_protocol(self, base)
    }
    async fn base_is_object(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        SynchronousExplicitBaseKindEffects::base_is_object(self, base)
    }
    async fn base_is_typed_dict(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        SynchronousExplicitBaseKindEffects::base_is_typed_dict(self, base)
    }
    async fn report_invalid_protocol_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        SynchronousExplicitBaseKindEffects::report_invalid_protocol_base(self, class, base, node)
    }
    async fn report_invalid_typed_dict_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        SynchronousExplicitBaseKindEffects::report_invalid_typed_dict_base(self, class, base, node)
    }
}

fn original_named_tuple_events<'db>(fields: &FxIndexMap<Name, Field<'db>>) -> Vec<Event<'db>> {
    let mut events = vec![Event::Fields];
    let mut previous = None;
    for (name, field) in fields {
        events.push(Event::Advance);
        if name.starts_with('_') {
            events.push(Event::Underscore(name.clone(), field.first_declaration));
        }
        if matches!(
            field.kind,
            FieldKind::NamedTuple {
                default_ty: Some(_)
            }
        ) {
            events.push(Event::Remember(name.clone(), None));
            previous = Some((name.clone(), field.first_declaration));
        } else if let Some((previous_name, previous_definition)) = &previous {
            events.push(Event::Required(
                name.clone(),
                field.first_declaration,
                previous_name.clone(),
                *previous_definition,
            ));
        }
    }
    events
}

#[test]
fn named_tuple_order_matches_the_original_loop_and_stops_at_every_refusal() -> anyhow::Result<()> {
    let db = database()?;
    let fields_class = class(&db, "Fields")?;
    let fields = fields_class.own_fields(&db, None, CodeGeneratorKind::NamedTuple);
    let mut missing = fields.clone();
    for field in missing.values_mut() {
        field.first_declaration = None;
    }
    let empty = FxIndexMap::default();
    for fields in [fields, &missing, &empty] {
        let expected = original_named_tuple_events(fields);
        for reject_at in (0..expected.len()).map(Some).chain([None]) {
            let asynchronous = Recording {
                reject_at,
                ..Recording::new(fields)
            };
            let synchronous = Recording {
                reject_at,
                ..Recording::new(fields)
            };
            let async_result = try_poll_immediate(check_named_tuple_fields_with(
                fields_class,
                StaticClassFacts,
                &asynchronous,
            ));
            let sync_result =
                check_named_tuple_fields_sync(fields_class, StaticClassFacts, &synchronous);
            assert_eq!(async_result, Poll::Ready(sync_result));
            assert_eq!(sync_result.is_err(), reject_at.is_some());
            let end = reject_at.map_or(expected.len(), |index| index + 1);
            assert_eq!(*asynchronous.events.borrow(), expected[..end]);
            assert_eq!(*synchronous.events.borrow(), expected[..end]);
        }
    }
    let Some(last) = fields.get("_last") else {
        anyhow::bail!("missing last field")
    };
    let Some(latest) = fields.get("latest_default_with_a_name_long_enough_to_share_storage") else {
        anyhow::bail!("missing latest field")
    };
    let expected = original_named_tuple_events(fields);
    let last_report = expected
        .iter()
        .position(|event| matches!(event, Event::Underscore(name, _) if name == "_last"))
        .ok_or_else(|| anyhow::anyhow!("missing underscore report"))?;
    assert_eq!(
        &expected[last_report..last_report + 2],
        &[
            Event::Underscore(Name::new_static("_last"), last.first_declaration),
            Event::Required(
                Name::new_static("_last"),
                last.first_declaration,
                Name::new_static("latest_default_with_a_name_long_enough_to_share_storage"),
                latest.first_declaration
            ),
        ]
    );
    Ok(())
}

fn specialized_base<'db>(db: &'db TestDb) -> anyhow::Result<ClassType<'db>> {
    let origin = class(db, "GenericMapping")?;
    let context = origin
        .generic_context(db)
        .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
    let arguments = [Type::int_literal(7)];
    let specialization = context.specialize(db, arguments.as_slice());
    Ok(ClassType::Generic(GenericAlias::new(
        db,
        origin,
        specialization,
    )))
}

#[test]
fn explicit_base_short_circuits_and_accumulates_in_original_order() -> anyhow::Result<()> {
    let db = database()?;
    let subject = class(&db, "Header")?;
    let existing = ClassType::NonGeneric(ClassLiteral::Static(class(&db, "Plain")?));
    let ordinary_base = ClassType::NonGeneric(ClassLiteral::Static(class(&db, "Mapping")?));
    let specialized = specialized_base(&db)?;
    let file = db.program_file(system_path_to_file(&db, "/src/classes.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let header = module
        .suite()
        .iter()
        .find_map(|statement| match statement {
            ast::Stmt::ClassDef(node) if node.name.as_str() == "Header" => Some(node),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("missing Header node"))?;
    let empty = FxIndexMap::default();
    for (index, base) in [(1, ordinary_base), (2, specialized)] {
        let node = header
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.args.get(index))
            .ok_or_else(|| anyhow::anyhow!("missing selected base expression"))?;
        let source = node.range();
        for (is_protocol, typed_dict, protocol, object, valid, expected) in [
            (true, false, true, false, false, vec![Event::Protocol(base)]),
            (
                true,
                false,
                false,
                true,
                false,
                vec![Event::Protocol(base), Event::Object(base)],
            ),
            (
                true,
                false,
                false,
                false,
                false,
                vec![
                    Event::Protocol(base),
                    Event::Object(base),
                    Event::InvalidProtocol {
                        class: subject,
                        base,
                        source,
                    },
                ],
            ),
            (
                false,
                true,
                false,
                false,
                true,
                vec![
                    Event::TypedDict(base),
                    Event::TypedDict(base),
                    Event::Append,
                ],
            ),
            (
                false,
                true,
                false,
                false,
                false,
                vec![
                    Event::TypedDict(base),
                    Event::InvalidTypedDict {
                        class: subject,
                        base,
                        source,
                    },
                    Event::TypedDict(base),
                ],
            ),
            (false, false, false, false, false, vec![]),
        ] {
            for reject_at in (0..expected.len()).map(Some).chain([None]) {
                let asynchronous = Recording {
                    reject_at,
                    protocol,
                    object,
                    typed_dict: valid,
                    ..Recording::new(&empty)
                };
                let synchronous = Recording {
                    reject_at,
                    protocol,
                    object,
                    typed_dict: valid,
                    ..Recording::new(&empty)
                };
                let kind = typed_dict.then_some(CodeGeneratorKind::TypedDict);
                let mut async_bases = vec![existing];
                let mut sync_bases = vec![existing];
                let asynchronous_result = try_poll_immediate(check_explicit_base_kind_with(
                    subject,
                    base,
                    node,
                    is_protocol,
                    kind,
                    &mut async_bases,
                    &asynchronous,
                ));
                let synchronous_result = check_explicit_base_kind_sync(
                    subject,
                    base,
                    node,
                    is_protocol,
                    kind,
                    &mut sync_bases,
                    &synchronous,
                );
                assert_eq!(asynchronous_result, Poll::Ready(synchronous_result));
                assert_eq!(synchronous_result.is_err(), reject_at.is_some());
                assert_eq!(async_bases, sync_bases);
                assert_eq!(
                    async_bases,
                    if typed_dict && valid && reject_at.is_none() {
                        vec![existing, base]
                    } else {
                        vec![existing]
                    }
                );
                let end = reject_at.map_or(expected.len(), |index| index + 1);
                assert_eq!(*asynchronous.events.borrow(), expected[..end]);
                assert_eq!(*synchronous.events.borrow(), expected[..end]);
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
enum LocalMutationFailure {
    CapacityExhausted,
    AllocationFailed(TryReserveError),
}

fn append_plan<T>(len: usize, capacity: usize) -> Result<Option<GrowthPlan>, LocalMutationFailure> {
    let required = len
        .checked_add(1)
        .ok_or(LocalMutationFailure::CapacityExhausted)?;
    if required <= capacity {
        return Ok(None);
    }
    let mut plan = match sequence_growth::<T, Infallible>(capacity, required) {
        Ok(plan) => plan,
        Err(TddError::CapacityExhausted) => return Err(LocalMutationFailure::CapacityExhausted),
        Err(TddError::Refused(never)) => match never {},
    };
    plan.relocation_units = len;
    Ok(Some(plan))
}

#[derive(Default)]
struct Journal {
    admission: RefCell<Vec<ExecutionWork>>,
    drops: RefCell<Vec<&'static str>>,
    owner_live: Cell<bool>,
    factory_ran: Cell<bool>,
    after_await: Cell<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    Refuse,
    QueueChild,
}

// The registry's observer invokes this injection at the actual work/resource admission boundary.
// A retained endpoint lets the observer queue a child and return Ok, exercising completion checks.
struct Admission<'run, 'db: 'run> {
    endpoint: RefCell<Option<TaskEndpoint<'run, 'db>>>,
    journal: Rc<Journal>,
    fault: Fault,
    fault_at: usize,
    active: Cell<bool>,
    calls: Cell<usize>,
}

impl ExecutionAdmission for Admission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !self.active.get()
            || !matches!(
                work,
                ExecutionWork::Work { .. } | ExecutionWork::Resource { .. }
            )
        {
            return Ok(());
        }
        let index = self.calls.get();
        self.calls.set(index + 1);
        self.journal.admission.borrow_mut().push(work);
        if index != self.fault_at {
            return Ok(());
        }
        self.active.set(false);
        match self.fault {
            Fault::None => Ok(()),
            Fault::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
            Fault::QueueChild => {
                let endpoint = self.endpoint.borrow();
                let endpoint = endpoint
                    .as_ref()
                    .ok_or(RunError::Contract("test observer has no endpoint"))?;
                let child = Child {
                    journal: self.journal.clone(),
                };
                let _demand = endpoint.demand(move || {
                    child.journal.factory_ran.set(true);
                    async move {
                        let _child = child;
                        Ok(())
                    }
                })?;
                Ok(())
            }
        }
    }
}

struct ClearEndpoint(&'static Admission<'static, 'static>);
impl Drop for ClearEndpoint {
    fn drop(&mut self) {
        self.0.active.set(false);
        self.0.endpoint.borrow_mut().take();
    }
}

struct Child {
    journal: Rc<Journal>,
}
impl Drop for Child {
    fn drop(&mut self) {
        assert!(self.journal.owner_live.get());
        self.journal.drops.borrow_mut().push("child");
    }
}

struct Controlled<'call, 'run, 'db: 'run> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    allocation_failure: bool,
}

impl Controlled<'_, '_, '_> {
    fn work(&self, units: usize) -> RunResult<()> {
        self.endpoint.admit_work(units)?;
        self.endpoint.check_completion()
    }
    fn resource(&self, bytes: usize) -> RunResult<()> {
        self.endpoint.admit(ExecutionWork::Resource {
            requested_bytes: bytes,
        })?;
        self.endpoint.check_completion()
    }
}

impl StaticClassLocalEffects for Controlled<'_, '_, '_> {
    type Error = LocalMutationFailure;

    async fn next_map_entry<'a, K, V>(
        &self,
        fields: &'a FxIndexMap<K, V>,
        cursor: &mut usize,
    ) -> Result<Option<(&'a K, &'a V)>, Self::Error> {
        self.endpoint
            .local_call(|| {
                if *cursor < fields.len() {
                    self.work(1)?;
                }
                self.endpoint.check_completion()?;
                Ok(Ok(next_map_entry(fields, cursor)))
            })
            .await
    }

    async fn remember_named_tuple_default<'db>(
        &self,
        previous: &mut PreviousNamedTupleDefault<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| {
                self.work(1)?;
                self.endpoint.check_completion()?;
                remember_named_tuple_default(previous, name, declaration);
                Ok(Ok(()))
            })
            .await
    }

    async fn append_copy<T: Copy>(&self, values: &mut Vec<T>, value: T) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| {
                let plan = match append_plan::<T>(values.len(), values.capacity()) {
                    Ok(plan) => plan,
                    Err(error) => return Ok(Err(error)),
                };
                self.work(1)?;
                if let Some(plan) = plan {
                    self.work(plan.relocation_units.max(1))?;
                    self.resource(plan.requested_payload_bytes)?;
                }
                self.endpoint.check_completion()?;
                if let Some(plan) = plan {
                    // Inject a real typed reservation error without requesting host memory. This is
                    // an allocator-boundary control; it cannot call an observer or queue a child.
                    let reservation = if self.allocation_failure {
                        Vec::<u8>::new().try_reserve_exact(usize::MAX)
                    } else {
                        values.try_reserve_exact(plan.requested_capacity - values.len())
                    };
                    if let Err(error) = reservation {
                        return Ok(Err(LocalMutationFailure::AllocationFailed(error)));
                    }
                }
                append_copy(values, value);
                Ok(Ok(()))
            })
            .await
    }
}

struct Owner<'db> {
    module: ParsedModuleRef,
    fields: FxIndexMap<Name, Field<'db>>,
    cursor: usize,
    previous: PreviousNamedTupleDefault<'db>,
    bases: Vec<ClassType<'db>>,
    original: ClassType<'db>,
    expected_success: bool,
    operation: Operation,
    original_capacity: usize,
    journal: Rc<Journal>,
}

impl Drop for Owner<'_> {
    fn drop(&mut self) {
        assert!(!self.module.suite().is_empty());
        let succeeded = self.expected_success && self.journal.after_await.get();
        assert_eq!(
            self.cursor,
            usize::from(succeeded && self.operation == Operation::Advance)
        );
        assert_eq!(
            self.previous.as_ref().map(|(name, _)| name.as_str()),
            Some(if succeeded && self.operation == Operation::Remember {
                "new_default_with_a_long_shared_name"
            } else {
                "old"
            })
        );
        assert_eq!(
            self.bases.len(),
            if succeeded
                && matches!(
                    self.operation,
                    Operation::AppendSpare | Operation::AppendGrowth
                )
            {
                2
            } else {
                1
            }
        );
        assert!(self.bases.iter().all(|base| *base == self.original));
        if !succeeded {
            assert_eq!(self.bases.capacity(), self.original_capacity);
        }
        assert!(self.journal.owner_live.replace(false));
        self.journal.drops.borrow_mut().push("owner");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Advance,
    Remember,
    AppendSpare,
    AppendGrowth,
}

fn run_local_case(
    operation: Operation,
    fault: Fault,
    fault_at: usize,
    allocation_failure: bool,
    accepted_child: bool,
) -> anyhow::Result<()> {
    // The observer and its retained endpoint refer to the same registry. The static fixture
    // follows the runtime's injection tests; ClearEndpoint breaks that reference on every exit.
    // The caller-owned values below are still created and destroyed by the root future.
    let db: &'static TestDb = Box::leak(Box::new(database()?));
    let class = class(db, "Fields")?;
    let base = ClassType::NonGeneric(ClassLiteral::Static(class));
    let journal = Rc::new(Journal::default());
    let admission: &'static Admission<'static, 'static> = Box::leak(Box::new(Admission {
        endpoint: RefCell::new(None),
        journal: journal.clone(),
        fault,
        fault_at,
        active: Cell::new(false),
        calls: Cell::new(0),
    }));
    let _clear = ClearEndpoint(admission);
    let fields = class
        .own_fields(db, None, CodeGeneratorKind::NamedTuple)
        .clone();
    let file = db.program_file(system_path_to_file(db, "/src/classes.py")?);
    let module = parsed_module(db, file.python_file(db)).load(db);
    let root_journal = journal.clone();
    let outcome = try_with_attempt(db, 100_000, || {
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                let journal = root_journal;
                let mut bases = if operation == Operation::AppendSpare {
                    Vec::with_capacity(4)
                } else {
                    Vec::with_capacity(1)
                };
                bases.push(base);
                let original_capacity = bases.capacity();
                let mut owner = Owner {
                    module,
                    fields,
                    cursor: 0,
                    previous: Some((Name::new_static("old"), None)),
                    bases,
                    original: base,
                    expected_success: fault == Fault::None && !allocation_failure,
                    operation,
                    original_capacity,
                    journal: journal.clone(),
                };
                assert!(!journal.owner_live.replace(true));
                if accepted_child {
                    let retained = &owner;
                    let child = Child {
                        journal: journal.clone(),
                    };
                    endpoint
                        .child_call(|| async {
                            let _retained = retained;
                            let demand = endpoint.demand(move || {
                                child.journal.factory_ran.set(true);
                                async move {
                                    let _child = child;
                                    Ok(())
                                }
                            })?;
                            demand.await
                        })
                        .await;
                }
                *admission.endpoint.borrow_mut() = Some(endpoint.clone());
                admission.active.set(true);
                let control = Controlled {
                    endpoint: &endpoint,
                    allocation_failure,
                };
                let result = match operation {
                    Operation::Advance => control
                        .next_map_entry(&owner.fields, &mut owner.cursor)
                        .await
                        .map(|_| ()),
                    Operation::Remember => {
                        let new_name = Name::new_heap("new_default_with_a_long_shared_name");
                        control
                            .remember_named_tuple_default(&mut owner.previous, &new_name, None)
                            .await
                    }
                    Operation::AppendSpare | Operation::AppendGrowth => {
                        control.append_copy(&mut owner.bases, base).await
                    }
                };
                admission.active.set(false);
                journal.after_await.set(true);
                if allocation_failure {
                    match result {
                        Err(LocalMutationFailure::AllocationFailed(error)) => {
                            assert!(!error.to_string().is_empty())
                        }
                        other => {
                            return Err(RunError::Contract(if other.is_ok() {
                                "allocation injection succeeded"
                            } else {
                                "wrong allocation failure"
                            }));
                        }
                    }
                } else if result.is_err() {
                    return Err(RunError::Contract("unexpected local mutation failure"));
                }
                Ok(())
            })
    });
    admission.endpoint.borrow_mut().take();
    match fault {
        Fault::None => assert!(
            matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
            "{outcome:?}"
        ),
        Fault::Refuse | Fault::QueueChild => {
            assert!(
                !matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
                "{outcome:?}"
            );
            assert!(!journal.after_await.get());
        }
    }
    let mut expected_drops = Vec::new();
    if accepted_child {
        expected_drops.push("child");
    }
    if fault == Fault::QueueChild {
        expected_drops.push("child");
    }
    expected_drops.push("owner");
    assert_eq!(*journal.drops.borrow(), expected_drops);
    assert_eq!(journal.factory_ran.get(), accepted_child);
    let expected_calls = if fault == Fault::None {
        if operation == Operation::AppendGrowth {
            3
        } else {
            1
        }
    } else {
        fault_at + 1
    };
    assert_eq!(admission.calls.get(), expected_calls);
    assert!(
        journal
            .admission
            .borrow()
            .iter()
            .all(|work| !matches!(work, ExecutionWork::Work { units: 0 }))
    );
    Ok(())
}

#[test]
fn every_admission_refusal_or_queued_child_preserves_the_retained_owner() -> anyhow::Result<()> {
    for operation in [
        Operation::Advance,
        Operation::Remember,
        Operation::AppendSpare,
        Operation::AppendGrowth,
    ] {
        let positions = if operation == Operation::AppendGrowth {
            3
        } else {
            1
        };
        for position in 0..positions {
            for fault in [Fault::Refuse, Fault::QueueChild] {
                run_local_case(operation, fault, position, false, false)?;
            }
        }
        run_local_case(operation, Fault::None, usize::MAX, false, false)?;
    }
    Ok(())
}

#[test]
fn fallible_growth_preserves_values_and_reports_allocation_failure() -> anyhow::Result<()> {
    run_local_case(
        Operation::AppendGrowth,
        Fault::None,
        usize::MAX,
        true,
        false,
    )?;
    assert!(matches!(
        append_plan::<u8>(usize::MAX, usize::MAX),
        Err(LocalMutationFailure::CapacityExhausted)
    ));
    assert!(matches!(
        append_plan::<u64>(isize::MAX as usize, isize::MAX as usize),
        Err(LocalMutationFailure::CapacityExhausted)
    ));
    Ok(())
}

#[test]
fn accepted_child_then_refusal_keeps_the_owner_and_a_fresh_retry_succeeds() -> anyhow::Result<()> {
    run_local_case(Operation::Remember, Fault::Refuse, 0, false, true)?;
    run_local_case(Operation::Remember, Fault::None, usize::MAX, false, true)?;
    Ok(())
}

#[test]
fn exhausted_cursors_and_zero_sized_values_need_no_growth() -> anyhow::Result<()> {
    let db = database()?;
    let journal = Rc::new(Journal::default());
    let admission = Admission {
        endpoint: RefCell::new(None),
        journal: journal.clone(),
        fault: Fault::None,
        fault_at: usize::MAX,
        active: Cell::new(false),
        calls: Cell::new(0),
    };
    let admission_ref = &admission;
    let outcome = try_with_attempt(&db, 100_000, || {
        RegistryBuilder::new(&db, admission_ref)?
            .seal()?
            .run(|endpoint| async move {
                let admission = admission_ref;
                admission.active.set(true);
                let control = Controlled {
                    endpoint: &endpoint,
                    allocation_failure: false,
                };
                let empty: FxIndexMap<Name, ()> = FxIndexMap::default();
                let mut cursor = usize::MAX;
                assert!(matches!(
                    control.next_map_entry(&empty, &mut cursor).await,
                    Ok(None)
                ));
                assert_eq!(cursor, usize::MAX);
                assert_eq!(admission.calls.get(), 0);
                let mut units = vec![()];
                assert!(control.append_copy(&mut units, ()).await.is_ok());
                assert!(control.append_copy(&mut units, ()).await.is_ok());
                assert_eq!(units.len(), 3);
                assert_eq!(admission.calls.get(), 2);
                admission.active.set(false);
                Ok(())
            })
    });
    assert!(
        matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
        "{outcome:?}"
    );
    Ok(())
}

#[derive(Default)]
struct BodyJournal {
    owner_live: Cell<bool>,
    default_borrow_live: Cell<bool>,
    children: Cell<usize>,
    pending: Cell<usize>,
    resumed: Cell<bool>,
    drops: RefCell<Vec<&'static str>>,
}

struct BodyChild {
    journal: Rc<BodyJournal>,
    borrows_default: bool,
}

impl Drop for BodyChild {
    fn drop(&mut self) {
        assert!(self.journal.owner_live.get());
        if self.borrows_default {
            assert!(self.journal.default_borrow_live.get());
        }
        self.journal.children.set(self.journal.children.get() - 1);
        self.journal.drops.borrow_mut().push("child");
    }
}

// This guard borrows the actual tuple owned by the authored future. The queued request receives
// an owned recording, while the guard checks the borrowed value after the child has retired.
struct BorrowedDefault<'a, 'db> {
    previous: &'a (Name, Option<Definition<'db>>),
    declaration: Option<Definition<'db>>,
    journal: Rc<BodyJournal>,
}

impl Drop for BorrowedDefault<'_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert_eq!(
            self.previous.0.as_str(),
            "latest_default_with_a_name_long_enough_to_share_storage"
        );
        assert_eq!(self.previous.1, self.declaration);
        assert!(self.journal.default_borrow_live.replace(false));
        self.journal.drops.borrow_mut().push("previous");
    }
}

struct ObservedBody<F> {
    future: Option<Pin<Box<F>>>,
    journal: Rc<BodyJournal>,
}

impl<F: Future> Future for ObservedBody<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(future) = this.future.as_mut() else {
            return Poll::Pending;
        };
        let result = future.as_mut().poll(context);
        if result.is_pending() {
            this.journal.pending.set(this.journal.pending.get() + 1);
        }
        result
    }
}

impl<F> Drop for ObservedBody<F> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        drop(self.future.take());
        assert!(!self.journal.default_borrow_live.get());
        self.journal.drops.borrow_mut().push("body");
    }
}

struct SuspendedRecording<'call, 'run, 'db: 'run> {
    local: Controlled<'call, 'run, 'db>,
    recording: &'call Recording<'db>,
    journal: Rc<BodyJournal>,
    semantic_calls: Cell<usize>,
    reject_at: Option<usize>,
}

impl<'run, 'db: 'run> SuspendedRecording<'_, 'run, 'db> {
    async fn child<T: Copy + 'run>(
        &self,
        event: Event<'db>,
        answer: T,
        borrows_default: bool,
    ) -> T {
        self.local
            .endpoint
            .child_call(|| async {
                self.recording.events.borrow_mut().push(event);
                let index = self.semantic_calls.get();
                self.semantic_calls.set(index + 1);
                let refuse = self.reject_at == Some(index);
                let journal = self.journal.clone();
                let demand = self.local.endpoint.demand(move || {
                    assert!(journal.owner_live.get());
                    if borrows_default {
                        assert!(journal.default_borrow_live.get());
                    }
                    journal.children.set(journal.children.get() + 1);
                    let child = BodyChild {
                        journal,
                        borrows_default,
                    };
                    async move {
                        let _child = child;
                        if refuse {
                            Err(RunError::Refused(Incomplete::Allowance))
                        } else {
                            Ok(answer)
                        }
                    }
                })?;
                demand.await
            })
            .await
    }
}

impl StaticClassLocalEffects for SuspendedRecording<'_, '_, '_> {
    type Error = LocalMutationFailure;
    async fn next_map_entry<'a, K, V>(
        &self,
        fields: &'a FxIndexMap<K, V>,
        cursor: &mut usize,
    ) -> Result<Option<(&'a K, &'a V)>, Self::Error> {
        if *cursor < fields.len() {
            self.recording.events.borrow_mut().push(Event::Advance);
        }
        self.local.next_map_entry(fields, cursor).await
    }
    async fn remember_named_tuple_default<'db>(
        &self,
        previous: &mut PreviousNamedTupleDefault<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        self.recording
            .events
            .borrow_mut()
            .push(Event::Remember(name.clone(), None));
        self.local
            .remember_named_tuple_default(previous, name, declaration)
            .await
    }
    async fn append_copy<T: Copy>(&self, values: &mut Vec<T>, value: T) -> Result<(), Self::Error> {
        self.recording.events.borrow_mut().push(Event::Append);
        self.local.append_copy(values, value).await
    }
}

impl<'db> NamedTupleFieldEffects<'db> for SuspendedRecording<'_, '_, 'db> {
    async fn named_tuple_fields(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<&'db FxIndexMap<Name, Field<'db>>, Self::Error> {
        Ok(self
            .child(Event::Fields, self.recording.fields, false)
            .await)
    }
    async fn report_leading_underscore(
        &self,
        _class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        Ok(self
            .child(Event::Underscore(name.clone(), declaration), (), false)
            .await)
    }
    async fn report_required_after_default(
        &self,
        _class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
        previous: &(Name, Option<Definition<'db>>),
    ) -> Result<(), Self::Error> {
        assert!(!self.journal.default_borrow_live.replace(true));
        let retained = BorrowedDefault {
            previous,
            declaration: previous.1,
            journal: self.journal.clone(),
        };
        self.child(
            Event::Required(name.clone(), declaration, previous.0.clone(), previous.1),
            (),
            true,
        )
        .await;
        drop(retained);
        Ok(())
    }
}

impl<'db> ExplicitBaseKindEffects<'db> for SuspendedRecording<'_, '_, 'db> {
    async fn base_is_protocol(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(self
            .child(Event::Protocol(base), self.recording.protocol, false)
            .await)
    }
    async fn base_is_object(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(self
            .child(Event::Object(base), self.recording.object, false)
            .await)
    }
    async fn base_is_typed_dict(&self, base: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(self
            .child(Event::TypedDict(base), self.recording.typed_dict, false)
            .await)
    }
    async fn report_invalid_protocol_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        Ok(self
            .child(
                Event::InvalidProtocol {
                    class,
                    base,
                    source: source_node.range(),
                },
                (),
                false,
            )
            .await)
    }
    async fn report_invalid_typed_dict_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        Ok(self
            .child(
                Event::InvalidTypedDict {
                    class,
                    base,
                    source: source_node.range(),
                },
                (),
                false,
            )
            .await)
    }
}

struct BodyOwner<'db> {
    module: ParsedModuleRef,
    bases: Vec<ClassType<'db>>,
    expected: Vec<ClassType<'db>>,
    journal: Rc<BodyJournal>,
}

impl Drop for BodyOwner<'_> {
    fn drop(&mut self) {
        assert!(!self.module.suite().is_empty());
        assert_eq!(self.bases, self.expected);
        assert_eq!(self.journal.children.get(), 0);
        assert!(!self.journal.default_borrow_live.get());
        assert!(self.journal.owner_live.replace(false));
        self.journal.drops.borrow_mut().push("owner");
    }
}

struct AdmitBody;
impl ExecutionAdmission for AdmitBody {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

#[test]
fn authored_bodies_retain_their_locals_across_children_and_retry_after_refusal()
-> anyhow::Result<()> {
    let db = database()?;
    let fields_class = class(&db, "Fields")?;
    let subject = class(&db, "Header")?;
    let existing = ClassType::NonGeneric(ClassLiteral::Static(class(&db, "Plain")?));
    let base = specialized_base(&db)?;
    let fields = fields_class.own_fields(&db, None, CodeGeneratorKind::NamedTuple);
    let file = db.program_file(system_path_to_file(&db, "/src/classes.py")?);
    for named_tuple in [true, false] {
        for refuse in [true, false] {
            let module = parsed_module(&db, file.python_file(&db)).load(&db);
            let recording = Recording {
                typed_dict: true,
                ..Recording::new(fields)
            };
            let journal = Rc::new(BodyJournal::default());
            let root_journal = journal.clone();
            let recording_ref = &recording;
            let outcome = try_with_attempt(&db, 100_000, || {
                RegistryBuilder::new(&db, &AdmitBody)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let journal = root_journal;
                        let mut owner = BodyOwner {
                            module,
                            bases: vec![existing],
                            expected: if !named_tuple && !refuse {
                                vec![existing, base]
                            } else {
                                vec![existing]
                            },
                            journal: journal.clone(),
                        };
                        assert!(!journal.owner_live.replace(true));
                        let effects = SuspendedRecording {
                            local: Controlled {
                                endpoint: &endpoint,
                                allocation_failure: false,
                            },
                            recording: recording_ref,
                            journal: journal.clone(),
                            semantic_calls: Cell::new(0),
                            reject_at: refuse.then_some(if named_tuple { 2 } else { 1 }),
                        };
                        let result = if named_tuple {
                            ObservedBody {
                                future: Some(Box::pin(check_named_tuple_fields_with(
                                    fields_class,
                                    StaticClassFacts,
                                    &effects,
                                ))),
                                journal: journal.clone(),
                            }
                            .await
                        } else {
                            let node = owner
                                .module
                                .suite()
                                .iter()
                                .find_map(|statement| match statement {
                                    ast::Stmt::ClassDef(node) if node.name.as_str() == "Header" => {
                                        node.arguments.as_ref()?.args.get(2)
                                    }
                                    _ => None,
                                })
                                .ok_or(RunError::Contract("missing specialized base source"))?;
                            ObservedBody {
                                future: Some(Box::pin(check_explicit_base_kind_with(
                                    subject,
                                    base,
                                    node,
                                    false,
                                    Some(CodeGeneratorKind::TypedDict),
                                    &mut owner.bases,
                                    &effects,
                                ))),
                                journal: journal.clone(),
                            }
                            .await
                        };
                        journal.resumed.set(true);
                        result.map_err(|_| {
                            RunError::Contract("recorded body had a local mutation failure")
                        })
                    })
            });
            assert_eq!(
                matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
                !refuse,
                "{outcome:?}"
            );
            assert_eq!(journal.resumed.get(), !refuse);
            assert!(journal.pending.get() >= if named_tuple { 3 } else { 2 });
            let mut expected = if named_tuple {
                original_named_tuple_events(fields)
            } else {
                vec![
                    Event::TypedDict(base),
                    Event::TypedDict(base),
                    Event::Append,
                ]
            };
            if refuse {
                if named_tuple {
                    let end = expected
                        .iter()
                        .position(
                            |event| matches!(event, Event::Required(name, ..) if name == "_last"),
                        )
                        .ok_or_else(|| anyhow::anyhow!("missing required-field report"))?
                        + 1;
                    expected.truncate(end);
                } else {
                    expected.truncate(2);
                }
            }
            assert_eq!(*recording.events.borrow(), expected);
            let drops = journal.drops.borrow();
            assert_eq!(&drops[drops.len() - 2..], &["body", "owner"]);
            if named_tuple {
                let previous = drops
                    .iter()
                    .position(|event| *event == "previous")
                    .ok_or_else(|| {
                        anyhow::anyhow!("the authored previous-default borrow was not observed")
                    })?;
                assert!(previous > 0);
                assert_eq!(drops[previous - 1], "child");
            }
        }
    }
    Ok(())
}
