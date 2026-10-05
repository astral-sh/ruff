//! Scratch integration of real conversion continuations with typed graph demands.
//!
//! This deliberately refuses effectful operations whose query contract has not migrated.

mod mapping;
mod member_lookup;
mod mro;
mod payload;
mod source;
mod tasks_tests;

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::definition::Definition;

pub(crate) use super::conversion::ConversionTransform;
use super::conversion::{self, ConversionStep};
use super::{CallableConversionRequest, CallableType, CallableTypes, UpcastPolicy};
use crate::types::call::CallError;
use crate::types::call::bind::Bindings;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::constructor::callable::constructor_callables_with;
use crate::types::constructor::effects::ConstructorCallableRequest;
use crate::types::constructor::scheduled_effects::{ConstructorEffect, QueuedConstructorEffects};
use crate::types::descriptor::scheduled_effects::{DescriptorEffect, QueuedDescriptorEffects};
use crate::types::descriptor::{
    self, DescriptorInvocationRequest, DescriptorRequest, DescriptorResult,
};
use crate::types::generics::GenericContext;
use crate::types::infer::type_parameter_header::TypeParameterHeader;
use crate::types::infer::{
    DefinitionInference, SourceDefinitionEffect, evaluate_scheduled_definition,
};
use crate::types::instance::effects::InstanceEffect;
use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
use crate::types::relation::scheduled_requests::{
    self, RelationKey, RelationOutput, RelationRequest,
};
use crate::types::relation_error::FrozenErrorContextTree;
use crate::types::signatures::CallableSignature;
use crate::types::{
    InternedType, KnownBoundMethodType, KnownInstanceType, Parameter, Parameters, Signature,
    SubclassOfInner, Type, UnionType,
};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

use mapping::{MappingAnswer, MappingRequest, SemanticWork};
use mro::task::{self as mro_task, MroNodeId, StaticMroOutcome, StaticMroRequest};

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub(crate) enum Boundary {
    SourceSignature,
    SourcePreparation,
    SourceSupport,
    SourceDefinition(SourceDefinitionEffect),
    CostOverflow,
    SemanticOperation,
    ConstraintDomain,
    ProgramDomain,
    RootReuse,
    RelationContext,
    SignatureEffect(crate::types::signatures::effects::SignatureEffect),
    DiagnosticSeed,
    DiagnosticConflict,
    ConstructorEffect(ConstructorEffect),
    DescriptorEffect(DescriptorEffect),
    InstanceEffect(InstanceEffect),
    InvocationPreparation,
    InvocationIdentity,
    MappingOperation(crate::types::mapping::effects::MappingOperation),
    MappingDomain,
    MroDomain,
}

type Answer<'db> = Result<Option<CallableTypes<'db>>, Boundary>;

/// A closed composition is observable before its alternatives have completed conversion.
#[derive(Clone)]
pub(crate) enum ConversionPlan<'db> {
    Alternatives(Rc<[CallableConversionRequest<'db>]>),
    Transform {
        input: CallableConversionRequest<'db>,
        transformation: ConversionTransform<'db>,
    },
    Complete(Answer<'db>),
}

type RelationAnswer<'db, 'c> = Result<RelationOutput<'db, 'c>, Boundary>;
type ConstructorAnswer<'db> = Result<CallableTypes<'db>, Boundary>;
type DescriptorAnswer<'db> = Result<DescriptorResult<'db>, Boundary>;
// Call errors already box their bindings; boxing successful bindings also keeps task messages small.
type InvocationAnswer<'db> = Result<Result<Box<Bindings<'db>>, CallError<'db>>, Boundary>;
type InvocationReply<'db> = Rc<RefCell<Option<InvocationAnswer<'db>>>>;
type HeaderAnswer<'db> = Result<source::SourceFact<'db, TypeParameterHeader<'db>>, Boundary>;
type GenericContextAnswer<'db> = Result<source::SourceFact<'db, GenericContext<'db>>, Boundary>;
type DefinitionAnswer<'db> = Result<Arc<DefinitionInference<'db>>, Boundary>;
type Task<'eval, 'db, 'c, R> = Pin<Box<dyn Future<Output = Output<'db, 'c, R>> + 'eval>>;

struct ScheduledTask<'eval, 'db, 'c, R> {
    future: Task<'eval, 'db, 'c, R>,
    mro_owner: Option<MroNodeId>,
}

impl<'eval, 'db, 'c, R> ScheduledTask<'eval, 'db, 'c, R> {
    fn ordinary(future: Task<'eval, 'db, 'c, R>) -> Self {
        Self {
            future,
            mro_owner: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DiagnosticSeedId {
    domain: usize,
    index: usize,
}

struct ActiveConsumer<'a>(&'a Cell<bool>);

impl Drop for ActiveConsumer<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct DriverTasks<'eval, 'db, 'c, R> {
    router: &'eval Router<'db, 'c>,
    tasks: FxHashMap<Key<'db>, ScheduledTask<'eval, 'db, 'c, R>>,
}

impl<'eval, 'db, 'c, R> Deref for DriverTasks<'eval, 'db, 'c, R> {
    type Target = FxHashMap<Key<'db>, ScheduledTask<'eval, 'db, 'c, R>>;

    fn deref(&self) -> &Self::Target {
        &self.tasks
    }
}

impl<R> DerefMut for DriverTasks<'_, '_, '_, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tasks
    }
}

impl<R> Drop for DriverTasks<'_, '_, '_, R> {
    fn drop(&mut self) {
        self.router.driver_live.set(false);
        self.tasks.clear();
        self.router.static_mro.borrow_mut().finish_driver();
    }
}

struct EvaluationDomain(Option<usize>);

