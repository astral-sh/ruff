//! Transport control for flat local continuations with real, uninferred call storage.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::{global_scope, semantic_index};

use super::{
    Bindings, CallArguments, InferenceRegion, Type, TypeContext, TypeInferenceBuilder, ast,
};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::call::CallableBinding;
use crate::types::signatures::effects::try_poll_immediate;
use crate::{Db, ProgramEnvironment};

const PATH: &str = "/src/frame.py";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Next,
    Prepare(usize),
    Push,
    Child,
    Resume,
    Retire(usize),
}

#[derive(Default)]
struct Journal {
    events: RefCell<Vec<Event>>,
    drops: RefCell<Vec<String>>,
    live_payloads: Cell<usize>,
    max_payloads: Cell<usize>,
    children: Cell<usize>,
    pending: Cell<usize>,
    pending_with_two_payloads: Cell<usize>,
    commits: Cell<usize>,
    resumed: Cell<bool>,
    module_live: Cell<bool>,
    slots_live: Cell<bool>,
}

struct Payload<'db, 'ast> {
    slot: usize,
    call: &'ast ast::ExprCall,
    arguments: CallArguments<'ast, 'db>,
    bindings: Bindings<'db>,
    argument_identity: *const (),
    journal: Rc<Journal>,
}
impl Payload<'_, '_> {
    fn check(&self) {
        assert_eq!(self.arguments.len(), self.call.arguments.len());
        assert_eq!(
            self.arguments
                .argument_types(0)
                .map(|types| std::ptr::from_ref(types).cast::<()>()),
            Some(self.argument_identity)
        );
        assert!(
            self.arguments
                .iter_types()
                .all(|types| types.get_default().is_none())
        );
        assert_eq!(
            self.bindings.callable_type(),
            Type::int_literal(self.slot as i64 + 1)
        );
        assert!(self.bindings.is_single());
        assert_eq!(self.bindings.iter_flat().count(), 1);
        assert!(
            self.bindings
                .iter_flat()
                .all(|binding| binding.overloads().is_empty())
        );
    }
}
impl Drop for Payload<'_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert!(self.journal.module_live.get());
        self.check();
        self.journal
            .live_payloads
            .set(self.journal.live_payloads.get() - 1);
        self.journal
            .drops
            .borrow_mut()
            .push(format!("payload{}", self.slot));
    }
}

enum Frame<'db, 'ast> {
    Enter(&'ast ast::ExprCall, usize),
    Resume(Payload<'db, 'ast>),
}

struct Slots<'db, 'ast> {
    builders: [TypeInferenceBuilder<'db, 'ast>; 2],
    cache_identity: *const (),
    journal: Rc<Journal>,
}
impl Slots<'_, '_> {
    fn check(&self) {
        assert!(
            self.builders
                .iter()
                .all(|builder| builder.expressions.is_empty())
        );
        let caches = self
            .builders
            .each_ref()
            .map(|builder| builder.expression_cache.as_ref());
        assert!(
            matches!(caches, [Some(root), Some(speculative)] if Rc::ptr_eq(root, speculative) && Rc::as_ptr(root).cast::<()>() == self.cache_identity && root.borrow().entries.is_empty())
        );
        assert!(std::ptr::eq(
            self.builders[0].module(),
            self.builders[1].module()
        ));
    }
}
impl Drop for Slots<'_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        self.check();
        assert!(self.journal.slots_live.replace(false));
        self.journal.drops.borrow_mut().push("slots".into());
    }
}

// This holds the actual selected builder and argument storage across the protected boundary.
struct Borrowed<'a, 'db, 'ast> {
    builder: &'a mut TypeInferenceBuilder<'db, 'ast>,
    payload: &'a Payload<'db, 'ast>,
}
impl Drop for Borrowed<'_, '_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.payload.journal.children.get(), 0);
        assert!(self.builder.expressions.is_empty());
        assert!(!self.builder.module().suite().is_empty());
        self.payload.check();
        self.payload
            .journal
            .drops
            .borrow_mut()
            .push(format!("borrow{}", self.payload.slot));
    }
}

