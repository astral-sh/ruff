use std::slice;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::{
    RecursiveNormalizationEffects, RecursiveNormalizationFacts, RecursiveNormalizationOperation,
    RecursiveNormalizationRequest, recursive_normalize_with,
};
#[cfg(test)]
use crate::Db;
use crate::types::instance::NominalInstanceClass;
use crate::types::instance::normalization::{
    NominalNormalizationEffects, NominalNormalizationFacts, nominal_normalize_with,
};
use crate::types::set_theoretic::normalization::{
    UnionNormalizationEffects, UnionNormalizationFacts, union_normalize_with,
};
use crate::types::tuple::buffer::{TupleBuffer, TupleBufferStorageEffects};
use crate::types::tuple::normalization::{
    TupleNormalizationEffects, TupleNormalizationFacts, tuple_normalize_with,
    tuple_spec_normalize_with,
};
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::{
    BoundMethodType, BoundSuperType, CallableType, ClassLiteral, EnumComplementType, FunctionType,
    GenericAlias, IntersectionType, KnownBoundMethodType, KnownInstanceType, NewType,
    NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, RecursiveVar,
    RecursivelyDefined, SlotDescriptorType, SubclassOfType, Type, TypeFormType, TypeGuardType,
    TypeIsType, UnionBuilder, UnionType,
};
use crate::{Program, ProgramEnvironment};

/// Cloning copies retained handles without allocation or database operations.
pub(in crate::types) trait RetainedNormalizationSource<'run, 'db: 'run>:
    Clone + 'run
{
    type Effects<'call>: NormalizationSourceEffects<'run, 'db>
    where
        Self: 'call;

    fn effects(&self) -> Self::Effects<'_>;
}

pub(in crate::types) trait NormalizationSourceEffects<'run, 'db: 'run>: TupleBufferStorageEffects<'db> {
    #[cfg(test)]
    fn db(&self) -> &'db dyn Db;
    fn endpoint(&self) -> &TaskEndpoint<'run, 'db>;

    async fn unavailable<T>(&self, operation: RecursiveNormalizationOperation) -> RunResult<T>;

    async fn retain_environment(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<&'run ProgramEnvironment<'db>>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>>;

    async fn tuple_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>>;

    async fn intern_tuple(
        &self,
        program: Program<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>>;

    async fn type_form_argument(&self, form: TypeFormType<'db>) -> RunResult<Type<'db>>;

    async fn intern_type_form(&self, argument: Type<'db>) -> RunResult<Type<'db>>;

    async fn new_recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<UnionBuilder<'db>>;

    async fn union_recursion(&self, union: UnionType<'db>) -> RunResult<RecursivelyDefined>;

    async fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> RunResult<()>;

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]>;

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()>;

    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>>;
}

pub(in crate::types) async fn recursive_normalize_with_retained<
    'run,
    'db: 'run,
    R: RetainedNormalizationSource<'run, 'db>,
>(
    request: RecursiveNormalizationRequest<'db>,
    env: &'run ProgramEnvironment<'db>,
    source: R,
) -> RunResult<Option<Type<'db>>> {
    let source_effects = source.effects();
    let effects = SourceNormalization {
        endpoint: source_effects.endpoint(),
        env,
        source: &source,
    };
    #[cfg(test)]
    effects
        .local(1, 0, || {
            observations::root(source_effects.db(), env);
            Ok(())
        })
        .await?;
    recursive_normalize_with(request, env, &effects, RecursiveNormalizationFacts).await
}

struct SourceNormalization<'call, 'run, 'db: 'run, R> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    env: &'run ProgramEnvironment<'db>,
    source: &'call R,
}

impl<'run, 'db: 'run, R: RetainedNormalizationSource<'run, 'db>>
    SourceNormalization<'_, 'run, 'db, R>
{
    fn check_environment(&self, env: &ProgramEnvironment<'db>) -> RunResult<()> {
        if std::ptr::eq(env, self.env) {
            Ok(())
        } else {
            Err(RunError::Contract(
                "normalization environment is not retained",
            ))
        }
    }

    async fn local<T>(
        &self,
        work: usize,
        bytes: usize,
        action: impl FnOnce() -> RunResult<T>,
    ) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(work)?;
                if bytes != 0 {
                    self.endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: bytes,
                    })?;
                }
                self.endpoint.check_completion()?;
                action()
            })
            .await)
    }

    async fn unavailable<T>(&self, operation: RecursiveNormalizationOperation) -> RunResult<T> {
        self.source.effects().unavailable(operation).await
    }

    async fn child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let (source, endpoint) = self
            .endpoint
            .local_call(|| {
                let work = size_of::<R>()
                    .checked_add(size_of::<TaskEndpoint<'run, 'db>>())
                    .and_then(|work| {
                        work.checked_add(size_of::<RecursiveNormalizationRequest<'db>>())
                    })
                    .and_then(|work| work.checked_add(size_of::<&ProgramEnvironment<'db>>()))
                    .and_then(|work| work.checked_mul(2))
                    .ok_or(RunError::Contract(
                        "normalization child capture quotation overflow",
                    ))?;
                self.endpoint.admit_work(work)?;
                self.endpoint.check_completion()?;
                self.check_environment(env)?;
                Ok((R::clone(self.source), self.endpoint.clone()))
            })
            .await;
        let env = self.env;
        #[cfg(test)]
        let child = self
            .local(1, 0, || Ok(observations::Child::new(env)))
            .await?;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .demand(move || async move {
                        #[cfg(test)]
                        let child = child;
                        #[cfg(test)]
                        endpoint
                            .local_call(|| child.enter(source.effects().db(), &endpoint, env))
                            .await;
                        let effects = SourceNormalization {
                            endpoint: &endpoint,
                            env,
                            source: &source,
                        };
                        recursive_normalize_with(
                            request,
                            env,
                            &effects,
                            RecursiveNormalizationFacts,
                        )
                        .await
                    })?
                    .await
            })
            .await)
    }
}