impl Default for EvaluationDomain {
    fn default() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .ok(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Key<'db> {
    Consumer,
    Conversion(CallableConversionRequest<'db>),
    Relation(RelationKey<'db>),
    Constructor(ConstructorCallableRequest<'db>),
    Descriptor(DescriptorRequest<'db>),
    Invocation(InvocationKey<'db>),
    Header(Definition<'db>),
    GenericContext(Definition<'db>),
    Definition(Definition<'db>),
    SourceWork(SourceWork<'db>),
    Mapping(MappingRequest<'db>),
    StaticMro(StaticMroRequest<'db>),
    SemanticWork(SemanticWork<'db>),
}

/// A definition's next local operation is admitted by the same scheduler as its dependencies.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SourceWork<'db> {
    owner: Definition<'db>,
    sequence: usize,
    units: usize,
}

/// Invocation-local inference state belongs to one owner and one call in its source order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct InvocationKey<'db> {
    owner: DescriptorRequest<'db>,
    sequence: usize,
    request: DescriptorInvocationRequest<'db>,
}

struct InvocationTicket<'db> {
    key: InvocationKey<'db>,
    reply: InvocationReply<'db>,
}

enum Output<'db, 'c, R> {
    Consumer(R),
    Conversion(Answer<'db>),
    Relation(RelationAnswer<'db, 'c>),
    Constructor(ConstructorAnswer<'db>),
    Descriptor(DescriptorAnswer<'db>),
    Header(HeaderAnswer<'db>),
    GenericContext(GenericContextAnswer<'db>),
    Definition(DefinitionAnswer<'db>),
    SourceWork,
    Mapping(MappingAnswer<'db>),
    StaticMro(StaticMroOutcome<'db>),
    SemanticWork,
    Invocation {
        reply: InvocationReply<'db>,
        answer: InvocationAnswer<'db>,
    },
}

struct Entry<'db, A> {
    answer: Option<A>,
    dependents: FxIndexSet<Key<'db>>,
}

impl<A> Default for Entry<'_, A> {
    fn default() -> Self {
        Self {
            answer: None,
            dependents: FxIndexSet::default(),
        }
    }
}

/// A family's mutations remain private until all ready tasks have read the same table snapshot.
struct Batch<'db, Q, K, A> {
    declarations: FxIndexMap<K, Q>,
    demands: FxIndexSet<(Key<'db>, K)>,
    completed: FxIndexMap<K, A>,
}

impl<Q, K, A> Default for Batch<'_, Q, K, A> {
    fn default() -> Self {
        Self {
            declarations: FxIndexMap::default(),
            demands: FxIndexSet::default(),
            completed: FxIndexMap::default(),
        }
    }
}

enum BatchTask<Q, K> {
    Declare { key: K, request: Q },
    Complete(K),
}

impl<'db, Q, K: Copy + Eq + Hash, A> Batch<'db, Q, K, A> {
    fn declare(&mut self, key: K, request: Q) {
        self.declarations.insert(key, request);
    }

    fn demand(&mut self, parent: Key<'db>, key: K, request: Q) {
        self.declare(key, request);
        self.demands.insert((parent, key));
    }

    fn complete(&mut self, key: K, answer: A) {
        self.completed.insert(key, answer);
    }

    fn stage_wakeups(
        &self,
        entries: &FxHashMap<K, Entry<'db, A>>,
        task_key: impl Fn(K) -> Key<'db>,
        next: &mut FxIndexSet<Key<'db>>,
    ) {
        for key in self.completed.keys() {
            next.extend(entries[key].dependents.iter().copied());
        }
        for key in self.declarations.keys() {
            if !entries.contains_key(key) {
                next.insert(task_key(*key));
            }
        }
        for (parent, child) in &self.demands {
            if entries
                .get(child)
                .is_some_and(|entry| entry.answer.is_some())
                || self.completed.contains_key(child)
            {
                next.insert(*parent);
            }
        }
    }

    fn commit(
        self,
        entries: &mut FxHashMap<K, Entry<'db, A>>,
        mut task: impl FnMut(BatchTask<Q, K>),
    ) {
        for (key, request) in self.declarations {
            if entries.entry(key).or_default().answer.is_none() {
                task(BatchTask::Declare { key, request });
            }
        }
        for (parent, child) in self.demands {
            entries.entry(child).or_default().dependents.insert(parent);
        }
        for (key, answer) in self.completed {
            entries.entry(key).or_default().answer = Some(answer);
            task(BatchTask::Complete(key));
        }
    }
}

enum Effect<'db, 'c> {
    DemandConversionPlan {
        parent: Key<'db>,
        child: CallableConversionRequest<'db>,
    },
    ConsumeConversion {
        parent: Key<'db>,
        child: CallableConversionRequest<'db>,
    },
    PublishConversionPlan {
        request: CallableConversionRequest<'db>,
        plan: ConversionPlan<'db>,
    },
    PublishRelationComposition {
        request: RelationKey<'db>,
        alternatives: Rc<[RelationRequest<'db, 'c>]>,
    },
    Declare(CallableConversionRequest<'db>),
    Demand {
        parent: Key<'db>,
        child: CallableConversionRequest<'db>,
    },
    Complete {
        request: CallableConversionRequest<'db>,
        answer: Answer<'db>,
    },
    DeclareRelation(RelationRequest<'db, 'c>),
    DemandRelation {
        parent: Key<'db>,
        child: RelationRequest<'db, 'c>,
    },
    CompleteRelation {
        request: RelationKey<'db>,
        answer: RelationAnswer<'db, 'c>,
    },
    DemandConstructor {
        parent: Key<'db>,
        child: ConstructorCallableRequest<'db>,
    },
    CompleteConstructor {
        request: ConstructorCallableRequest<'db>,
        answer: ConstructorAnswer<'db>,
    },
    DeclareDescriptor(DescriptorRequest<'db>),
    DemandDescriptor {
        parent: Key<'db>,
        child: DescriptorRequest<'db>,
    },
    CompleteDescriptor {
        request: DescriptorRequest<'db>,
        answer: DescriptorAnswer<'db>,
    },
    DemandInvocation {
        parent: Key<'db>,
        ticket: InvocationTicket<'db>,
    },
    CompleteInvocation {
        key: InvocationKey<'db>,
        reply: InvocationReply<'db>,
        answer: InvocationAnswer<'db>,
    },
    DeclareHeader(Definition<'db>),
    DemandHeader {
        parent: Key<'db>,
        child: Definition<'db>,
    },
    CompleteHeader {
        request: Definition<'db>,
        answer: HeaderAnswer<'db>,
    },
    DemandGenericContext {
        parent: Key<'db>,
        child: Definition<'db>,
    },
    CompleteGenericContext {
        request: Definition<'db>,
        answer: GenericContextAnswer<'db>,
    },
    DemandDefinition {
        parent: Key<'db>,
        child: Definition<'db>,
    },
    CompleteDefinition {
        request: Definition<'db>,
        answer: DefinitionAnswer<'db>,
    },
    DemandSourceWork(SourceWork<'db>),
    CompleteSourceWork(SourceWork<'db>),
    DemandMapping {
        parent: Key<'db>,
        child: MappingRequest<'db>,
    },
    CompleteMapping {
        request: MappingRequest<'db>,
        answer: MappingAnswer<'db>,
    },
    DemandSemanticWork(SemanticWork<'db>),
    CompleteSemanticWork(SemanticWork<'db>),
    DemandStaticMro(mro_task::Demand<'db>),
    CompleteStaticMro(mro_task::Completion<'db>),
}

#[derive(Default)]
pub(crate) struct Router<'db, 'c> {
    driven: Cell<bool>,
    driver_live: Cell<bool>,
    consumer_active: Cell<bool>,
    consumer_work_sequence: Cell<usize>,
    logical_work: Cell<usize>,
    cancellation_probe: RefCell<Option<(usize, salsa::CancellationToken)>>,
    entries: RefCell<FxHashMap<CallableConversionRequest<'db>, Entry<'db, Answer<'db>>>>,
    conversion_plans: RefCell<FxHashMap<CallableConversionRequest<'db>, ConversionPlan<'db>>>,
    conversion_plan_dependents:
        RefCell<FxHashMap<CallableConversionRequest<'db>, FxIndexSet<Key<'db>>>>,
    conversion_inputs:
        RefCell<FxHashMap<Key<'db>, FxHashMap<CallableConversionRequest<'db>, usize>>>,
    relations: RefCell<FxHashMap<RelationKey<'db>, Entry<'db, RelationAnswer<'db, 'c>>>>,
    relation_compositions: RefCell<FxHashMap<RelationKey<'db>, Rc<[RelationRequest<'db, 'c>]>>>,
    constructors:
        RefCell<FxHashMap<ConstructorCallableRequest<'db>, Entry<'db, ConstructorAnswer<'db>>>>,
    descriptors: RefCell<FxHashMap<DescriptorRequest<'db>, Entry<'db, DescriptorAnswer<'db>>>>,
    invocations: RefCell<FxHashMap<InvocationKey<'db>, Entry<'db, ()>>>,
    headers: RefCell<FxHashMap<Definition<'db>, Entry<'db, HeaderAnswer<'db>>>>,
    generic_contexts: RefCell<FxHashMap<Definition<'db>, Entry<'db, GenericContextAnswer<'db>>>>,
    definitions: RefCell<FxHashMap<Definition<'db>, Entry<'db, DefinitionAnswer<'db>>>>,
    source_work: RefCell<FxHashMap<SourceWork<'db>, Entry<'db, ()>>>,
    invocation_sequences: RefCell<FxHashMap<DescriptorRequest<'db>, usize>>,
    constraints: Option<&'c ConstraintSetBuilder<'db>>,
    effects: RefCell<Vec<Effect<'db, 'c>>>,
    diagnostic_seeds: RefCell<Vec<FrozenErrorContextTree<'db>>>,
    diagnostic_seed_ids: RefCell<FxHashMap<FrozenErrorContextTree<'db>, DiagnosticSeedId>>,
    evaluation_domain: EvaluationDomain,
    prepared_sources: source::PreparedSources<'db>,
    declarations: Option<Rc<source::PreparedDeclarations<'db>>>,
    mapping_sequence: Cell<usize>,
    mappings: RefCell<FxHashMap<MappingRequest<'db>, Entry<'db, MappingAnswer<'db>>>>,
    semantic_work: RefCell<FxHashMap<SemanticWork<'db>, Entry<'db, ()>>>,
    static_mro: RefCell<mro_task::State<'db>>,
    mro_effect_count: Cell<usize>,
}

impl<'db, 'c> Router<'db, 'c> {
    pub(crate) fn cancel_at(&self, work: usize, token: salsa::CancellationToken) {
        *self.cancellation_probe.borrow_mut() = Some((work, token));
    }

    pub(crate) fn intern_diagnostic_seed(
        &self,
        seed: FrozenErrorContextTree<'db>,
    ) -> Result<DiagnosticSeedId, Boundary> {
        let domain = self.evaluation_domain.0.ok_or(Boundary::DiagnosticSeed)?;
        let mut ids = self.diagnostic_seed_ids.borrow_mut();
        if let Some(id) = ids.get(&seed) {
            return Ok(*id);
        }
        let mut seeds = self.diagnostic_seeds.borrow_mut();
        let id = DiagnosticSeedId {
            domain,
            index: seeds.len(),
        };
        seeds.push(seed.clone());
        ids.insert(seed, id);
        Ok(id)
    }

    pub(crate) fn diagnostic_seed(
        &self,
        id: DiagnosticSeedId,
    ) -> Result<FrozenErrorContextTree<'db>, Boundary> {
        if self.evaluation_domain.0 != Some(id.domain) {
            return Err(Boundary::DiagnosticSeed);
        }
        self.diagnostic_seeds
            .borrow()
            .get(id.index)
            .cloned()
            .ok_or(Boundary::DiagnosticSeed)
    }

    pub(crate) fn with_constraints(constraints: &'c ConstraintSetBuilder<'db>) -> Self {
        Self {
            constraints: Some(constraints),
            ..Self::default()
        }
    }

    fn with_sources(prepared_sources: source::PreparedSources<'db>) -> Self {
        Self {
            prepared_sources,
            ..Self::default()
        }
    }

    fn with_declarations(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declarations: Rc<source::PreparedDeclarations<'db>>,
    ) -> Result<Self, Boundary> {
        let router = Self {
            declarations: Some(declarations),
            ..Self::default()
        };
        router.validate_declarations(db, env)?;
        Ok(router)
    }

    fn validate_declarations(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<(), Boundary> {
        if let Some(declarations) = &self.declarations
            && !declarations.matches(db, env)
        {
            return Err(Boundary::SourceSupport);
        }
        Ok(())
    }

    fn declare_header(&self, child: Definition<'db>) {
        self.effects.borrow_mut().push(Effect::DeclareHeader(child));
    }

    fn consumer_definition_demand(
        &self,
        child: Definition<'db>,
    ) -> impl Future<Output = DefinitionAnswer<'db>> + '_ {
        self.definition_demand_from(Key::Consumer, child)
    }

    pub(crate) async fn definition_demand(
        &self,
        parent: Definition<'db>,
        child: Definition<'db>,
    ) -> DefinitionAnswer<'db> {
        if self
            .definitions
            .borrow()
            .get(&parent)
            .is_none_or(|entry| entry.answer.is_some())
        {
            return Err(Boundary::SourceSupport);
        }
        self.definition_demand_from(Key::Definition(parent), child)
            .await
    }

    fn definition_demand_from(
        &self,
        parent: Key<'db>,
        child: Definition<'db>,
    ) -> impl Future<Output = DefinitionAnswer<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandDefinition { parent, child });
                registered = true;
                return Poll::Pending;
            }
            // Only a completed producer can populate this table. The owning session is
            // retained by this future; a warm legacy query is never consulted here.
            self.definitions
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer.clone())
                .map_or(Poll::Pending, Poll::Ready)
        })
    }

    pub(crate) async fn source_checkpoint(
        &self,
        owner: Definition<'db>,
        sequence: usize,
        units: usize,
    ) -> Result<(), Boundary> {
        if self
            .definitions
            .borrow()
            .get(&owner)
            .is_none_or(|entry| entry.answer.is_some())
        {
            return Err(Boundary::SourceSupport);
        }
        let request = SourceWork {
            owner,
            sequence,
            units,
        };
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandSourceWork(request));
                registered = true;
                return Poll::Pending;
            }
            self.source_work
                .borrow()
                .get(&request)
                .and_then(|entry| entry.answer)
                .map_or(Poll::Pending, |()| Poll::Ready(Ok(())))
        })
        .await
    }

    fn consumer_header_demand(
        &self,
        child: Definition<'db>,
    ) -> impl Future<Output = HeaderAnswer<'db>> + '_ {
        self.header_demand(Key::Consumer, child)
    }

    fn generic_context_header_demand(
        &self,
        parent: Definition<'db>,
        child: Definition<'db>,
    ) -> impl Future<Output = HeaderAnswer<'db>> + '_ {
        self.header_demand(Key::GenericContext(parent), child)
    }

    fn header_demand(
        &self,
        parent: Key<'db>,
        child: Definition<'db>,
    ) -> impl Future<Output = HeaderAnswer<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandHeader { parent, child });
                registered = true;
                return Poll::Pending;
            }
            self.headers
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer)
                .map_or(Poll::Pending, Poll::Ready)
        })
    }

    fn consumer_generic_context_demand(
        &self,
        child: Definition<'db>,
    ) -> impl Future<Output = GenericContextAnswer<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandGenericContext {
                        parent: Key::Consumer,
                        child,
                    });
                registered = true;
                return Poll::Pending;
            }
            self.generic_contexts
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer)
                .map_or(Poll::Pending, Poll::Ready)
        })
    }

    pub(crate) fn validate_relation(
        &self,
        request: RelationRequest<'db, 'c>,
    ) -> Result<(), Boundary> {
        if self
            .constraints
            .is_some_and(|constraints| request.belongs_to(constraints))
        {
            Ok(())
        } else {
            Err(Boundary::ConstraintDomain)
        }
    }

    pub(crate) fn declare_relation(
        &self,
        request: RelationRequest<'db, 'c>,
    ) -> Result<(), Boundary> {
        self.validate_relation(request)?;
        self.effects
            .borrow_mut()
            .push(Effect::DeclareRelation(request));
        Ok(())
    }

    pub(crate) async fn consumer_relation_demand(
        &self,
        request: RelationRequest<'db, 'c>,
    ) -> RelationAnswer<'db, 'c> {
        self.validate_relation(request)?;
        if request.is_composed() {
            return Err(Boundary::RelationContext);
        }
        RelationDemand {
            router: self,
            parent: Key::Consumer,
            request,
            registered: false,
        }
        .await
    }

    pub(crate) async fn relation_demand(
        &self,
        parent: RelationKey<'db>,
        request: RelationRequest<'db, 'c>,
    ) -> RelationAnswer<'db, 'c> {
        self.validate_relation(request)?;
        RelationDemand {
            router: self,
            parent: Key::Relation(parent),
            request,
            registered: false,
        }
        .await
    }

    pub(crate) fn relation_conversion_demand(
        &self,
        parent: RelationKey<'db>,
        child: CallableConversionRequest<'db>,
    ) -> impl Future<Output = Answer<'db>> + '_ {
        self.demand(Key::Relation(parent), child)
    }

    pub(crate) fn relation_conversion_plan_demand(
        &self,
        parent: RelationKey<'db>,
        child: CallableConversionRequest<'db>,
    ) -> impl Future<Output = ConversionPlan<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandConversionPlan {
                        parent: Key::Relation(parent),
                        child,
                    });
                registered = true;
                return Poll::Pending;
            }
            if let Some(plan) = self.conversion_plans.borrow().get(&child) {
                self.effects.borrow_mut().push(Effect::ConsumeConversion {
                    parent: Key::Relation(parent),
                    child,
                });
                return Poll::Ready(plan.clone());
            }
            let answer = self
                .entries
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer.clone());
            answer.map_or(Poll::Pending, |answer| {
                self.effects.borrow_mut().push(Effect::ConsumeConversion {
                    parent: Key::Relation(parent),
                    child,
                });
                Poll::Ready(ConversionPlan::Complete(answer))
            })
        })
    }

    pub(crate) fn conversion_plan(
        &self,
        request: CallableConversionRequest<'db>,
    ) -> Option<ConversionPlan<'db>> {
        self.conversion_plans.borrow().get(&request).cloned()
    }

    pub(crate) fn publish_relation_composition(
        &self,
        request: RelationKey<'db>,
        alternatives: Rc<[RelationRequest<'db, 'c>]>,
    ) -> Result<(), Boundary> {
        for alternative in alternatives.iter() {
            self.validate_relation(*alternative)?;
        }
        let mut effects = self.effects.borrow_mut();
        effects.extend(alternatives.iter().copied().map(Effect::DeclareRelation));
        effects.push(Effect::PublishRelationComposition {
            request,
            alternatives,
        });
        Ok(())
    }

    pub(crate) fn relation_composition(
        &self,
        request: RelationKey<'db>,
    ) -> Option<Rc<[RelationRequest<'db, 'c>]>> {
        self.relation_compositions.borrow().get(&request).cloned()
    }

    pub(crate) fn relation_answer(
        &self,
        request: RelationKey<'db>,
    ) -> Option<RelationAnswer<'db, 'c>> {
        self.relations
            .borrow()
            .get(&request)
            .and_then(|entry| entry.answer.clone())
    }

    pub(crate) fn constructor_conversion_demand(
        &self,
        parent: ConstructorCallableRequest<'db>,
        child: CallableConversionRequest<'db>,
    ) -> impl Future<Output = Answer<'db>> + '_ {
        self.demand(Key::Constructor(parent), child)
    }

    fn constructor_demand(
        &self,
        parent: Key<'db>,
        child: ConstructorCallableRequest<'db>,
    ) -> impl Future<Output = ConstructorAnswer<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandConstructor { parent, child });
                registered = true;
                return Poll::Pending;
            }
            self.constructors
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer.clone())
                .map_or(Poll::Pending, Poll::Ready)
        })
    }

    pub(crate) fn declare_descriptor(&self, child: DescriptorRequest<'db>) {
        self.effects
            .borrow_mut()
            .push(Effect::DeclareDescriptor(child));
    }

    pub(crate) fn descriptor_demand(
        &self,
        parent: DescriptorRequest<'db>,
        child: DescriptorRequest<'db>,
    ) -> impl Future<Output = DescriptorAnswer<'db>> + '_ {
        self.descriptor_demand_from(Key::Descriptor(parent), child)
    }

    fn descriptor_demand_from(
        &self,
        parent: Key<'db>,
        child: DescriptorRequest<'db>,
    ) -> impl Future<Output = DescriptorAnswer<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandDescriptor { parent, child });
                registered = true;
                return Poll::Pending;
            }
            self.descriptors
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer)
                .map_or(Poll::Pending, Poll::Ready)
        })
    }

    pub(crate) async fn descriptor_invocation_demand(
        &self,
        owner: DescriptorRequest<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Boundary> {
        self.invocation_demand(Key::Descriptor(owner), owner, request)
            .await
            .map(|result| result.map(|bindings| *bindings))
    }

    async fn invocation_demand(
        &self,
        parent: Key<'db>,
        owner: DescriptorRequest<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> InvocationAnswer<'db> {
        let sequence = {
            let mut sequences = self.invocation_sequences.borrow_mut();
            let next = sequences.entry(owner).or_default();
            let sequence = *next;
            *next = next.checked_add(1).ok_or(Boundary::InvocationIdentity)?;
            sequence
        };
        let reply = Rc::new(RefCell::new(None));
        let mut ticket = Some(InvocationTicket {
            key: InvocationKey {
                owner,
                sequence,
                request,
            },
            reply: Rc::clone(&reply),
        });
        std::future::poll_fn(move |_| {
            if let Some(ticket) = ticket.take() {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandInvocation { parent, ticket });
                return Poll::Pending;
            }
            reply.borrow_mut().take().map_or(Poll::Pending, Poll::Ready)
        })
        .await
    }

    fn declare(&self, child: CallableConversionRequest<'db>) {
        self.effects.borrow_mut().push(Effect::Declare(child));
    }

    fn demand(
        &self,
        parent: Key<'db>,
        child: CallableConversionRequest<'db>,
    ) -> Demand<'_, 'db, 'c> {
        Demand {
            router: self,
            parent,
            child,
            registered: false,
        }
    }

    pub(crate) fn consumer_demand(
        &self,
        child: CallableConversionRequest<'db>,
    ) -> impl Future<Output = Answer<'db>> + '_ {
        self.demand(Key::Consumer, child)
    }
}