struct Facts;
shared_semantic_family! {
    #[synchronous(SynchronousFrameEffects)]
    trait FrameEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        #[progress]
        async fn next(&self, frames: &mut Vec<Frame<'db, 'ast>>) -> Result<Option<Frame<'db, 'ast>>, Self::Error>;
        #[operation(local)]
        async fn prepare(&self, call: &'ast ast::ExprCall, slot: usize) -> Result<Payload<'db, 'ast>, Self::Error>;
        #[operation(local)]
        async fn push(&self, frames: &mut Vec<Frame<'db, 'ast>>, parent: Frame<'db, 'ast>, child: Frame<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn child(&self, slots: &mut Slots<'db, 'ast>, payload: &Payload<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn resume(&self, slots: &mut Slots<'db, 'ast>, payload: &Payload<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn retire(&self, payload: Payload<'db, 'ast>) -> Result<(), Self::Error>;
    }
    #[finite_capability]
    impl Facts {
        fn nested<'ast>(&self, call: &'ast ast::ExprCall) -> Option<&'ast ast::ExprCall> {
            match call.arguments.args.first()? {
                ast::Expr::Call(child) => Some(child),
                _ => None,
            }
        }
    }
    #[synchronous(drive_sync)]
    #[capabilities(effects = FrameEffects, facts = Facts)]
    #[passive_values(Frame::Resume, Frame::Enter)]
    async fn drive<'db, 'ast, E: FrameEffects<'db, 'ast>>(
        mut frames: Vec<Frame<'db, 'ast>>,
        mut slots: Slots<'db, 'ast>,
        facts: Facts,
        effects: &E,
    ) -> Result<(), E::Error> {
        #[cursor_loop]
        while let Some(frame) = effects.next(&mut frames).await? {
            match frame {
                Frame::Enter(call, slot) => {
                    let payload = effects.prepare(call, slot).await?;
                    match facts.nested(call) {
                        Some(child) => effects.push(&mut frames, Frame::Resume(payload), Frame::Enter(child, 1)).await?,
                        None => {
                            effects.child(&mut slots, &payload).await?;
                            effects.retire(payload).await?;
                        }
                    }
                }
                Frame::Resume(payload) => {
                    effects.resume(&mut slots, &payload).await?;
                    effects.retire(payload).await?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Refused(Event);

struct Effects<'call, 'run, 'db: 'run> {
    endpoint: Option<&'call TaskEndpoint<'run, 'db>>,
    admission: Option<&'call Admission<'run, 'db>>,
    journal: Rc<Journal>,
    refuse_at: Option<usize>,
    refuse_child: bool,
}
impl Effects<'_, '_, '_> {
    fn before_sync(&self, event: Event) -> Result<(), Refused> {
        let position = self.journal.events.borrow().len();
        self.journal.events.borrow_mut().push(event);
        if self.refuse_at == Some(position) {
            Err(Refused(event))
        } else {
            Ok(())
        }
    }
    async fn before(&self, event: Event) -> Result<(), Refused> {
        let Some(endpoint) = self.endpoint else {
            return self.before_sync(event);
        };
        if event == Event::Child {
            endpoint
                .child_call(|| async {
                    self.before_sync(event)
                        .map_err(|_| RunError::Refused(Incomplete::Allowance))?;
                    assert_eq!(self.journal.live_payloads.get(), 2);
                    let child = Child::new(self.journal.clone());
                    let refuse_child = self.refuse_child;
                    endpoint
                        .demand(move || async move {
                            let _child = child;
                            if refuse_child {
                                Err(RunError::Refused(Incomplete::Allowance))
                            } else {
                                Ok(Ok(()))
                            }
                        })?
                        .await
                })
                .await
        } else {
            endpoint
                .local_call(|| {
                    let result = self.before_sync(event);
                    if let Some(admission) = self.admission {
                        admission.active.set(true);
                    }
                    let admitted = endpoint.admit_work(1);
                    if let Some(admission) = self.admission {
                        admission.active.set(false);
                    }
                    admitted?;
                    endpoint.check_completion()?;
                    Ok(result)
                })
                .await
        }
    }
    fn committed(&self) {
        self.journal.commits.set(self.journal.commits.get() + 1);
    }
}

macro_rules! transport_effects {
    ($trait:ident, [$($async:tt)*], $before:ident, [$($await:tt)*]) => {
        impl<'db, 'ast, 'run> $trait<'db, 'ast> for Effects<'_, 'run, 'db> where 'db: 'run {
            type Error = Refused;
            $($async)* fn next(&self, frames: &mut Vec<Frame<'db, 'ast>>) -> Result<Option<Frame<'db, 'ast>>, Refused> {
                if frames.is_empty() { return Ok(None); }
                self.$before(Event::Next) $($await)*?;
                self.committed();
                Ok(frames.pop())
            }
            $($async)* fn prepare(&self, call: &'ast ast::ExprCall, slot: usize) -> Result<Payload<'db, 'ast>, Refused> {
                self.$before(Event::Prepare(slot)) $($await)*?;
                let mut splats = 0;
                let arguments = CallArguments::from_arguments(&call.arguments, |_, _| { splats += 1; Type::unknown() });
                assert_eq!(splats, 0);
                let Some(argument_identity) = arguments.argument_types(0).map(|types| std::ptr::from_ref(types).cast::<()>()) else { return Err(Refused(Event::Prepare(slot))); };
                let live = self.journal.live_payloads.get() + 1;
                self.journal.live_payloads.set(live);
                self.journal.max_payloads.set(self.journal.max_payloads.get().max(live));
                let payload = Payload { slot, call, arguments, bindings: CallableBinding::not_callable(Type::int_literal(slot as i64 + 1)).into(), argument_identity, journal: self.journal.clone() };
                payload.check();
                self.committed();
                Ok(payload)
            }
            $($async)* fn push(&self, frames: &mut Vec<Frame<'db, 'ast>>, parent: Frame<'db, 'ast>, child: Frame<'db, 'ast>) -> Result<(), Refused> {
                self.$before(Event::Push) $($await)*?;
                frames.push(parent);
                frames.push(child);
                self.committed();
                Ok(())
            }
            $($async)* fn child(&self, slots: &mut Slots<'db, 'ast>, payload: &Payload<'db, 'ast>) -> Result<(), Refused> {
                slots.check();
                let _borrowed = Borrowed { builder: &mut slots.builders[payload.slot], payload };
                self.$before(Event::Child) $($await)*
            }
            $($async)* fn resume(&self, slots: &mut Slots<'db, 'ast>, payload: &Payload<'db, 'ast>) -> Result<(), Refused> {
                slots.check();
                let _borrowed = Borrowed { builder: &mut slots.builders[payload.slot], payload };
                self.$before(Event::Resume) $($await)*?;
                self.committed();
                Ok(())
            }
            $($async)* fn retire(&self, payload: Payload<'db, 'ast>) -> Result<(), Refused> {
                self.$before(Event::Retire(payload.slot)) $($await)*?;
                payload.check();
                self.committed();
                Ok(())
            }
        }
    };
}
transport_effects!(SynchronousFrameEffects, [], before_sync, []);
transport_effects!(FrameEffects, [async], before, [.await]);

struct Child(Rc<Journal>);
impl Child {
    fn new(journal: Rc<Journal>) -> Self {
        assert!(journal.module_live.get() && journal.slots_live.get());
        journal.children.set(journal.children.get() + 1);
        Self(journal)
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        assert!(self.0.module_live.get() && self.0.slots_live.get());
        self.0.children.set(self.0.children.get() - 1);
        self.0.drops.borrow_mut().push("child".into());
    }
}
struct ModuleOwner {
    module: ParsedModuleRef,
    journal: Rc<Journal>,
}
impl Drop for ModuleOwner {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert_eq!(self.journal.live_payloads.get(), 0);
        assert!(!self.journal.slots_live.get());
        assert!(!self.module.suite().is_empty());
        assert!(self.journal.module_live.replace(false));
        self.journal.drops.borrow_mut().push("module".into());
    }
}
struct Observed<F> {
    future: Option<Pin<Box<F>>>,
    journal: Rc<Journal>,
}
impl<F: Future> Future for Observed<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(future) = this.future.as_mut() else {
            return Poll::Pending;
        };
        let result = future.as_mut().poll(cx);
        if result.is_pending() {
            this.journal.pending.set(this.journal.pending.get() + 1);
            if this.journal.children.get() > 0 && this.journal.live_payloads.get() == 2 {
                assert!(this.journal.module_live.get() && this.journal.slots_live.get());
                this.journal
                    .pending_with_two_payloads
                    .set(this.journal.pending_with_two_payloads.get() + 1);
            }
        }
        result
    }
}
impl<F> Drop for Observed<F> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        drop(self.future.take());
        self.journal.drops.borrow_mut().push("body".into());
    }
}

