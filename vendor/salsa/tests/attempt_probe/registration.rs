//! Cross-crate lifetime controls for callback registration, before tracked fetch integration.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use salsa::attempt_probe::{AttemptOutcome, Incomplete, report_incomplete, try_with_attempt};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, ProviderContext, RegistryBuilder, Route, RouteProvider,
    RunError, RunResult, TaskEndpoint, VerifyResult,
};
use salsa::plumbing::function::{Configuration, IngredientImpl};
use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::{Database, DatabaseImpl, Id, Revision};

#[salsa::input]
struct Node {
    #[returns(copy)]
    next: Option<Node>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
struct Count(u32);

thread_local! {
    static ORDINARY_ENTRIES: Cell<usize> = const { Cell::new(0) };
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn scalar(_db: &dyn Database, _node: Node) -> u32 {
    ORDINARY_ENTRIES.set(ORDINARY_ENTRIES.get() + 1);
    999
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn wrapped(_db: &dyn Database, _node: Node) -> Count {
    ORDINARY_ENTRIES.set(ORDINARY_ENTRIES.get() + 1);
    Count(999)
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_only(_db: &dyn Database, _node: Node) -> u32 {
    ORDINARY_ENTRIES.set(ORDINARY_ENTRIES.get() + 1);
    999
}

trait ScriptOutput: Sized {
    const SCALAR: bool;
    fn new(value: u32) -> Self;
    fn value(self) -> u32;
}

impl ScriptOutput for u32 {
    const SCALAR: bool = true;

    fn new(value: u32) -> Self {
        value
    }

    fn value(self) -> u32 {
        self
    }
}

impl ScriptOutput for Count {
    const SCALAR: bool = false;

    fn new(value: u32) -> Self {
        Self(value)
    }

    fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkKind {
    Resource,
    Task,
    Poll,
    Work,
}

struct Admission {
    observations: RefCell<Vec<ExecutionWork>>,
    refuse: Cell<Option<(WorkKind, usize)>>,
    matching: Cell<usize>,
}

impl Admission {
    fn new() -> Self {
        Self {
            observations: RefCell::new(Vec::with_capacity(4096)),
            refuse: Cell::new(None),
            matching: Cell::new(0),
        }
    }

    fn refuse_at(&self, kind: WorkKind, ordinal: usize) {
        self.refuse.set(Some((kind, ordinal)));
        self.matching.set(0);
    }
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.observations.borrow_mut().push(work);
        let kind = match work {
            ExecutionWork::Task { .. } => WorkKind::Task,
            ExecutionWork::Resource { .. } => WorkKind::Resource,
            ExecutionWork::Poll => WorkKind::Poll,
            ExecutionWork::Work { .. } => WorkKind::Work,
        };
        if let Some((target, ordinal)) = self.refuse.get()
            && kind == target
        {
            let current = self.matching.get();
            self.matching.set(current + 1);
            if current == ordinal {
                return Err(RunError::Refused(Incomplete::Allowance));
            }
        }
        Ok(())
    }
}

struct Resources<'a> {
    label: &'a str,
    bodies: RefCell<Vec<bool>>,
    verified: RefCell<Vec<bool>>,
    resource_addresses: RefCell<Vec<usize>>,
    fail_body: Cell<Option<usize>>,
}

impl<'a> Resources<'a> {
    fn new(label: &'a str) -> Self {
        Self {
            label,
            bodies: RefCell::new(Vec::with_capacity(4096)),
            verified: RefCell::new(Vec::with_capacity(16)),
            resource_addresses: RefCell::new(Vec::with_capacity(4096)),
            fail_body: Cell::new(None),
        }
    }
}

struct Providers<'resources, 'db, A: Configuration, B: Configuration> {
    scalar: Route<'db, A>,
    wrapped: Route<'db, B>,
    resources: &'resources Resources<'resources>,
}

impl<'run, 'db: 'run, 'resources: 'run, A, B, C> RouteProvider<'run, 'db, C>
    for Providers<'resources, 'db, A, B>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node>,
    C: Configuration<DbView = dyn Database, Input<'db> = Node>,
    A::Output<'db>: ScriptOutput,
    B::Output<'db>: ScriptOutput,
    C::Output<'db>: ScriptOutput,
{
    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<C::Output<'db>> {
        context.endpoint().admit(ExecutionWork::Work { units: 1 })?;
        let ordinal = self.resources.bodies.borrow().len();
        if self.resources.fail_body.get() == Some(ordinal) {
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        assert_eq!(self.resources.label, "borrowed resources");
        self.resources
            .bodies
            .borrow_mut()
            .push(<C::Output<'db> as ScriptOutput>::SCALAR);
        self.resources
            .resource_addresses
            .borrow_mut()
            .push(std::ptr::from_ref(self.resources).addr());
        let child = match node.next(db) {
            None => 0,
            Some(next) if <C::Output<'db> as ScriptOutput>::SCALAR => {
                context.body_callback(&self.wrapped, next)?.await?.value()
            }
            Some(next) => context.body_callback(&self.scalar, next)?.await?.value(),
        };
        Ok(<C::Output<'db> as ScriptOutput>::new(child + 1))
    }

    async fn verify(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        id: Id,
        revision: Revision,
    ) -> RunResult<VerifyResult> {
        assert_eq!(revision, salsa::plumbing::current_revision(db));
        self.resources
            .verified
            .borrow_mut()
            .push(<C::Output<'db> as ScriptOutput>::SCALAR);
        let node = C::id_to_input(db.zalsa(), id);
        // This tests the erased factory's binding and lifetime, not memo validity.
        let value = if <C::Output<'db> as ScriptOutput>::SCALAR {
            context.body_callback(&self.scalar, node)?.await?.value()
        } else {
            context.body_callback(&self.wrapped, node)?.await?.value()
        };
        assert!(value > 0);
        Ok(VerifyResult::Unchanged {
            #[cfg(feature = "accumulator")]
            accumulated: Default::default(),
        })
    }
}

fn typed_and_erased<'db, A, B>(
    db: &'db dyn Database,
    scalar_ingredient: &'db IngredientImpl<A>,
    wrapped_ingredient: &'db IngredientImpl<B>,
    node: Node,
    resources: &Resources<'_>,
    admission: &Admission,
) -> RunResult<(u32, u32)>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node>,
    A::Output<'db>: ScriptOutput,
    B::Output<'db>: ScriptOutput,
{
    let mut registry = RegistryBuilder::new(db, admission)?;
    let scalar = registry.reserve(db, scalar_ingredient)?;
    let wrapped = registry.reserve(db, wrapped_ingredient)?;
    let providers = Providers {
        scalar: scalar.clone(),
        wrapped: wrapped.clone(),
        resources,
    };
    // Move the table after its borrowed provider so partial setup also drops in that order.
    let mut registry = registry;
    let binding = registry.provider(&providers)?;
    let other_binding = registry.provider(&providers)?;
    registry.bind(&scalar, &binding)?;
    registry.bind(&wrapped, &binding)?;
    assert!(matches!(
        registry.bind(&scalar, &binding),
        Err(RunError::Contract("query route already bound or missing"))
    ));
    let (foreign_route, foreign_binding) = {
        let mut other = RegistryBuilder::new(db, admission)?;
        (
            other.reserve(db, scalar_ingredient)?,
            other.provider(&providers)?,
        )
    };
    registry.seal()?.run(move |endpoint| async move {
        assert!(matches!(
            endpoint.provider(foreign_binding),
            Err(RunError::Contract("foreign provider binding"))
        ));
        let other_context = endpoint.provider(other_binding)?;
        assert!(matches!(
            other_context.body_callback(&scalar, node),
            Err(RunError::Contract(
                "query route has a different provider binding"
            ))
        ));
        let context = endpoint.provider(binding)?;
        assert!(matches!(
            context.body_callback(&foreign_route, node),
            Err(RunError::Contract("foreign query route"))
        ));
        let first = context.body_callback(&scalar, node)?.await?.value();
        let second = context.body_callback(&wrapped, node)?.await?.value();
        let revision = salsa::plumbing::current_revision(db);
        assert!(matches!(
            endpoint
                .verify_callback(scalar.database_key(node.as_id()), revision)?
                .await?,
            VerifyResult::Unchanged { .. }
        ));
        assert!(matches!(
            endpoint
                .verify_callback(wrapped.database_key(node.as_id()), revision)?
                .await?,
            VerifyResult::Unchanged { .. }
        ));
        Ok((first, second))
    })
}

#[test]
fn borrowed_provider_routes_share_typed_and_erased_dispatch() {
    let db = DatabaseImpl::default();
    let mut node = Node::new(&db, None);
    for _ in 1..128 {
        node = Node::new(&db, Some(node));
    }
    let label = String::from("borrowed resources");
    let resources = Resources::new(&label);
    let admission = Admission::new();
    let result = try_with_attempt(&db, 100_000, || {
        typed_and_erased(
            &db,
            scalar::fn_ingredient_(&db, db.zalsa()),
            wrapped::fn_ingredient_(&db, db.zalsa()),
            node,
            &resources,
            &admission,
        )
    });
    assert_eq!(result, Ok(AttemptOutcome::Complete(Ok((128, 128)))));
    assert_eq!(&*resources.verified.borrow(), &[true, false]);
    assert_eq!(resources.bodies.borrow().len(), 4 * 128);
    assert!(
        resources
            .bodies
            .borrow()
            .chunks_exact(128)
            .all(|chunk| { chunk.windows(2).all(|pair| pair[0] != pair[1]) })
    );
    assert!(
        resources
            .resource_addresses
            .borrow()
            .iter()
            .all(|address| { *address == std::ptr::from_ref(&resources).addr() })
    );
    ORDINARY_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
}

#[test]
fn refused_callback_has_no_success_and_resources_survive_retry() {
    let db = DatabaseImpl::default();
    let leaf = Node::new(&db, None);
    let node = Node::new(&db, Some(leaf));
    let label = String::from("borrowed resources");
    let resources = Resources::new(&label);
    let admission = Admission::new();
    resources.fail_body.set(Some(1));
    let run = || {
        typed_and_erased(
            &db,
            scalar::fn_ingredient_(&db, db.zalsa()),
            wrapped::fn_ingredient_(&db, db.zalsa()),
            node,
            &resources,
            &admission,
        )
    };
    assert_eq!(
        try_with_attempt(&db, 100, run),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(&*resources.bodies.borrow(), &[true]);
    resources.fail_body.set(None);
    assert_eq!(
        try_with_attempt(&db, 100, run),
        Ok(AttemptOutcome::Complete(Ok((2, 2))))
    );
    ORDINARY_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
}

#[test]
fn setup_and_task_admission_precede_factories_and_polling() {
    let db = DatabaseImpl::default();
    for kind in [WorkKind::Resource, WorkKind::Task, WorkKind::Poll] {
        let admission = Admission::new();
        admission.refuse_at(kind, 0);
        let factory_entered = Cell::new(false);
        let result = try_with_attempt(&db, 100, || {
            RegistryBuilder::new(&db, &admission)?.seal()?.run(|_| {
                factory_entered.set(true);
                async { Ok(()) }
            })
        });
        assert_eq!(
            result,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert!(!factory_entered.get());
    }
}

#[test]
fn unbound_duplicate_and_foreign_routes_fail_before_callbacks() {
    let db = DatabaseImpl::default();
    let other_db = DatabaseImpl::default();
    let admission = Admission::new();
    let result = try_with_attempt(&db, 100, || -> RunResult<()> {
        let scalar_ingredient = scalar::fn_ingredient_(&db, db.zalsa());
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let _route = registry.reserve(&db as &dyn Database, scalar_ingredient)?;
        assert!(matches!(
            registry.reserve(&db as &dyn Database, scalar_ingredient),
            Err(RunError::Contract("query route already reserved"))
        ));
        assert!(matches!(
            registry.reserve(
                &other_db as &dyn Database,
                scalar::fn_ingredient_(&other_db, other_db.zalsa())
            ),
            Err(RunError::Contract(
                "route has a foreign database or ingredient"
            ))
        ));
        assert!(matches!(
            registry.seal(),
            Err(RunError::Contract("query route remains unbound"))
        ));
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        assert!(matches!(
            registry.reserve(
                &db as &dyn Database,
                complete_only::fn_ingredient_(&db, db.zalsa())
            ),
            Err(RunError::Contract(
                "only return-only routes can be registered"
            ))
        ));
        let key = scalar_ingredient.database_key_index(Node::new(&db, None).as_id());
        let revision = salsa::plumbing::current_revision(&db);
        registry.seal()?.run(|endpoint| async move {
            assert!(matches!(
                endpoint.verify_callback(key, revision),
                Err(RunError::Contract("verification route is not registered"))
            ));
            Ok(())
        })?;
        Ok(())
    });
    assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(()))));
    ORDINARY_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
}

#[test]
fn endpoint_is_closed_after_the_only_driver_finishes() {
    let db = DatabaseImpl::default();
    let admission = Admission::new();
    let result = try_with_attempt(&db, 100, || {
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(|endpoint| async move { Ok(endpoint) })
    });
    let Ok(AttemptOutcome::Complete(Ok(endpoint))) = result else {
        panic!("registered driver completed");
    };
    assert!(matches!(
        endpoint.demand(|| async { Ok(()) }),
        Err(RunError::Contract("execution driver has ended"))
    ));
    assert!(matches!(
        endpoint.admit(ExecutionWork::Work { units: 1 }),
        Err(RunError::Contract("execution driver has ended"))
    ));
}

async fn nested_run<'run, 'db: 'run>(
    db: &'db dyn Database,
    admission: &'run Admission,
    endpoint: TaskEndpoint<'run, 'db>,
) -> RunResult<()> {
    let inner = RegistryBuilder::new(db, admission)?.seal()?;
    assert!(matches!(
        inner.run(|_| async { Ok(()) }),
        Err(RunError::Contract("nested execution driver"))
    ));
    endpoint.demand(|| async { Ok(()) })?.await
}

#[test]
fn nested_driver_rejection_leaves_the_original_endpoint_usable() {
    let db = DatabaseImpl::default();
    let admission = Admission::new();
    let result = try_with_attempt(&db, 100, || {
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(|endpoint| nested_run(&db, &admission, endpoint))
    });
    assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(()))));
}