struct Demand<'eval, 'db, 'c> {
    router: &'eval Router<'db, 'c>,
    parent: Key<'db>,
    child: CallableConversionRequest<'db>,
    registered: bool,
}

impl<'db> Future for Demand<'_, 'db, '_> {
    type Output = Answer<'db>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.registered {
            self.router.effects.borrow_mut().push(Effect::Demand {
                parent: self.parent,
                child: self.child,
            });
            self.registered = true;
            return Poll::Pending;
        }
        let answer = self
            .router
            .entries
            .borrow()
            .get(&self.child)
            .and_then(|entry| entry.answer.clone());
        answer.map_or(Poll::Pending, |answer| {
            self.router
                .effects
                .borrow_mut()
                .push(Effect::ConsumeConversion {
                    parent: self.parent,
                    child: self.child,
                });
            Poll::Ready(answer)
        })
    }
}

struct RelationDemand<'eval, 'db, 'c> {
    router: &'eval Router<'db, 'c>,
    parent: Key<'db>,
    request: RelationRequest<'db, 'c>,
    registered: bool,
}

impl<'db, 'c> Future for RelationDemand<'_, 'db, 'c> {
    type Output = RelationAnswer<'db, 'c>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.registered {
            self.router
                .effects
                .borrow_mut()
                .push(Effect::DemandRelation {
                    parent: self.parent,
                    child: self.request,
                });
            self.registered = true;
            return Poll::Pending;
        }
        self.router
            .relations
            .borrow()
            .get(&self.request.key())
            .and_then(|entry| entry.answer.clone())
            .map_or(Poll::Pending, Poll::Ready)
    }
}