fn storage<'db, 'ast>(
    db: &'db TestDb,
    owner: &'ast ModuleOwner,
    env: &'ast ProgramEnvironment<'db>,
) -> anyhow::Result<(Vec<Frame<'db, 'ast>>, Slots<'db, 'ast>)> {
    let file = db.program_file(system_path_to_file(db, PATH)?);
    let scope = global_scope(db, file);
    let Some(ast::Stmt::Expr(statement)) = owner.module.suite().first() else {
        anyhow::bail!("missing expression");
    };
    let ast::Expr::Call(call) = &*statement.value else {
        anyhow::bail!("missing outer call");
    };
    let mut root = TypeInferenceBuilder::new(
        db,
        env,
        InferenceRegion::Scope(scope, TypeContext::default()),
        file.file(db),
        file,
        semantic_index(db, file),
        &owner.module,
    );
    root.context.defuse();
    root.setup_expression_cache();
    let cache_identity = root
        .expression_cache
        .as_ref()
        .map(|cache| Rc::as_ptr(cache).cast::<()>())
        .ok_or_else(|| anyhow::anyhow!("missing expression cache"))?;
    let speculative = root.speculate();
    owner.journal.slots_live.set(true);
    Ok((
        vec![Frame::Enter(call, 0)],
        Slots {
            builders: [root, speculative],
            cache_identity,
            journal: owner.journal.clone(),
        },
    ))
}
fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(PATH, "outer(inner(1))\n")
        .build()
}
fn module(db: &TestDb, journal: Rc<Journal>) -> anyhow::Result<ModuleOwner> {
    let file = db.program_file(system_path_to_file(db, PATH)?);
    journal.module_live.set(true);
    Ok(ModuleOwner {
        module: parsed_module(db, file.python_file(db)).load(db),
        journal,
    })
}
fn expected() -> Vec<Event> {
    vec![
        Event::Next,
        Event::Prepare(0),
        Event::Push,
        Event::Next,
        Event::Prepare(1),
        Event::Child,
        Event::Retire(1),
        Event::Next,
        Event::Resume,
        Event::Retire(0),
    ]
}