#[test]
fn escaped_registry_keeps_its_original_attempt_owner() {
    let db = DatabaseImpl::default();
    let admission = Admission::new();
    for incomplete in [false, true] {
        let mut escaped = None;
        let first = try_with_attempt(&db, 100, || -> RunResult<()> {
            escaped = Some(RegistryBuilder::new(&db, &admission)?);
            if incomplete {
                report_incomplete(&db, Incomplete::Allowance);
            }
            Ok(())
        });
        assert_eq!(
            first,
            if incomplete {
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            } else {
                Ok(AttemptOutcome::Complete(Ok(())))
            }
        );
        let Some(mut escaped) = escaped else {
            panic!("registration was constructed");
        };
        let second = try_with_attempt(&db, 100, || -> RunResult<()> {
            assert!(matches!(
                escaped.provider(&()),
                Err(RunError::Contract("execution run has a foreign attempt"))
            ));
            assert!(matches!(
                escaped.reserve(
                    &db as &dyn Database,
                    scalar::fn_ingredient_(&db, db.zalsa())
                ),
                Err(RunError::Contract("execution run has a foreign attempt"))
            ));
            assert!(matches!(
                escaped.seal(),
                Err(RunError::Contract("execution run has a foreign attempt"))
            ));
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|_| async { Ok(()) })
        });
        assert_eq!(second, Ok(AttemptOutcome::Complete(Ok(()))));
    }
}