async fn convert<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    request: CallableConversionRequest<'db>,
) -> Answer<'db> {
    // Recursive-reference detection discovers source overloads. Prepared callable signatures
    // do not establish whether the implementation belongs to that recursive definition.
    if matches!(request.ty, Type::FunctionLiteral(_)) && request.recursive_definition.is_some() {
        return Err(Boundary::SourceSignature);
    }

    // Admit stored values and prepared declaration callables. Descriptor, normalization and
    // receiver-relation work still require their own supervised adapters.
    match request.ty {
        Type::Callable(_) | Type::FunctionLiteral(_) | Type::Union(_) | Type::Never => {}
        Type::GenericAlias(_) => {}
        Type::SubclassOf(subclass)
            if request.policy == UpcastPolicy::Unsound
                && matches!(subclass.subclass_of(), SubclassOfInner::Class(_)) => {}
        Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper))
            if wrapper.kind(db) == MethodWrapperKind::Staticmethod => {}
        Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(_)) => {}
        _ => return Err(Boundary::SemanticOperation),
    }

    let mut step = conversion::start(db, env, request, true);
    loop {
        step = match step {
            ConversionStep::Complete(value) => return Ok(value),
            ConversionStep::Function(pending) => {
                let callable = router
                    .declarations
                    .as_ref()
                    .and_then(|declarations| declarations.callable(pending.function))
                    .ok_or(Boundary::SourceSignature)?;
                pending.resume(callable)
            }
            ConversionStep::RuntimeUnion(union) => {
                let alternatives: Rc<[_]> = union.requests().collect();
                let mut effects = router.effects.borrow_mut();
                effects.extend(alternatives.iter().copied().map(Effect::Declare));
                effects.push(Effect::PublishConversionPlan {
                    request,
                    plan: ConversionPlan::Alternatives(alternatives),
                });
                drop(effects);
                union.sequential(db, env)
            }
            ConversionStep::Convert(pending) => {
                if let Some(transformation) = pending.transform() {
                    router
                        .effects
                        .borrow_mut()
                        .push(Effect::PublishConversionPlan {
                            request,
                            plan: ConversionPlan::Transform {
                                input: pending.request,
                                transformation,
                            },
                        });
                }
                let answer = router
                    .demand(Key::Conversion(request), pending.request)
                    .await?;
                pending.resume(db, env, answer)
            }
            ConversionStep::Constructor { class, receiver } => {
                return router
                    .constructor_demand(
                        Key::Conversion(request),
                        ConstructorCallableRequest { class, receiver },
                    )
                    .await
                    .map(Some);
            }
            ConversionStep::CallMember(_)
            | ConversionStep::CachedBoundMethod(_)
            | ConversionStep::SubclassInstance(_) => {
                return Err(Boundary::SemanticOperation);
            }
        };
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot<'db> {
    static_mro_values: FxHashMap<StaticMroRequest<'db>, mro_task::StaticMroResultId>,
    static_mro_pending: FxHashSet<StaticMroRequest<'db>>,
    mapping_values: FxHashMap<MappingRequest<'db>, MappingAnswer<'db>>,
    mapping_pending: FxHashSet<MappingRequest<'db>>,
    values: FxHashMap<CallableConversionRequest<'db>, Answer<'db>>,
    pending: FxHashSet<CallableConversionRequest<'db>>,
    header_values: FxHashMap<Definition<'db>, HeaderAnswer<'db>>,
    header_pending: FxHashSet<Definition<'db>>,
    generic_context_values: FxHashMap<Definition<'db>, GenericContextAnswer<'db>>,
    generic_context_pending: FxHashSet<Definition<'db>>,
    definition_values: FxHashMap<Definition<'db>, DefinitionAnswer<'db>>,
    definition_pending: FxHashSet<Definition<'db>>,
    polls: FxHashMap<CallableConversionRequest<'db>, usize>,
    work: usize,
    boundaries: Vec<usize>,
    exhausted: bool,
}

pub(crate) struct ConsumerSnapshot<'db, R> {
    pub(crate) consumer: Option<R>,
    pub(crate) consumer_polls: usize,
    pub(crate) relation_polls: FxHashMap<RelationKey<'db>, usize>,
    constructor_polls: FxHashMap<ConstructorCallableRequest<'db>, usize>,
    descriptor_polls: FxHashMap<DescriptorRequest<'db>, usize>,
    invocation_polls: FxHashMap<InvocationKey<'db>, usize>,
    header_polls: FxHashMap<Definition<'db>, usize>,
    generic_context_polls: FxHashMap<Definition<'db>, usize>,
    definition_polls: FxHashMap<Definition<'db>, usize>,
    definition_starts: FxHashMap<Definition<'db>, usize>,
    source_work_polls: FxHashMap<SourceWork<'db>, usize>,
    mapping_polls: FxHashMap<MappingRequest<'db>, usize>,
    semantic_work_polls: FxHashMap<SemanticWork<'db>, usize>,
    static_mro_polls: FxHashMap<StaticMroRequest<'db>, usize>,
    static_mro_starts: FxHashMap<StaticMroRequest<'db>, usize>,
    static_mro_counts: mro_task::MroCounters,
    graph: Snapshot<'db>,
}

impl<R> ConsumerSnapshot<'_, R> {
    pub(crate) fn work(&self) -> usize {
        self.graph.work
    }
}

fn run<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    root: CallableConversionRequest<'db>,
    budget: usize,
    reverse_execution: bool,
    reverse_merge: bool,
) -> Snapshot<'db> {
    let router = Router::default();
    router.entries.borrow_mut().insert(root, Entry::default());
    let mut tasks: FxHashMap<_, Task<'_, 'db, '_, ()>> = FxHashMap::default();
    tasks.insert(
        Key::Conversion(root),
        conversion_task(db, env, &router, root),
    );
    drive(
        db,
        env,
        &router,
        tasks,
        budget,
        reverse_execution,
        reverse_merge,
    )
    .expect("fresh root router")
    .graph
}

pub(crate) fn run_with<'eval, 'db: 'eval, 'c: 'eval, R: 'eval, Fut>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    budget: usize,
    reverse_execution: bool,
    reverse_merge: bool,
    consumer: impl FnOnce(&'eval Router<'db, 'c>) -> Fut,
) -> Result<ConsumerSnapshot<'db, R>, Boundary>
where
    Fut: Future<Output = R> + 'eval,
{
    router.validate_declarations(db, env)?;
    let consumer = consumer(router);
    let mut tasks: FxHashMap<_, Task<'eval, 'db, 'c, R>> = FxHashMap::default();
    tasks.insert(
        Key::Consumer,
        Box::pin(async move { Output::Consumer(consumer.await) }),
    );
    drive(
        db,
        env,
        router,
        tasks,
        budget,
        reverse_execution,
        reverse_merge,
    )
}

fn conversion_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: CallableConversionRequest<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move { Output::Conversion(convert(db, env, router, request).await) })
}

fn relation_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: RelationRequest<'db, 'c>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        let Some(constraints) = router.constraints else {
            return Output::Relation(Err(Boundary::ConstraintDomain));
        };
        Output::Relation(scheduled_requests::evaluate(db, env, constraints, router, request).await)
    })
}