#[test]
fn shared_frames_preserve_moves_and_typed_refusal_prefixes() -> anyhow::Result<()> {
    let db = database()?;
    let file = db.program_file(system_path_to_file(&db, PATH)?);
    let env = ProgramEnvironment::from_file(file);
    for asynchronous in [false, true] {
        for refuse_at in (0..expected().len()).map(Some).chain([None]) {
            let journal = Rc::new(Journal::default());
            let owner = module(&db, journal.clone())?;
            let (frames, slots) = storage(&db, &owner, &env)?;
            let effects = Effects {
                endpoint: None,
                admission: None,
                journal: journal.clone(),
                refuse_at,
                refuse_child: false,
            };
            let result = if asynchronous {
                try_poll_immediate(drive(frames, slots, Facts, &effects))
            } else {
                Poll::Ready(drive_sync(frames, slots, Facts, &effects))
            };
            assert_eq!(
                result,
                Poll::Ready(refuse_at.map_or(Ok(()), |at| Err(Refused(expected()[at]))))
            );
            assert_eq!(
                *journal.events.borrow(),
                expected()[..refuse_at.map_or(expected().len(), |at| at + 1)]
            );
            if refuse_at.is_none() {
                assert_eq!(journal.max_payloads.get(), 2);
            }
            drop(owner);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    Refuse(usize),
    QueueChild(usize),
    Child,
}
struct Admission<'run, 'db: 'run> {
    endpoint: RefCell<Option<TaskEndpoint<'run, 'db>>>,
    journal: Rc<Journal>,
    active: Cell<bool>,
    calls: Cell<usize>,
    fault: Fault,
}
impl ExecutionAdmission for Admission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !self.active.replace(false) || !matches!(work, ExecutionWork::Work { .. }) {
            return Ok(());
        }
        let position = self.calls.get();
        self.calls.set(position + 1);
        match self.fault {
            Fault::Refuse(at) if at == position => Err(RunError::Refused(Incomplete::Allowance)),
            Fault::QueueChild(at) if at == position => {
                let endpoint = self.endpoint.borrow();
                let endpoint = endpoint
                    .as_ref()
                    .ok_or(RunError::Contract("missing observer endpoint"))?;
                let child = Child::new(self.journal.clone());
                let _demand = endpoint.demand(move || async move {
                    let _child = child;
                    Ok(())
                })?;
                Ok(())
            }
            _ => Ok(()),
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

#[test]
fn real_child_and_every_local_refusal_keep_frames_until_children_retire() -> anyhow::Result<()> {
    // The runtime's admission observer needs static fixture storage. ClearEndpoint removes its
    // retained endpoint; these controls do not measure database or observer reclamation.
    let db: &'static TestDb = Box::leak(Box::new(database()?));
    let positions: Vec<_> = expected()
        .iter()
        .enumerate()
        .filter_map(|(at, event)| (*event != Event::Child).then_some(at))
        .collect();
    let faults = [Fault::None, Fault::Child]
        .into_iter()
        .chain((0..positions.len()).flat_map(|at| [Fault::Refuse(at), Fault::QueueChild(at)]));
    for fault in faults {
        let journal = Rc::new(Journal::default());
        let admission: &'static Admission<'static, 'static> = Box::leak(Box::new(Admission {
            endpoint: RefCell::new(None),
            journal: journal.clone(),
            active: Cell::new(false),
            calls: Cell::new(0),
            fault,
        }));
        let clear = ClearEndpoint(admission);
        let root_journal = journal.clone();
        let owner = module(db, root_journal.clone())?;
        let file = db.program_file(system_path_to_file(db, PATH)?);
        let _ = semantic_index(db, file);
        let _ = global_scope(db, file);
        let outcome = try_with_attempt(db, 100_000, || {
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(move |endpoint| async move {
                    *admission.endpoint.borrow_mut() = Some(endpoint.clone());
                    let env = ProgramEnvironment::from_file(file);
                    let (frames, slots) = storage(db, &owner, &env)
                        .map_err(|_| RunError::Contract("fixture storage"))?;
                    let effects = Effects {
                        endpoint: Some(&endpoint),
                        admission: Some(admission),
                        journal: root_journal.clone(),
                        refuse_at: None,
                        refuse_child: matches!(fault, Fault::Child),
                    };
                    let result = Observed {
                        future: Some(Box::pin(drive(frames, slots, Facts, &effects))),
                        journal: root_journal.clone(),
                    }
                    .await;
                    root_journal.resumed.set(true);
                    result.map_err(|_| RunError::Contract("unexpected transport refusal"))
                })
        });
        assert!(journal.pending.get() > 0, "{fault:?}");
        assert_eq!(journal.children.get(), 0);
        assert_eq!(journal.live_payloads.get(), 0);
        assert_eq!(journal.resumed.get(), matches!(fault, Fault::None));
        if matches!(fault, Fault::None | Fault::Child) {
            assert!(journal.pending_with_two_payloads.get() > 0);
        }
        let trace_end = match fault {
            Fault::None => {
                assert!(
                    matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
                    "{outcome:?}"
                );
                assert_eq!(journal.max_payloads.get(), 2);
                expected().len()
            }
            Fault::Child => {
                assert!(
                    matches!(
                        outcome,
                        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                    ),
                    "{outcome:?}"
                );
                6
            }
            Fault::Refuse(at) | Fault::QueueChild(at) => {
                match fault {
                    Fault::Refuse(_) => assert!(
                        matches!(
                            outcome,
                            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                        ),
                        "{outcome:?}"
                    ),
                    Fault::QueueChild(_) => assert!(
                        matches!(
                            outcome,
                            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                        ),
                        "{outcome:?}"
                    ),
                    _ => {}
                }
                assert_eq!(admission.calls.get(), at + 1);
                assert_eq!(journal.commits.get(), at);
                positions[at] + 1
            }
        };
        assert_eq!(*journal.events.borrow(), expected()[..trace_end]);
        assert_eq!(
            &journal.drops.borrow()[journal.drops.borrow().len() - 2..],
            &["body", "module"]
        );
        drop(clear);
        assert!(admission.endpoint.borrow().is_none());
    }
    Ok(())
}