#[test]
fn sealed_registry_cannot_adopt_a_later_attempt() {
    let db = DatabaseImpl::default();
    let admission = Admission::new();
    let entered = Cell::new(false);
    let first = try_with_attempt(&db, 100, || RegistryBuilder::new(&db, &admission)?.seal());
    let Ok(AttemptOutcome::Complete(Ok(sealed))) = first else {
        panic!("registration was sealed");
    };
    let second = try_with_attempt(&db, 100, || {
        sealed.run(|_| {
            entered.set(true);
            async { Ok(()) }
        })
    });
    assert_eq!(
        second,
        Ok(AttemptOutcome::Complete(Err(RunError::Contract(
            "execution run has a foreign attempt"
        ))))
    );
    assert!(!entered.get());
}

#[test]
fn refused_registration_stops_all_further_setup() {
    let db = DatabaseImpl::default();
    let admission = Admission::new();
    let result = try_with_attempt(&db, 100, || -> RunResult<()> {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        admission.refuse_at(WorkKind::Resource, 0);
        assert!(matches!(
            registry.reserve(
                &db as &dyn Database,
                scalar::fn_ingredient_(&db, db.zalsa())
            ),
            Err(RunError::Refused(Incomplete::Allowance))
        ));
        let calls = admission.observations.borrow().len();
        admission.refuse.set(None);
        assert!(matches!(
            registry.provider(&()),
            Err(RunError::Refused(Incomplete::Allowance))
        ));
        assert!(matches!(
            registry.reserve(
                &db as &dyn Database,
                wrapped::fn_ingredient_(&db, db.zalsa())
            ),
            Err(RunError::Refused(Incomplete::Allowance))
        ));
        assert!(matches!(
            registry.seal(),
            Err(RunError::Refused(Incomplete::Allowance))
        ));
        assert_eq!(admission.observations.borrow().len(), calls);
        Ok(())
    });
    assert_eq!(
        result,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
}

#[test]
fn registration_cannot_change_its_enclosing_operation_scope() {
    let db = DatabaseImpl::default();
    let admission = Admission::new();
    let result = try_with_attempt(&db, 100, || -> RunResult<()> {
        let registry = RegistryBuilder::new(&db, &admission)?;
        let nested = salsa::attempt_probe::try_with_operation(&db, || registry.seal());
        assert!(matches!(
            nested,
            Ok(Err(RunError::Contract(
                "execution registration changed its enclosing scope"
            )))
        ));
        Ok(())
    });
    assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(()))));
}