fn constructor_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: ConstructorCallableRequest<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        let class_env =
            ProgramEnvironment::from_file(request.class.class_literal(db).program_file(db));
        if class_env.program(db) != env.program(db) {
            return Output::Constructor(Err(Boundary::ProgramDomain));
        }
        Output::Constructor(
            constructor_callables_with(
                db,
                &class_env,
                &QueuedConstructorEffects {
                    router,
                    parent: request,
                },
                request,
            )
            .await,
        )
    })
}

fn descriptor_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: DescriptorRequest<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        Output::Descriptor(
            descriptor::evaluate_entry_with_effects(
                db,
                env,
                request,
                &QueuedDescriptorEffects {
                    router,
                    parent: request,
                },
            )
            .await,
        )
    })
}

fn invocation_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    ticket: InvocationTicket<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        Output::Invocation {
            reply: ticket.reply,
            answer: Err(Boundary::InvocationPreparation),
        }
    })
}

fn header_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: Definition<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move { Output::Header(source::evaluate_header(db, env, router, request)) })
}

fn generic_context_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: Definition<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        Output::GenericContext(source::evaluate_generic_context(db, env, router, request).await)
    })
}

fn definition_task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: Definition<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        Output::Definition(evaluate_scheduled_definition(db, env, router, request).await)
    })
}

fn install_mro_tasks<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    router: &'eval Router<'db, 'c>,
    tasks: &mut FxHashMap<Key<'db>, ScheduledTask<'eval, 'db, 'c, R>>,
    starts: &mut FxHashMap<StaticMroRequest<'db>, usize>,
    next: &mut FxIndexSet<Key<'db>>,
    commit: &mut mro_task::Commit<'db>,
) -> Result<(), Boundary> {
    for request in commit.completed.drain(..) {
        tasks.remove(&Key::StaticMro(request));
    }
    for (request, node) in commit.spawned.drain(..) {
        tasks.insert(
            Key::StaticMro(request),
            ScheduledTask {
                future: mro_task::task(db, router, request, node),
                mro_owner: Some(node),
            },
        );
        router.static_mro.borrow_mut().activate(node)?;
        *starts.entry(request).or_default() += 1;
        next.insert(Key::StaticMro(request));
    }
    next.extend(commit.wakeups.drain(..));
    Ok(())
}

