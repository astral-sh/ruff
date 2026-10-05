//! Expand constructor member callables with canonical signatures and the caller's guard.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

#[cfg(test)]
use crate::types::infer::source_runtime::tests::constructor_preparation as observations;

use super::callable_guard::GuardedPreparationEffects;
use super::{SourceAccess, SourceEffects, SourceOperation, storage};
use crate::types::call::bind::source::initial_overloads_quote;
use crate::types::call::bindings::{BindingsEffects, InstanceBindingsWork};
use crate::types::call::preparation::bound_method::{
    BoundMethodBindingEffects, BoundMethodOverloads, BoundMethodPreparationOperation,
    BoundMethodReceivers,
};
use crate::types::call::{Bindings, CallableBinding};
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::instance::Protocol;
use crate::types::method::BoundMethodReceiver;
use crate::types::{
    BoundMethodType, CallableRecursionGuard, CallableSignature, CallableType, DescriptorOrigin,
    MemberLookupResult, Signature, Type,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BindingsEffects<'db>
    for GuardedPreparationEffects<'_, '_, '_, 'run, 'db, A>
{
    type Error = RunError;

    fn checkpoint(&self, _db: &dyn Db, _work: InstanceBindingsWork) -> RunResult<()> {
        self.source.access.endpoint().admit_work(1)?;
        self.source.access.endpoint().check_completion()
    }

    async fn call_member(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source.guarded_callable_member(ty, guard).await
    }

    async fn bindings_from_descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> RunResult<Bindings<'db>> {
        GuardedPreparationEffects::bindings_from_descriptor(self, db, env, ty, origin).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Returns the receiver type used for `typing.Self`, using an instance for class methods.
    pub(super) async fn constructor_typing_self_type(
        &self,
        method: BoundMethodType<'db>,
    ) -> RunResult<Type<'db>> {
        let fields = self.access.endpoint().field_request_context();
        let receiver = self.field(method.receiver_request(fields)).await?;
        let receiver = self
            .local(2, size_of::<Type<'db>>(), || match receiver {
                BoundMethodReceiver::Instance(receiver)
                | BoundMethodReceiver::Constrained { receiver, .. } => receiver,
            })
            .await?;
        if !self
            .field(method.field_requests(fields).class_method())
            .await?
        {
            return Ok(receiver);
        }
        let program = self.field(method.field_requests(fields).program()).await?;
        self.check_program(program)?;
        let instance = self
            .allocate_future(|| NamespaceLookupEffects::instance_approximation(self, receiver))
            .await?
            .await?;
        self.local(2, size_of::<Type<'db>>(), || {
            instance.unwrap_or(Type::unknown())
        })
        .await
    }

    /// Clones every stored overload into an initial binding, including an empty signature list.
    pub(super) async fn stored_callable_bindings(
        &self,
        callable_type: Type<'db>,
        callable: CallableType<'db>,
    ) -> RunResult<Bindings<'db>> {
        let signature = self
            .field(
                callable
                    .field_requests(self.access.endpoint().field_request_context())
                    .signatures(),
            )
            .await?;
        self.initial_callable_bindings(callable_type, signature, None)
            .await
    }

    /// Builds bindings from borrowed canonical signatures and an optional captured receiver.
    async fn initial_callable_bindings(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
        receiver: Option<Type<'db>>,
    ) -> RunResult<Bindings<'db>> {
        let quote_work = Self::checked(
            signature
                .overloads
                .len()
                .checked_mul(16)
                .and_then(|work| work.checked_add(4)),
        )?;
        let quote = self
            .local(quote_work, 0, || {
                initial_overloads_quote(&signature.overloads)
            })
            .await?
            .ok_or(RunError::Contract(
                "initial callable binding quotation overflow",
            ))?;
        self.local(1, size_of::<Option<Bindings<'db>>>(), || ())
            .await?;
        let mut owner = None;
        let bytes = Self::checked(quote.bytes.checked_add(size_of::<Option<Bindings<'db>>>()))?;
        #[cfg(test)]
        observations::observe_before(observations::Stage::InitialSignature);
        self.local(quote.work, bytes, || {
            let mut callable = CallableBinding::from_signature(callable_type, signature);
            callable.bound_type = receiver;
            owner = Some(Bindings::from(callable));
            #[cfg(test)]
            observations::observe_after(observations::Stage::InitialSignature);
        })
        .await?;
        owner.ok_or(RunError::Contract(
            "initial callable bindings were not constructed",
        ))
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BoundMethodBindingEffects<'db>
    for GuardedPreparationEffects<'_, '_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn callable(
        &self,
        _db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .field(
                method
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .func(),
            )
            .await
    }

    async fn unbound_signatures(
        &self,
        _db: &'db dyn Db,
        callable: Type<'db>,
    ) -> RunResult<Option<&'db CallableSignature<'db>>> {
        self.source.work(1).await?;
        match callable {
            Type::FunctionLiteral(function) => {
                self.source.function_signature(function).await.map(Some)
            }
            Type::Callable(callable) => self
                .source
                .field(
                    callable
                        .field_requests(self.source.access.endpoint().field_request_context())
                        .signatures(),
                )
                .await
                .map(Some),
            _ => Ok(None),
        }
    }

    async fn receivers(
        &self,
        _db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> RunResult<BoundMethodReceivers<'db>> {
        let receiver = self
            .source
            .field(method.receiver_request(self.source.access.endpoint().field_request_context()))
            .await?;
        self.source
            .local(
                2,
                size_of::<BoundMethodReceivers<'db>>() * 2,
                || match receiver {
                    BoundMethodReceiver::Instance(receiver) => BoundMethodReceivers {
                        self_instance: receiver,
                        signature_receiver: receiver,
                    },
                    BoundMethodReceiver::Constrained {
                        receiver,
                        constraint,
                    } => BoundMethodReceivers {
                        self_instance: receiver,
                        signature_receiver: constraint,
                    },
                },
            )
            .await
    }

    async fn protocol_receiver_is_specialized(
        &self,
        _db: &'db dyn Db,
        receiver: Type<'db>,
        signature: &CallableSignature<'db>,
    ) -> RunResult<bool> {
        let has_origin = self
            .source
            .local(4, size_of::<bool>(), || {
                receiver
                    .as_protocol_instance()
                    .is_some_and(|protocol| match protocol.inner {
                        Protocol::FromClass(_) | Protocol::Materialized(_) => true,
                        Protocol::Synthesized(_) => false,
                    })
            })
            .await?;
        if !has_origin {
            return Ok(false);
        }
        let work = SourceEffects::<'_, 'run, 'db, A>::checked(
            signature
                .overloads
                .len()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.source
            .local(work, size_of::<bool>(), || {
                signature
                    .overloads
                    .iter()
                    .all(Signature::has_implicit_positional_receiver_annotation)
            })
            .await
    }

    async fn from_signature(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
        receiver: Type<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .initial_callable_bindings(callable_type, signature, Some(receiver))
            .await
    }

    async fn bake_receiver(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> RunResult<()> {
        self.source.bake_constructor_receivers(env, bindings).await
    }

    async fn empty_overloads(&self) -> RunResult<BoundMethodOverloads<'db>> {
        self.source
            .local(
                2,
                size_of::<BoundMethodOverloads<'db>>() * 2,
                BoundMethodOverloads::new,
            )
            .await
    }

    async fn receiver_determines_typevar(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
    ) -> RunResult<bool> {
        let needs_search = self
            .source
            .local(6, size_of::<bool>(), || {
                signature.has_explicit_positional_receiver_annotation()
                    && signature.generic_context.is_some()
                    && signature.definition().is_some()
            })
            .await?;
        if !needs_search {
            return Ok(false);
        }
        self.source
            .unavailable(SourceOperation::BoundMethodPreparation(
                BoundMethodPreparationOperation::ReceiverTypevarSearch,
            ))
            .await
    }

    async fn specialize_receiver(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _signature: &Signature<'db>,
        _method: BoundMethodType<'db>,
        _receiver: Type<'db>,
    ) -> RunResult<Option<CallableSignature<'db>>> {
        self.source
            .unavailable(SourceOperation::BoundMethodPreparation(
                BoundMethodPreparationOperation::ReceiverSpecialization,
            ))
            .await
    }

    async fn append_overloads(
        &self,
        output: &mut BoundMethodOverloads<'db>,
        signatures: &[Signature<'db>],
    ) -> RunResult<()> {
        self.source.work(16).await?;
        let required =
            SourceEffects::<'_, 'run, 'db, A>::checked(output.len().checked_add(signatures.len()))?;
        let grows = required > output.capacity();
        let replacement_capacity = if grows {
            SourceEffects::<'_, 'run, 'db, A>::checked(output.capacity().checked_mul(2))?
                .max(required)
                .max(4)
        } else {
            0
        };
        Layout::array::<Signature<'db>>(replacement_capacity)
            .map_err(|_| RunError::Contract("bound-method overload layout overflow"))?;
        let additional_capacity = if grows {
            SourceEffects::<'_, 'run, 'db, A>::checked(
                replacement_capacity.checked_sub(output.len()),
            )?
        } else {
            0
        };
        let retired_capacity = if grows && output.spilled() {
            output.capacity()
        } else {
            0
        };
        let relocated_bytes = if grows {
            SourceEffects::<'_, 'run, 'db, A>::checked(
                output.len().checked_mul(size_of::<Signature<'db>>()),
            )?
        } else {
            0
        };
        let mut quote = storage::sequence_merge::<Signature<'db>>(
            output.len(),
            output.capacity(),
            signatures.len(),
        )
        .ok_or(RunError::Contract(
            "bound-method overload growth quotation overflow",
        ))?;
        let scans = SourceEffects::<'_, 'run, 'db, A>::checked(
            signatures
                .len()
                .checked_mul(16)
                .and_then(|work| work.checked_add(4)),
        )?;
        let additions = self
            .source
            .local(scans, 0, || {
                let mut work = Some(0usize);
                let mut bytes = Some(0usize);
                for signature in signatures {
                    work = work.and_then(|work| {
                        signature
                            .retirement_work()
                            .and_then(|retirement| work.checked_add(retirement))
                            .and_then(|work| work.checked_add(16))
                    });
                    bytes = bytes.and_then(|bytes| {
                        signature
                            .clone_requested_bytes()
                            .and_then(|clone| bytes.checked_add(clone))
                            .and_then(|bytes| bytes.checked_add(size_of::<Signature<'db>>() * 2))
                    });
                }
                (work, bytes)
            })
            .await?;
        let added_work = SourceEffects::<'_, 'run, 'db, A>::checked(additions.0)?;
        let added_bytes = SourceEffects::<'_, 'run, 'db, A>::checked(additions.1)?;
        quote.work = SourceEffects::<'_, 'run, 'db, A>::checked(
            quote
                .work
                .checked_add(added_work)
                .and_then(|work| work.checked_add(retired_capacity))
                .and_then(|work| work.checked_add(replacement_capacity)),
        )?;
        quote.bytes = SourceEffects::<'_, 'run, 'db, A>::checked(
            quote
                .bytes
                .checked_add(added_bytes)
                .and_then(|bytes| bytes.checked_add(relocated_bytes)),
        )?;
        #[cfg(test)]
        observations::observe_before(observations::Stage::InitialSignature);
        self.source
            .local(quote.work, quote.bytes, || {
                if grows {
                    output.reserve_exact(additional_capacity);
                }
                output.extend(signatures.iter().cloned());
                #[cfg(test)]
                observations::observe_after(observations::Stage::InitialSignature);
            })
            .await
    }

    async fn finish_overloads(
        &self,
        callable_type: Type<'db>,
        receiver: Type<'db>,
        overloads: BoundMethodOverloads<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .local(
                2,
                size_of::<Option<BoundMethodOverloads<'db>>>() + size_of::<Option<Bindings<'db>>>(),
                || (),
            )
            .await?;
        let mut input = Some(overloads);
        let mut result = None;
        let count = self
            .source
            .local(1, size_of::<usize>(), || {
                input.as_deref().map(<[_]>::len).unwrap_or(0)
            })
            .await?;
        let scans = SourceEffects::<'_, 'run, 'db, A>::checked(
            count.checked_mul(16).and_then(|work| work.checked_add(4)),
        )?;
        let quote = self
            .source
            .local(scans, 0, || {
                input.as_deref().and_then(initial_overloads_quote)
            })
            .await?
            .ok_or(RunError::Contract(
                "bound-method binding quotation overflow",
            ))?;
        let bytes = SourceEffects::<'_, 'run, 'db, A>::checked(
            quote.bytes.checked_add(size_of::<Option<Bindings<'db>>>()),
        )?;
        self.source
            .local(quote.work, bytes, || {
                if let Some(overloads) = input.take() {
                    result = Some(Bindings::from(
                        CallableBinding::from_overloads(callable_type, overloads)
                            .with_bound_type(receiver),
                    ));
                }
            })
            .await?;
        result.ok_or(RunError::Contract(
            "bound-method bindings were not constructed",
        ))
    }

    async fn upcast(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _callable_type: Type<'db>,
        _method: BoundMethodType<'db>,
        _unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .unavailable(SourceOperation::BoundMethodPreparation(
                BoundMethodPreparationOperation::CallableInstanceUpcast,
            ))
            .await
    }
}