impl<'run, 'db: 'run, R: RetainedNormalizationSource<'run, 'db>> RecursiveNormalizationEffects<'db>
    for SourceNormalization<'_, 'run, 'db, R>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local(1, 0, || Ok(())).await
    }

    async fn nominal_instance(
        &self,
        value: NominalInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        Ok(nominal_normalize_with(
            value,
            env,
            divergent,
            nested,
            self,
            NominalNormalizationFacts,
        )
        .await?
        .map(Type::NominalInstance))
    }

    async fn unbound_recursive_variable(
        &self,
        _value: RecursiveVar<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::UnboundRecursiveVariable)
            .await
    }

    async fn union(
        &self,
        value: UnionType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        union_normalize_with(value, env, divergent, nested, self, UnionNormalizationFacts).await
    }

    async fn intersection(
        &self,
        _value: IntersectionType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::Intersection)
            .await
    }

    async fn enum_complement(
        &self,
        _value: EnumComplementType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::EnumComplement)
            .await
    }

    async fn callable(
        &self,
        _value: CallableType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::Callable)
            .await
    }

    async fn protocol_instance(
        &self,
        _value: ProtocolInstanceType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::ProtocolInstance)
            .await
    }

    async fn function_literal(
        &self,
        _value: FunctionType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::FunctionLiteral)
            .await
    }

    async fn property_instance(
        &self,
        _value: PropertyInstanceType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::PropertyInstance)
            .await
    }

    async fn slot_descriptor(
        &self,
        _value: SlotDescriptorType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::SlotDescriptor)
            .await
    }

    async fn known_bound_method(
        &self,
        _value: KnownBoundMethodType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::KnownBoundMethod)
            .await
    }

    async fn bound_method(
        &self,
        _value: BoundMethodType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::BoundMethod)
            .await
    }

    async fn bound_super(
        &self,
        _value: BoundSuperType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::BoundSuper)
            .await
    }

    async fn generic_alias(
        &self,
        _value: GenericAlias<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::GenericAlias)
            .await
    }

    async fn class_literal(
        &self,
        _value: ClassLiteral<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::ClassLiteral)
            .await
    }

    async fn subclass_of(
        &self,
        _value: SubclassOfType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::SubclassOf)
            .await
    }

    async fn known_instance(
        &self,
        _value: KnownInstanceType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::KnownInstance)
            .await
    }

    async fn type_is(
        &self,
        _value: TypeIsType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::TypeIs)
            .await
    }

    async fn type_guard(
        &self,
        _value: TypeGuardType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::TypeGuard)
            .await
    }

    async fn new_type_instance(
        &self,
        _value: NewType<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::NewTypeInstance)
            .await
    }

    async fn type_form_argument(&self, value: TypeFormType<'db>) -> RunResult<Type<'db>> {
        self.source.effects().type_form_argument(value).await
    }

    async fn normalize_child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.child(request, env).await
    }

    async fn intern_type_form(&self, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.source.effects().intern_type_form(argument).await
    }
}

impl<'run, 'db: 'run, R: RetainedNormalizationSource<'run, 'db>> UnionNormalizationEffects<'db>
    for SourceNormalization<'_, 'run, 'db, R>
{
    type Error = RunError;

    async fn new_recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        self.local(1, 0, || self.check_environment(env)).await?;
        self.source.effects().new_recovery_union(env).await
    }

    async fn union_recursion(&self, union: UnionType<'db>) -> RunResult<RecursivelyDefined> {
        self.source.effects().union_recursion(union).await
    }

    async fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> RunResult<()> {
        self.source
            .effects()
            .merge_recursion(builder, recursion)
            .await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.source.effects().union_elements(union).await
    }

    async fn next_element(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(size_of::<Type<'db>>() + 2, 0, || {
            Ok(elements.next().copied())
        })
        .await
    }

    async fn normalize_child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.child(request, env).await
    }

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        self.source.effects().union_add(builder, ty).await
    }

    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        self.source.effects().union_build(builder).await
    }
}