fn drive<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    tasks: FxHashMap<Key<'db>, Task<'eval, 'db, 'c, R>>,
    budget: usize,
    reverse_execution: bool,
    reverse_merge: bool,
) -> Result<ConsumerSnapshot<'db, R>, Boundary> {
    router.validate_declarations(db, env)?;
    // Tables have one program/revision and budget lifetime. Reusing a router would also
    // retain cancelled task entries, whose futures no longer exist.
    if router.driven.replace(true) {
        return Err(Boundary::RootReuse);
    }
    let mut tasks = DriverTasks {
        router,
        tasks: tasks
            .into_iter()
            .map(|(key, future)| (key, ScheduledTask::ordinary(future)))
            .collect(),
    };
    router.driver_live.set(true);
    router
        .consumer_active
        .set(tasks.contains_key(&Key::Consumer));
    let _consumer_lifetime = ActiveConsumer(&router.consumer_active);
    let mut ready = tasks.keys().copied().collect::<Vec<_>>();
    let mut polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut relation_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut constructor_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut descriptor_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut invocation_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut header_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut generic_context_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut definition_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut definition_starts: FxHashMap<_, usize> = FxHashMap::default();
    let mut source_work_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut mapping_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut semantic_work_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut static_mro_polls: FxHashMap<_, usize> = FxHashMap::default();
    let mut static_mro_starts: FxHashMap<_, usize> = FxHashMap::default();
    let mut consumer = None;
    let mut consumer_polls = 0;
    let mut work = 0;
    let mut boundaries = Vec::new();
    let mut exhausted = false;
    let mut cx = Context::from_waker(Waker::noop());

    loop {
        let cancel = router
            .cancellation_probe
            .borrow()
            .as_ref()
            .is_some_and(|(at, _)| work >= *at);
        if cancel && let Some((_, token)) = router.cancellation_probe.take() {
            token.cancel();
        }
        db.unwind_if_revision_cancelled();
        if ready.is_empty() {
            let mut allowance = mro_task::Budget {
                work: &mut work,
                limit: budget,
            };
            let round = {
                let mut state = router.static_mro.borrow_mut();
                if !state.cleanup(router.consumer_active.get(), &mut allowance)? {
                    exhausted = true;
                    break;
                }
                state.cyclic_round(&mut allowance)?
            };
            let Some(round) = round else {
                exhausted = true;
                break;
            };
            if round.completed.is_empty() {
                break;
            }
            for completed in &round.completed {
                router
                    .static_mro
                    .borrow_mut()
                    .retire_owner(Some(completed.node))?;
                tasks.remove(&Key::StaticMro(completed.request));
            }
            let committed = router.static_mro.borrow_mut().commit_round(
                round,
                router.evaluation_domain.0.ok_or(Boundary::MroDomain)?,
                router.consumer_active.get(),
                &mut allowance,
            )?;
            let Some(mut committed) = committed else {
                exhausted = true;
                break;
            };
            let mut next = FxIndexSet::default();
            install_mro_tasks(
                db,
                router,
                &mut tasks,
                &mut static_mro_starts,
                &mut next,
                &mut committed,
            )?;
            next.retain(|key| tasks.contains_key(key));
            ready.extend(next);
            router
                .static_mro
                .borrow_mut()
                .deliver(committed, router.consumer_active.get());
            router.logical_work.set(work);
            boundaries.push(work);
            continue;
        }
        // Reserve the whole round before polling any task. Source payloads are charged once;
        // resumed tasks still pay for their transition and frozen dependent fanout.
        let mut payload_errors = FxHashMap::default();
        let mut payload_exhausted = false;
        let charge: usize = {
            let entries = router.entries.borrow();
            let relations = router.relations.borrow();
            let constructors = router.constructors.borrow();
            let descriptors = router.descriptors.borrow();
            let invocations = router.invocations.borrow();
            let headers = router.headers.borrow();
            let generic_contexts = router.generic_contexts.borrow();
            let definitions = router.definitions.borrow();
            let source_work = router.source_work.borrow();
            let mappings = router.mappings.borrow();
            let semantic_work = router.semantic_work.borrow();
            let static_mro = router.static_mro.borrow();
            let conversion_inputs = router.conversion_inputs.borrow();
            ready.iter().try_fold(0usize, |total, key| {
                let fanout = match key {
                    Key::Consumer => 0,
                    Key::Conversion(request) => entries[request].dependents.len(),
                    Key::Relation(request) => relations[request].dependents.len(),
                    Key::Constructor(request) => constructors[request].dependents.len(),
                    Key::Descriptor(request) => descriptors[request].dependents.len(),
                    Key::Invocation(request) => invocations[request].dependents.len(),
                    Key::Header(request) => headers[request].dependents.len(),
                    Key::GenericContext(request) => generic_contexts[request].dependents.len(),
                    Key::Definition(request) => definitions[request].dependents.len(),
                    Key::SourceWork(request) => source_work[request].dependents.len(),
                    Key::Mapping(request) => mappings[request].dependents.len(),
                    Key::StaticMro(_) => {
                        let owner = tasks
                            .get(key)
                            .and_then(|task| task.mro_owner)
                            .ok_or(Boundary::MroDomain)?;
                        static_mro.fanout(owner)?
                    }
                    Key::SemanticWork(request) => semantic_work[request].dependents.len(),
                };
                let first_source_poll = match key {
                    Key::Header(request) => !header_polls.contains_key(request),
                    Key::GenericContext(request) => !generic_context_polls.contains_key(request),
                    _ => false,
                };
                let source_payload = if first_source_poll {
                    match source::payload_debit(router, *key) {
                        Ok(debit) => debit,
                        Err(boundary) => {
                            payload_errors.insert(*key, boundary);
                            0
                        }
                    }
                } else {
                    0
                };
                let declaration_payload = match key {
                    Key::Conversion(request)
                        if !polls.contains_key(request)
                            && matches!(request.ty, Type::FunctionLiteral(_)) =>
                    {
                        // Preparation interns the callable. This pays for admission, the
                        // declaration lookup and singleton result without walking signatures.
                        8
                    }
                    _ => 0,
                };
                let composition_payload = match key {
                    Key::Conversion(request) if !polls.contains_key(request) => {
                        // Preparing and publishing a closed alternative list visits each edge.
                        // Its semantic interpretation remains in the shared dispatcher.
                        if let Type::Union(union) = request.ty {
                            union
                                .elements(db)
                                .len()
                                .checked_mul(4)
                                .ok_or(Boundary::CostOverflow)?
                        } else {
                            0
                        }
                    }
                    Key::Relation(request) => {
                        scheduled_requests::composition_payload_debit(router, *request)?
                    }
                    _ => 0,
                };
                // A resumed consumer can copy a sealed answer and transform every callable.
                // Charge that payload independently of the number of direct dependency edges.
                let rehash = match key {
                    Key::Conversion(request) => matches!(
                        router.conversion_plan(*request),
                        Some(ConversionPlan::Transform {
                            transformation: ConversionTransform::Regularize,
                            ..
                        })
                    ),
                    Key::Relation(request) => scheduled_requests::composition_regularizes(*request),
                    _ => false,
                };
                let fixed = total
                    .checked_add(1)
                    .and_then(|cost| cost.checked_add(fanout))
                    .and_then(|cost| cost.checked_add(source_payload))
                    .and_then(|cost| cost.checked_add(declaration_payload))
                    .and_then(|cost| cost.checked_add(composition_payload))
                    .and_then(|cost| {
                        cost.checked_add(match key {
                            Key::SourceWork(request) => request.units,
                            Key::SemanticWork(request) => request.units,
                            _ => 0,
                        })
                    })
                    .ok_or(Boundary::CostOverflow)?;
                let reply_poll = match key {
                    Key::Consumer => static_mro.poll_units(None)?,
                    Key::StaticMro(_) => {
                        static_mro.poll_units(tasks.get(key).and_then(|task| task.mro_owner))?
                    }
                    _ => 0,
                };
                let fixed = fixed
                    .checked_add(reply_poll)
                    .ok_or(Boundary::CostOverflow)?;
                if payload_exhausted || fixed > budget - work {
                    payload_exhausted = true;
                    return Ok(total);
                }
                let input_payload = conversion_inputs.get(key).into_iter().flatten().try_fold(
                    0usize,
                    |debit, (request, copies)| {
                        if payload_exhausted {
                            return Ok(debit);
                        }
                        let Some(remaining) = (budget - work - fixed)
                            .checked_sub(debit)
                            .and_then(|credit| credit.checked_sub(*copies))
                        else {
                            payload_exhausted = true;
                            return Ok(debit);
                        };
                        let payload =
                            match entries.get(request).and_then(|entry| entry.answer.as_ref()) {
                                Some(Ok(Some(callables))) => payload::callable_debit(
                                    db, callables, *copies, rehash, remaining,
                                ),
                                _ => Ok(Some(payload::CallableDebit {
                                    work: 0,
                                    boundary: None,
                                })),
                            };
                        let copied = match payload {
                            Ok(Some(debit)) => {
                                if let Some(boundary) = debit.boundary {
                                    payload_errors.insert(*key, boundary);
                                }
                                debit.work
                            }
                            Ok(None) => {
                                payload_exhausted = true;
                                return Ok(debit);
                            }
                            Err(boundary) => return Err(boundary),
                        };
                        copied
                            .checked_add(*copies)
                            .and_then(|cost| cost.checked_add(debit))
                            .ok_or(Boundary::CostOverflow)
                    },
                )?;
                fixed
                    .checked_add(input_payload)
                    .ok_or(Boundary::CostOverflow)
            })?
        };
        if payload_exhausted || charge > budget - work {
            exhausted = true;
            break;
        }
        router.logical_work.set(work + charge);
        if reverse_execution {
            ready.reverse();
        }
        for key in ready.drain(..) {
            match key {
                Key::Consumer => consumer_polls += 1,
                Key::Conversion(request) => *polls.entry(request).or_default() += 1,
                Key::Relation(request) => *relation_polls.entry(request).or_default() += 1,
                Key::Constructor(request) => *constructor_polls.entry(request).or_default() += 1,
                Key::Descriptor(request) => *descriptor_polls.entry(request).or_default() += 1,
                Key::Invocation(request) => *invocation_polls.entry(request).or_default() += 1,
                Key::Header(request) => *header_polls.entry(request).or_default() += 1,
                Key::GenericContext(request) => {
                    *generic_context_polls.entry(request).or_default() += 1;
                }
                Key::Definition(request) => {
                    *definition_polls.entry(request).or_default() += 1;
                }
                Key::SourceWork(request) => {
                    *source_work_polls.entry(request).or_default() += 1;
                }
                Key::Mapping(request) => *mapping_polls.entry(request).or_default() += 1,
                Key::StaticMro(request) => *static_mro_polls.entry(request).or_default() += 1,
                Key::SemanticWork(request) => *semantic_work_polls.entry(request).or_default() += 1,
            }
            let Some(task) = tasks.get_mut(&key) else {
                panic!("ready request has a live task");
            };
            if matches!(key, Key::Consumer | Key::StaticMro(_)) {
                router.static_mro.borrow_mut().begin_poll(task.mro_owner)?;
            }
            let output = match (key, payload_errors.remove(&key)) {
                (Key::Conversion(_), Some(boundary)) => {
                    Poll::Ready(Output::Conversion(Err(boundary)))
                }
                (Key::Relation(_), Some(boundary)) => Poll::Ready(Output::Relation(Err(boundary))),
                (Key::Header(_), Some(boundary)) => Poll::Ready(Output::Header(Err(boundary))),
                (Key::GenericContext(_), Some(boundary)) => {
                    Poll::Ready(Output::GenericContext(Err(boundary)))
                }
                _ => task.future.as_mut().poll(&mut cx),
            };
            if let Poll::Ready(output) = output {
                match (key, output) {
                    (Key::Conversion(request), Output::Conversion(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::Complete { request, answer });
                    }
                    (Key::Consumer, Output::Consumer(answer)) => {
                        router.consumer_active.set(false);
                        router.static_mro.borrow_mut().retire_owner(None)?;
                        consumer = Some(answer);
                        tasks.remove(&Key::Consumer);
                    }
                    (Key::Relation(request), Output::Relation(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteRelation { request, answer });
                    }
                    (Key::Constructor(request), Output::Constructor(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteConstructor { request, answer });
                    }
                    (Key::Descriptor(request), Output::Descriptor(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteDescriptor { request, answer });
                    }
                    (Key::Invocation(key), Output::Invocation { reply, answer }) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteInvocation { key, reply, answer });
                    }
                    (Key::Header(request), Output::Header(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteHeader { request, answer });
                    }
                    (Key::GenericContext(request), Output::GenericContext(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteGenericContext { request, answer });
                    }
                    (Key::Definition(request), Output::Definition(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteDefinition { request, answer });
                    }
                    (Key::SourceWork(request), Output::SourceWork) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteSourceWork(request));
                    }
                    (Key::Mapping(request), Output::Mapping(answer)) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteMapping { request, answer });
                    }
                    (Key::StaticMro(request), Output::StaticMro(outcome)) => {
                        let node = task.mro_owner.ok_or(Boundary::MroDomain)?;
                        router.static_mro.borrow_mut().retire_owner(Some(node))?;
                        router.mro_effect_count.set(
                            router
                                .mro_effect_count
                                .get()
                                .checked_add(1)
                                .ok_or(Boundary::CostOverflow)?,
                        );
                        router.effects.borrow_mut().push(Effect::CompleteStaticMro(
                            mro_task::Completion {
                                request,
                                node,
                                outcome,
                            },
                        ));
                    }
                    (Key::SemanticWork(request), Output::SemanticWork) => {
                        router
                            .effects
                            .borrow_mut()
                            .push(Effect::CompleteSemanticWork(request));
                    }
                    _ => panic!("task output matches its typed slot"),
                }
            }
        }

        work += charge;
        let mro_effects = router.mro_effect_count.replace(0);
        let stage_units = mro_effects
            .checked_mul(64)
            .and_then(|units| units.checked_add(if mro_effects == 0 { 0 } else { 32 }))
            .ok_or(Boundary::CostOverflow)?;
        if !(mro_task::Budget {
            work: &mut work,
            limit: budget,
        })
        .reserve(stage_units)?
        {
            exhausted = true;
            boundaries.push(work);
            break;
        }
        let mut mro_round = mro_task::Round {
            demands: Vec::with_capacity(mro_effects),
            completed: Vec::with_capacity(mro_effects),
        };

        let mut effects = std::mem::take(&mut *router.effects.borrow_mut());
        if reverse_merge {
            effects.reverse();
        }
        let mut conversions = Batch::default();
        let mut relations_batch = Batch::default();
        let mut constructors_batch = Batch::default();
        let mut descriptors_batch = Batch::default();
        let mut invocations_batch = Batch::default();
        let mut headers_batch = Batch::default();
        let mut generic_contexts_batch = Batch::default();
        let mut definitions_batch = Batch::default();
        let mut source_work_batch = Batch::default();
        let mut mappings_batch = Batch::default();
        let mut semantic_work_batch = Batch::default();
        let mut invocation_replies = Vec::new();
        let mut conversion_plans = FxIndexMap::default();
        let mut relation_compositions = FxHashMap::default();
        let mut consumed_conversions = Vec::new();
        let mut demanded_conversions = Vec::new();
        let mut plan_demands = FxIndexSet::default();
        for effect in effects {
            match effect {
                Effect::DemandConversionPlan { parent, child } => {
                    demanded_conversions.push((parent, child));
                    conversions.demand(parent, child, child);
                    plan_demands.insert((parent, child));
                }
                Effect::ConsumeConversion { parent, child } => {
                    consumed_conversions.push((parent, child));
                }
                Effect::PublishConversionPlan { request, plan } => {
                    conversion_plans.insert(request, plan);
                }
                Effect::PublishRelationComposition {
                    request,
                    alternatives,
                } => {
                    relation_compositions.insert(request, alternatives);
                }
                Effect::Declare(child) => {
                    conversions.declare(child, child);
                }
                Effect::Demand { parent, child } => {
                    demanded_conversions.push((parent, child));
                    conversions.demand(parent, child, child);
                }
                Effect::Complete { request, answer } => {
                    conversions.complete(request, answer);
                }
                Effect::DeclareRelation(child) => {
                    relations_batch.declare(child.key(), child);
                }
                Effect::DemandRelation { parent, child } => {
                    relations_batch.demand(parent, child.key(), child);
                }
                Effect::CompleteRelation { request, answer } => {
                    relations_batch.complete(request, answer);
                }
                Effect::DemandConstructor { parent, child } => {
                    constructors_batch.demand(parent, child, child);
                }
                Effect::CompleteConstructor { request, answer } => {
                    constructors_batch.complete(request, answer);
                }
                Effect::DeclareDescriptor(child) => descriptors_batch.declare(child, child),
                Effect::DemandDescriptor { parent, child } => {
                    descriptors_batch.demand(parent, child, child);
                }
                Effect::CompleteDescriptor { request, answer } => {
                    descriptors_batch.complete(request, answer);
                }
                Effect::DemandInvocation { parent, ticket } => {
                    invocations_batch.demand(parent, ticket.key, ticket);
                }
                Effect::CompleteInvocation { key, reply, answer } => {
                    invocations_batch.complete(key, ());
                    invocation_replies.push((reply, answer));
                }
                Effect::DeclareHeader(child) => headers_batch.declare(child, child),
                Effect::DemandHeader { parent, child } => {
                    headers_batch.demand(parent, child, child);
                }
                Effect::CompleteHeader { request, answer } => {
                    headers_batch.complete(request, answer);
                }
                Effect::DemandGenericContext { parent, child } => {
                    generic_contexts_batch.demand(parent, child, child);
                }
                Effect::CompleteGenericContext { request, answer } => {
                    generic_contexts_batch.complete(request, answer);
                }
                Effect::DemandDefinition { parent, child } => {
                    definitions_batch.demand(parent, child, child);
                }
                Effect::CompleteDefinition { request, answer } => {
                    definitions_batch.complete(request, answer);
                }
                Effect::DemandSourceWork(request) => {
                    source_work_batch.demand(Key::Definition(request.owner), request, request);
                }
                Effect::CompleteSourceWork(request) => {
                    source_work_batch.complete(request, ());
                }
                Effect::DemandMapping { parent, child } => {
                    mappings_batch.demand(parent, child, child);
                }
                Effect::CompleteMapping { request, answer } => {
                    mappings_batch.complete(request, answer);
                }
                Effect::DemandSemanticWork(request) => {
                    semantic_work_batch.demand(request.owner.key(), request, request);
                }
                Effect::CompleteSemanticWork(request) => semantic_work_batch.complete(request, ()),
                Effect::DemandStaticMro(demand) => mro_round.demands.push(demand),
                Effect::CompleteStaticMro(completion) => mro_round.completed.push(completion),
            }
        }
        let committed_mro = router.static_mro.borrow_mut().commit_round(
            mro_round,
            router.evaluation_domain.0.ok_or(Boundary::MroDomain)?,
            router.consumer_active.get(),
            &mut mro_task::Budget {
                work: &mut work,
                limit: budget,
            },
        )?;
        let Some(mut committed_mro) = committed_mro else {
            exhausted = true;
            boundaries.push(work);
            break;
        };
        let mut next: FxIndexSet<_> = FxIndexSet::default();
        install_mro_tasks(
            db,
            router,
            &mut tasks,
            &mut static_mro_starts,
            &mut next,
            &mut committed_mro,
        )?;
        let mut entries = router.entries.borrow_mut();
        let mut relations = router.relations.borrow_mut();
        let mut constructors = router.constructors.borrow_mut();
        let mut descriptors = router.descriptors.borrow_mut();
        let mut invocations = router.invocations.borrow_mut();
        let mut headers = router.headers.borrow_mut();
        let mut generic_contexts = router.generic_contexts.borrow_mut();
        let mut definitions = router.definitions.borrow_mut();
        let mut source_work = router.source_work.borrow_mut();
        let mut mappings = router.mappings.borrow_mut();
        let mut semantic_work = router.semantic_work.borrow_mut();
        // Stage all wakeups against the same snapshot plus the complete batch of outputs.
        conversions.stage_wakeups(&entries, Key::Conversion, &mut next);
        relations_batch.stage_wakeups(&relations, Key::Relation, &mut next);
        constructors_batch.stage_wakeups(&constructors, Key::Constructor, &mut next);
        descriptors_batch.stage_wakeups(&descriptors, Key::Descriptor, &mut next);
        invocations_batch.stage_wakeups(&invocations, Key::Invocation, &mut next);
        headers_batch.stage_wakeups(&headers, Key::Header, &mut next);
        generic_contexts_batch.stage_wakeups(&generic_contexts, Key::GenericContext, &mut next);
        definitions_batch.stage_wakeups(&definitions, Key::Definition, &mut next);
        source_work_batch.stage_wakeups(&source_work, Key::SourceWork, &mut next);
        mappings_batch.stage_wakeups(&mappings, Key::Mapping, &mut next);
        semantic_work_batch.stage_wakeups(&semantic_work, Key::SemanticWork, &mut next);
        for request in conversion_plans.keys() {
            if let Some(dependents) = router.conversion_plan_dependents.borrow().get(request) {
                next.extend(dependents.iter().copied());
            }
        }
        for (parent, child) in &plan_demands {
            if router.conversion_plans.borrow().contains_key(child)
                || conversion_plans.contains_key(child)
            {
                next.insert(*parent);
            }
        }
        {
            let mut dependents = router.conversion_plan_dependents.borrow_mut();
            for (parent, child) in plan_demands {
                dependents.entry(child).or_default().insert(parent);
            }
        }
        {
            let mut inputs = router.conversion_inputs.borrow_mut();
            for (parent, child) in demanded_conversions {
                let copies = inputs.entry(parent).or_default().entry(child).or_default();
                *copies = copies.checked_add(1).ok_or(Boundary::CostOverflow)?;
            }
            for (parent, child) in consumed_conversions {
                if let Some(waiting) = inputs.get_mut(&parent) {
                    if let Some(copies) = waiting.get_mut(&child) {
                        *copies -= 1;
                        if *copies == 0 {
                            waiting.remove(&child);
                        }
                    }
                    if waiting.is_empty() {
                        inputs.remove(&parent);
                    }
                }
            }
        }
        conversions.commit(&mut entries, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Conversion(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(conversion_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Conversion(key));
            }
        });
        relations_batch.commit(&mut relations, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Relation(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(relation_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Relation(key));
            }
        });
        constructors_batch.commit(&mut constructors, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Constructor(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(constructor_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Constructor(key));
            }
        });
        descriptors_batch.commit(&mut descriptors, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Descriptor(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(descriptor_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Descriptor(key));
            }
        });
        invocations_batch.commit(&mut invocations, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks
                    .entry(Key::Invocation(key))
                    .or_insert_with(|| ScheduledTask::ordinary(invocation_task(request)));
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Invocation(key));
            }
        });
        headers_batch.commit(&mut headers, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Header(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(header_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Header(key));
            }
        });
        generic_contexts_batch.commit(&mut generic_contexts, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::GenericContext(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(generic_context_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::GenericContext(key));
            }
        });
        definitions_batch.commit(&mut definitions, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Definition(key)).or_insert_with(|| {
                    *definition_starts.entry(key).or_default() += 1;
                    ScheduledTask::ordinary(definition_task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Definition(key));
            }
        });
        source_work_batch.commit(&mut source_work, |change| match change {
            BatchTask::Declare { key, .. } => {
                tasks.entry(Key::SourceWork(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(Box::pin(async { Output::SourceWork }))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::SourceWork(key));
            }
        });
        // Publish owned responses only after every family has committed the closed round.
        mappings_batch.commit(&mut mappings, |change| match change {
            BatchTask::Declare { key, request } => {
                tasks.entry(Key::Mapping(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(mapping::task(db, env, router, request))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::Mapping(key));
            }
        });
        semantic_work_batch.commit(&mut semantic_work, |change| match change {
            BatchTask::Declare { key, .. } => {
                tasks.entry(Key::SemanticWork(key)).or_insert_with(|| {
                    ScheduledTask::ordinary(Box::pin(async { Output::SemanticWork }))
                });
            }
            BatchTask::Complete(key) => {
                tasks.remove(&Key::SemanticWork(key));
            }
        });
        // Taking a private response later does not reopen the graph's completed invocation.
        router
            .conversion_plans
            .borrow_mut()
            .extend(conversion_plans);
        router
            .relation_compositions
            .borrow_mut()
            .extend(relation_compositions);
        for (reply, answer) in invocation_replies {
            *reply.borrow_mut() = Some(answer);
        }
        router
            .static_mro
            .borrow_mut()
            .deliver(committed_mro, router.consumer_active.get());
        next.retain(|key| match key {
            Key::Consumer => consumer.is_none(),
            Key::Conversion(request) => entries[request].answer.is_none(),
            Key::Relation(request) => relations[request].answer.is_none(),
            Key::Constructor(request) => constructors[request].answer.is_none(),
            Key::Descriptor(request) => descriptors[request].answer.is_none(),
            Key::Invocation(request) => invocations[request].answer.is_none(),
            Key::Header(request) => headers[request].answer.is_none(),
            Key::GenericContext(request) => generic_contexts[request].answer.is_none(),
            Key::Definition(request) => definitions[request].answer.is_none(),
            Key::SourceWork(request) => source_work[request].answer.is_none(),
            Key::Mapping(request) => mappings[request].answer.is_none(),
            Key::StaticMro(_) => tasks.contains_key(key),
            Key::SemanticWork(request) => semantic_work[request].answer.is_none(),
        });
        ready.extend(next);
        drop(entries);
        drop(relations);
        drop(constructors);
        drop(descriptors);
        drop(invocations);
        drop(headers);
        drop(generic_contexts);
        drop(definitions);
        drop(source_work);
        drop(mappings);
        drop(semantic_work);
        router.logical_work.set(work);
        boundaries.push(work);
    }

    // Drop parked tasks before the router they borrow, regardless of dependency depth.
    router.driver_live.set(false);
    drop(tasks);
    let entries = router.entries.borrow();
    let headers = router.headers.borrow();
    let generic_contexts = router.generic_contexts.borrow();
    let definitions = router.definitions.borrow();
    let mappings = router.mappings.borrow();
    Ok(ConsumerSnapshot {
        consumer,
        consumer_polls,
        relation_polls,
        constructor_polls,
        descriptor_polls,
        invocation_polls,
        header_polls,
        generic_context_polls,
        definition_polls,
        definition_starts,
        source_work_polls,
        mapping_polls,
        semantic_work_polls,
        static_mro_polls,
        static_mro_starts,
        static_mro_counts: router.static_mro.borrow().counters(),
        graph: Snapshot {
            static_mro_values: router.static_mro.borrow().values(),
            static_mro_pending: router.static_mro.borrow().pending(),
            mapping_values: mappings
                .iter()
                .filter_map(|(key, entry)| entry.answer.map(|value| (*key, value)))
                .collect(),
            mapping_pending: mappings
                .iter()
                .filter_map(|(key, entry)| entry.answer.is_none().then_some(*key))
                .collect(),
            values: entries
                .iter()
                .filter_map(|(key, entry)| entry.answer.clone().map(|value| (*key, value)))
                .collect(),
            pending: entries
                .iter()
                .filter_map(|(key, entry)| entry.answer.is_none().then_some(*key))
                .collect(),
            header_values: headers
                .iter()
                .filter_map(|(key, entry)| entry.answer.map(|value| (*key, value)))
                .collect(),
            header_pending: headers
                .iter()
                .filter_map(|(key, entry)| entry.answer.is_none().then_some(*key))
                .collect(),
            generic_context_values: generic_contexts
                .iter()
                .filter_map(|(key, entry)| entry.answer.map(|value| (*key, value)))
                .collect(),
            generic_context_pending: generic_contexts
                .iter()
                .filter_map(|(key, entry)| entry.answer.is_none().then_some(*key))
                .collect(),
            definition_values: definitions
                .iter()
                .filter_map(|(key, entry)| entry.answer.clone().map(|value| (*key, value)))
                .collect(),
            definition_pending: definitions
                .iter()
                .filter_map(|(key, entry)| entry.answer.is_none().then_some(*key))
                .collect(),
            polls,
            work,
            boundaries,
            exhausted,
        },
    })
}