#[test]
fn registration_rejects_replaced_scopes_at_the_same_depth() {
    for nested in [false, true] {
        let db = DatabaseImpl::default();
        let admission = Admission::new();
        let result = try_with_attempt(&db, 100, || {
            let check = || {
                let registry = salsa::attempt_probe::try_with_operation(&db, || {
                    RegistryBuilder::new(&db, &admission).expect("registration is admitted")
                })
                .expect("first scope is admitted");
                let replaced = salsa::attempt_probe::try_with_operation(&db, || registry.seal());
                assert!(matches!(
                    replaced,
                    Ok(Err(RunError::Contract(
                        "execution registration changed its enclosing scope"
                    )))
                ));

                let entered = Cell::new(false);
                let sealed = salsa::attempt_probe::try_with_operation(&db, || {
                    RegistryBuilder::new(&db, &admission)
                        .expect("registration is admitted")
                        .seal()
                        .expect("registration is sealed")
                })
                .expect("first scope is admitted");
                let replaced = salsa::attempt_probe::try_with_operation(&db, || {
                    sealed.run(|_| async {
                        entered.set(true);
                        Ok(())
                    })
                });
                assert!(matches!(
                    replaced,
                    Ok(Err(RunError::Contract(
                        "execution registration changed its enclosing scope"
                    )))
                ));
                assert!(!entered.get());
            };
            if nested {
                assert_eq!(salsa::attempt_probe::try_with_operation(&db, check), Ok(()));
            } else {
                check();
            }
        });
        assert_eq!(result, Ok(AttemptOutcome::Complete(())));
    }
}

#[test]
fn temporary_child_scopes_restore_the_registered_parent() {
    for unwind in [false, true] {
        let db = DatabaseImpl::default();
        let admission = Admission::new();
        let result = try_with_attempt(&db, 100, || {
            salsa::attempt_probe::try_with_operation(&db, || -> RunResult<u32> {
                let registry = RegistryBuilder::new(&db, &admission)?;
                let inner = catch_unwind(AssertUnwindSafe(|| {
                    salsa::attempt_probe::try_with_operation(&db, || {
                        assert!(!unwind, "temporary child panic");
                    })
                }));
                if unwind {
                    assert!(inner.is_err());
                } else {
                    assert!(matches!(inner, Ok(Ok(()))));
                }
                registry
                    .seal()?
                    .run(|endpoint| async move { endpoint.demand(|| async { Ok(7) })?.await })
            })
        });
        assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(Ok(7)))));
    }
}