impl<'run, 'db: 'run, R: RetainedNormalizationSource<'run, 'db>> NominalNormalizationEffects<'db>
    for SourceNormalization<'_, 'run, 'db, R>
{
    type Error = RunError;

    async fn exact_tuple(
        &self,
        tuple: TupleType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> RunResult<Option<TupleType<'db>>> {
        tuple_normalize_with(tuple, env, divergent, nested, self).await
    }

    async fn non_tuple(
        &self,
        _class: NominalInstanceClass<'db>,
        _env: &ProgramEnvironment<'db>,
        _divergent: Type<'db>,
        _nested: bool,
    ) -> RunResult<Option<NominalInstanceClass<'db>>> {
        self.unavailable(RecursiveNormalizationOperation::NonTupleInstance)
            .await
    }
}

#[cfg(test)]
type NormalizationTupleBuffer<'db> = TupleBuffer<'db, observations::Buffer>;
#[cfg(not(test))]
type NormalizationTupleBuffer<'db> = TupleBuffer<'db>;

impl<'run, 'db: 'run, R: RetainedNormalizationSource<'run, 'db>> TupleNormalizationEffects<'db>
    for SourceNormalization<'_, 'run, 'db, R>
{
    type Error = RunError;
    type Buffer = NormalizationTupleBuffer<'db>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        self.local(1, 0, || self.check_environment(env)).await?;
        self.source.effects().program(env).await
    }

    async fn spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        self.source.effects().tuple_spec(tuple).await
    }

    async fn normalize_spec(
        &self,
        spec: &TupleSpec<'db>,
        env: &ProgramEnvironment<'db>,
        program: Program<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> RunResult<Option<TupleSpec<'db>>> {
        let resolved = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                self.check_environment(env)?;
                Ok(ProgramEnvironment::from_program(program))
            })
            .await?;
        let env = self.source.effects().retain_environment(&resolved).await?;
        let effects = SourceNormalization {
            endpoint: self.endpoint,
            env,
            source: self.source,
        };
        tuple_spec_normalize_with(
            spec,
            env,
            divergent,
            nested,
            &effects,
            TupleNormalizationFacts,
        )
        .await
    }

    async fn normalize_child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.child(request, env).await
    }

    async fn next_element(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(2, 0, || Ok(elements.next().copied())).await
    }

    async fn new_buffer(&self, capacity: usize) -> RunResult<Self::Buffer> {
        TupleBuffer::new(self.endpoint, &self.source.effects(), capacity, || {
            #[cfg(test)]
            {
                observations::Buffer::new()
            }
        })
        .await
    }

    async fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> RunResult<()> {
        #[cfg(test)]
        let db = self.source.effects().db();
        buffer
            .push(self.endpoint, &self.source.effects(), ty, |_observer, _len| {
                #[cfg(test)]
                _observer.pushed(db, _len);
            })
            .await
    }

    async fn set_variable(
        &self,
        buffer: &mut Self::Buffer,
        variable: VariableSegment<'db>,
    ) -> RunResult<()> {
        buffer.start_variable(self.endpoint, variable).await
    }

    async fn finish_buffer(&self, buffer: &mut Self::Buffer) -> RunResult<TupleSpec<'db>> {
        #[cfg(test)]
        let db = self.source.effects().db();
        buffer
            .finish(self.endpoint, &self.source.effects(), |_observer| {
                #[cfg(test)]
                _observer.completed(db);
            })
            .await
    }

    async fn intern_tuple(
        &self,
        program: Program<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        self.source.effects().intern_tuple(program, spec).await
    }
}

#[cfg(test)]
pub(in crate::types) mod observations {
    use std::cell::Cell;

    use salsa::execution_probe::{ExecutionWork, RunResult, TaskEndpoint};

    use crate::{Db, ProgramEnvironment};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum StopKind {
        Work,
        Bytes,
        Cancel,
        Panic,
    }

    #[derive(Clone, Copy)]
    pub(in crate::types) struct Stop {
        pub(in crate::types) child: usize,
        pub(in crate::types) kind: StopKind,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum Event {
        Root {
            environment: usize,
        },
        ChildQueued {
            child: usize,
            environment: usize,
        },
        ChildEntered {
            child: usize,
            environment: usize,
            live_buffers: usize,
            remaining: Option<usize>,
        },
        ChildDropped {
            child: usize,
            live_buffers: usize,
        },
        BufferCreated {
            buffer: usize,
        },
        BufferPushed {
            buffer: usize,
            len: usize,
            remaining: Option<usize>,
        },
        BufferFinished {
            buffer: usize,
            remaining: Option<usize>,
        },
        BufferDropped {
            buffer: usize,
        },
    }