fn request(ty: Type<'_>) -> CallableConversionRequest<'_> {
    CallableConversionRequest::new(ty, UpcastPolicy::Unsound)
}

fn wrap<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
        db,
        ty,
        MethodWrapperKind::Staticmethod,
    )))
}

#[test]
fn scheduled_conversion_uses_real_continuations() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let first = Type::Callable(CallableType::single(&db, Signature::unknown()));
    let second = Type::Callable(CallableType::single(&db, Signature::bottom()));
    let child = wrap(&db, first);
    let root_ty = UnionType::from_elements(&db, &env, [wrap(&db, child), wrap(&db, second)]);
    let root = request(root_ty);
    let full = run(&db, &env, root, 1000, false, false);
    let expected = root.evaluate(&db, &env, None);
    assert_eq!(full.values.get(&root), Some(&Ok(expected)));
    assert!(full.pending.is_empty());
    assert!(full.polls.values().all(|polls| *polls <= 3));
    for budget in 0..=full.work + 1 {
        let baseline = run(&db, &env, root, budget, false, false);
        for execution in [false, true] {
            for merge in [false, true] {
                assert_eq!(baseline, run(&db, &env, root, budget, execution, merge));
            }
        }
    }
}

#[test]
fn scheduled_conversion_refuses_unmigrated_boundaries() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let callable = CallableType::single(&db, Signature::unknown());
    let first = Type::Callable(callable);
    let unsupported = Type::int_literal(0);
    let root = request(UnionType::from_elements(
        &db,
        &env,
        [wrap(&db, unsupported), wrap(&db, first)],
    ));
    let result = run(&db, &env, root, 1000, false, false);
    assert_eq!(
        result.values.get(&root),
        Some(&Err(Boundary::SemanticOperation))
    );
    assert_eq!(
        result.values.get(&request(first)),
        Some(&Ok(Some(CallableTypes::one(callable))))
    );
}