    #[derive(Clone, Copy, Debug)]
    pub(in crate::types) struct Snapshot {
        pub(in crate::types) children: usize,
        pub(in crate::types) started: usize,
        pub(in crate::types) dropped: usize,
        pub(in crate::types) buffers: usize,
        pub(in crate::types) live_buffers: usize,
        pub(in crate::types) dropped_buffers: usize,
        pub(in crate::types) events: [Option<Event>; 256],
        pub(in crate::types) event_count: usize,
    }

    impl Snapshot {
        const fn new() -> Self {
            Self {
                children: 0,
                started: 0,
                dropped: 0,
                buffers: 0,
                live_buffers: 0,
                dropped_buffers: 0,
                events: [None; 256],
                event_count: 0,
            }
        }
    }

    thread_local! {
        static SNAPSHOT: Cell<Snapshot> = const { Cell::new(Snapshot::new()) };
        static STOP: Cell<Option<Stop>> = const { Cell::new(None) };
    }

    pub(in crate::types) fn reset(stop: Option<Stop>) {
        SNAPSHOT.set(Snapshot::new());
        STOP.set(stop);
    }

    pub(in crate::types) fn snapshot() -> Snapshot {
        SNAPSHOT.get()
    }

    fn record(event: Event) {
        let mut snapshot = SNAPSHOT.get();
        if let Some(slot) = snapshot.events.get_mut(snapshot.event_count) {
            *slot = Some(event);
        }
        snapshot.event_count += 1;
        SNAPSHOT.set(snapshot);
    }

    pub(super) fn root(_db: &dyn Db, env: &ProgramEnvironment<'_>) {
        record(Event::Root {
            environment: std::ptr::from_ref(env).addr(),
        });
    }

    pub(super) struct Child(usize);

    impl Child {
        pub(super) fn new(env: &ProgramEnvironment<'_>) -> Self {
            let mut snapshot = SNAPSHOT.get();
            snapshot.children += 1;
            SNAPSHOT.set(snapshot);
            record(Event::ChildQueued {
                child: snapshot.children,
                environment: std::ptr::from_ref(env).addr(),
            });
            Self(snapshot.children)
        }

        pub(super) fn enter<'run, 'db: 'run>(
            &self,
            db: &'db dyn Db,
            endpoint: &TaskEndpoint<'run, 'db>,
            env: &ProgramEnvironment<'db>,
        ) -> RunResult<()> {
            let mut snapshot = SNAPSHOT.get();
            snapshot.started += 1;
            SNAPSHOT.set(snapshot);
            record(Event::ChildEntered {
                child: self.0,
                environment: std::ptr::from_ref(env).addr(),
                live_buffers: snapshot.live_buffers,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            });
            if let Some(stop) = STOP.get()
                && stop.child == self.0
            {
                STOP.set(None);
                match stop.kind {
                    StopKind::Work => endpoint.admit_work(usize::MAX)?,
                    StopKind::Bytes => endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: usize::MAX,
                    })?,
                    StopKind::Cancel => db.cancellation_token().cancel(),
                    StopKind::Panic => panic!("recursive normalization child panic"),
                }
            }
            endpoint.check_completion()
        }
    }

    impl Drop for Child {
        fn drop(&mut self) {
            let mut snapshot = SNAPSHOT.get();
            snapshot.dropped += 1;
            SNAPSHOT.set(snapshot);
            record(Event::ChildDropped {
                child: self.0,
                live_buffers: snapshot.live_buffers,
            });
        }
    }

    pub(super) struct Buffer(usize);

    impl Buffer {
        pub(super) fn new() -> Self {
            let mut snapshot = SNAPSHOT.get();
            snapshot.buffers += 1;
            snapshot.live_buffers += 1;
            SNAPSHOT.set(snapshot);
            record(Event::BufferCreated {
                buffer: snapshot.buffers,
            });
            Self(snapshot.buffers)
        }

        pub(super) fn pushed(&mut self, db: &dyn Db, len: usize) {
            record(Event::BufferPushed {
                buffer: self.0,
                len,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            });
        }

        pub(super) fn completed(&mut self, db: &dyn Db) {
            record(Event::BufferFinished {
                buffer: self.0,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            });
        }
    }

    impl Drop for Buffer {
        fn drop(&mut self) {
            let mut snapshot = SNAPSHOT.get();
            snapshot.live_buffers -= 1;
            snapshot.dropped_buffers += 1;
            SNAPSHOT.set(snapshot);
            record(Event::BufferDropped { buffer: self.0 });
        }
    }
}