#[test]
fn scheduled_conversion_does_not_recursively_poll_wrapper_chains() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let leaf = CallableType::single(&db, Signature::unknown());
    let mut ty = Type::Callable(leaf);
    for depth in 0..=1024 {
        if [0, 1, 16, 1024].contains(&depth) {
            let root = request(ty);
            let result = run(&db, &env, root, 10_000, false, false);
            assert_eq!(
                result.values.get(&root),
                Some(&Ok(Some(CallableTypes::one(leaf))))
            );
            assert_eq!(result.polls.len(), depth + 1);
            assert_eq!(result.polls.values().sum::<usize>(), 2 * depth + 1);
            assert!(result.pending.is_empty());
        }
        ty = wrap(&db, ty);
    }
}

#[test]
fn scheduled_conversion_sequential_demands_share_completed_children() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let leaf = CallableType::single(&db, Signature::unknown());
    let child = request(wrap(&db, wrap(&db, Type::Callable(leaf))));
    for count in [1, 4, 32] {
        let router = Router::default();
        let starts = Cell::new(0);
        let resumes = Cell::new(0);
        let result = run_with(&db, &env, &router, 10_000, false, false, |router| async {
            starts.set(starts.get() + 1);
            for index in 0..count {
                let value = router.consumer_demand(child).await?;
                assert_eq!(value, Some(CallableTypes::one(leaf)));
                assert_eq!(resumes.get(), index);
                resumes.set(index + 1);
            }
            Ok::<_, Boundary>(count)
        })
        .expect("fresh root router");
        assert_eq!(result.consumer, Some(Ok(count)));
        assert_eq!(result.consumer_polls, count + 1);
        assert_eq!(starts.get(), 1);
        assert_eq!(resumes.get(), count);
        assert_eq!(result.graph.polls.len(), 3);
        assert_eq!(result.graph.polls.values().sum::<usize>(), 5);
    }
}

#[test]
fn scheduled_conversion_accounts_for_wide_sealed_inputs() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let mut wrapper_costs = Vec::new();
    for width in [1, 64] {
        let ty = UnionType::from_elements(
            &db,
            &env,
            (0..width).map(|index| {
                Type::Callable(CallableType::single(
                    &db,
                    Signature::new(crate::types::Parameters::empty(), Type::int_literal(index)),
                ))
            }),
        );
        let unwrapped = run(&db, &env, request(ty), 10_000, false, false);
        let wrapped_request = request(wrap(&db, ty));
        let wrapped = run(&db, &env, wrapped_request, 10_000, false, false);
        assert!(unwrapped.pending.is_empty());
        assert!(wrapped.pending.is_empty());
        assert_eq!(
            unwrapped.values[&request(ty)],
            wrapped.values[&wrapped_request]
        );
        wrapper_costs.push(wrapped.work - unwrapped.work);

        let stopped = run(&db, &env, wrapped_request, wrapped.work - 1, false, false);
        assert!(!stopped.values.contains_key(&wrapped_request));
        assert!(stopped.pending.contains(&wrapped_request));
    }
    // The extra wrapper copies the complete callable set even though it adds just one edge.
    assert!(wrapper_costs[1] > 32 * wrapper_costs[0]);
}

#[test]
fn scheduled_regularization_accounts_for_signature_structure() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let mut costs = Vec::new();
    for (overloads, parameters, name_bytes) in [(1, 1, 1), (64, 1, 1), (1, 64, 1), (1, 1, 10_000)] {
        let name = Name::from("x".repeat(name_bytes));
        let callable = CallableType::new(
            &db,
            CallableSignature::from_overloads((0..overloads).map(|index| {
                Signature::new(
                    Parameters::standard((0..parameters).map(|_| {
                        Parameter::positional_only(Some(name.clone()))
                            .with_annotated_type(Type::Never)
                    })),
                    Type::int_literal(index),
                )
            })),
            super::CallableTypeKind::FunctionLike,
        );
        let root = request(Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(
            InternedType::new(&db, Type::Callable(callable)),
        )));
        let complete = run(&db, &env, root, 100_000, false, false);
        assert_eq!(
            complete.values.get(&root),
            Some(&Ok(root.evaluate(&db, &env, None)))
        );
        assert!(complete.pending.is_empty());
        costs.push(complete.work);
        let stopped = run(&db, &env, root, complete.work - 1, false, false);
        assert!(stopped.pending.contains(&root));
        assert!(!stopped.values.contains_key(&root));
    }
    assert!(costs[1] > 8 * costs[0]);
    assert!(costs[2] > 8 * costs[0]);
    assert!(costs[3] > 100 * costs[0]);
}
